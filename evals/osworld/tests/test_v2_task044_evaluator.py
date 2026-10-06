"""Offline Task044 bridge tests; no guest, GUI input, or model calls."""

from __future__ import annotations

import hashlib
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from auv_osworld import v2_task044_evaluator as bridge

PILOT = Path("/tmp/auv-osworld-batch-20261005")
UPSTREAM = PILOT / "v2"
TASK_SOURCE = PILOT / "v2-task-classes" / "task_044.py"
ASSET = PILOT / "v2-assets-pilot" / "task_044" / "promo_video.mp4"
HAS_PINNED_SOURCES = UPSTREAM.exists() and TASK_SOURCE.exists() and ASSET.exists()


class Response:
    def __init__(self, content: bytes, status: int = 200):
        self.content = content
        self.text = content.decode()
        self.status_code = status

    def raise_for_status(self):
        if self.status_code >= 400:
            raise bridge.requests.HTTPError(f"HTTP {self.status_code}")


class FakeCv2:
    __version__ = "4.8.1"
    CAP_PROP_FRAME_WIDTH = 3
    CAP_PROP_FRAME_HEIGHT = 4

    def __init__(self, source=(834, 1112), export=(834, 1112)):
        self.source = source
        self.export = export

    def VideoCapture(self, path):
        dimensions = self.source if str(path).endswith("promo_video_source.mp4") else self.export

        class Capture:
            def isOpened(self):
                return True

            def get(self, prop):
                return dimensions[0] if prop == FakeCv2.CAP_PROP_FRAME_WIDTH else dimensions[1]

            def release(self):
                pass

        return Capture()


def pinned_task(cv2=None):
    with patch.object(bridge, "_pinned_cv2", return_value=cv2 or FakeCv2()):
        return bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)[0]


def crop_xml(top: int, *, center: str = "0", service: str = "crop") -> bytes:
    position = (
        f"<property name='top'>{top}</property>"
        if service == "crop"
        else f"<property name='rect'>0 {top} 834 1112</property>"
    )
    return (
        f"<mlt><filter><property name='mlt_service'>{service}</property>{position}"
        f"<property name='center'>{center}</property></filter></mlt>"
    ).encode()


class Task044BridgeTest(unittest.TestCase):
    def test_unpinned_revision_and_task_bytes_are_rejected_before_guest_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            task = root / "task_044.py"
            task.write_text("# mutated")
            with patch.object(bridge.subprocess, "check_output", return_value="wrong-revision\n"):
                with self.assertRaisesRegex(ValueError, "not pinned"):
                    bridge.load_task(root, task, root / "video.mp4")
            with patch.object(bridge.subprocess, "check_output", side_effect=[bridge.UPSTREAM_REV + "\n", ""]):
                with self.assertRaisesRegex(ValueError, "SHA256"):
                    bridge.load_task(root, task, root / "video.mp4")

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 Task044 sources and gated video unavailable")
    def test_dirty_checkout_getter_and_asset_tampering_are_rejected(self):
        with patch.object(bridge.subprocess, "check_output", side_effect=[bridge.UPSTREAM_REV + "\n", " M changed\n"]):
            with self.assertRaisesRegex(ValueError, "uncommitted changes"):
                bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            getter = root / "desktop_env" / "evaluators" / "getters" / "file.py"
            getter.parent.mkdir(parents=True)
            getter.write_text("# mutated getter")
            with patch.object(bridge.subprocess, "check_output", side_effect=[bridge.UPSTREAM_REV + "\n", ""]):
                with self.assertRaisesRegex(ValueError, "SHA256"):
                    bridge.load_task(root, TASK_SOURCE, ASSET)
            asset = root / "promo_video.mp4"
            asset.write_bytes(b"mutated asset")
            with self.assertRaisesRegex(ValueError, "SHA256"):
                bridge.load_task(UPSTREAM, TASK_SOURCE, asset)

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 Task044 sources and gated video unavailable")
    def test_exact_sources_asset_setup_and_original_method_bodies(self):
        task = pinned_task()
        self.assertEqual(task.setup.__code__.co_filename, str(TASK_SOURCE))
        self.assertEqual(task.evaluate.__code__.co_filename, str(TASK_SOURCE))
        self.assertEqual(ASSET.stat().st_size, bridge.ASSET_BYTES)
        self.assertEqual(hashlib.sha256(ASSET.read_bytes()).hexdigest(), bridge.ASSET_SHA256)

        class Setup:
            files = None
            launch_command = None

            def download(self, files):
                self.files = files

            def launch(self, command):
                self.launch_command = command

        setup = Setup()
        task.setup(setup)
        self.assertEqual(setup.files, [{"url": str(ASSET), "path": bridge.SOURCE_VM_PATH}])
        self.assertEqual(setup.launch_command, ["shotcut"])

    def test_opencv_dependency_mismatch_is_explicit(self):
        def version(name):
            if name == "opencv-python":
                return "4.12.0.88"
            raise bridge.metadata.PackageNotFoundError(name)

        with patch.object(bridge.metadata, "version", side_effect=version), patch.dict(sys.modules, {"cv2": FakeCv2()}):
            with self.assertRaisesRegex(RuntimeError, "OpenCV mismatch"):
                bridge._pinned_cv2()

    def test_transport_allows_only_loopback_and_three_files(self):
        with self.assertRaisesRegex(ValueError, "loopback"):
            bridge.FileTransport("http://example.com:5000")
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        with self.assertRaisesRegex(ValueError, "unreviewed guest file"):
            transport.get_file("/tmp/arbitrary")
        with self.assertRaisesRegex(ValueError, "unreviewed launch"):
            transport.launch(["bash"])
        with self.assertRaisesRegex(ValueError, "unreviewed launch"):
            transport.launch(["shotcut"], shell=True)

    def test_upload_and_launch_require_exact_receipts(self):
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        video = b"test-video"
        with (
            patch.object(bridge, "ASSET_SHA256", hashlib.sha256(video).hexdigest()),
            patch.object(bridge, "ASSET_BYTES", len(video)),
            patch.object(
                bridge.requests,
                "post",
                side_effect=[
                    Response(f"File Uploaded: {len(video)} bytes".encode()),
                    Response(video),
                    Response(b"shotcut launched successfully"),
                ],
            ) as post,
        ):
            transport.download(
                [{"url": "/local/promo_video.mp4", "path": bridge.SOURCE_VM_PATH}],
                video,
                Path("/local/promo_video.mp4"),
            )
            transport.launch(["shotcut"])
        self.assertEqual(
            [call.args[0] for call in post.call_args_list],
            [
                "http://127.0.0.1:5000/setup/upload",
                "http://127.0.0.1:5000/file",
                "http://127.0.0.1:5000/setup/launch",
            ],
        )
        self.assertEqual(post.call_args_list[0].kwargs["files"]["file_data"][1], video)
        with self.assertRaisesRegex(ValueError, "unreviewed download"):
            transport.download(
                [{"url": "/local/promo_video.mp4", "path": "/tmp/wrong"}], video, Path("/local/promo_video.mp4")
            )
        with patch.object(bridge.requests, "post", return_value=Response(b"not launched")):
            with self.assertRaisesRegex(RuntimeError, "not confirmed"):
                transport.launch(["shotcut"])
        with patch.object(bridge.requests, "post", return_value=Response(b"shotcut launched successfully", status=500)):
            with self.assertRaisesRegex(RuntimeError, "not confirmed"):
                transport.launch(["shotcut"])
        with patch.object(bridge.requests, "post", return_value=Response(b"short upload")):
            with self.assertRaisesRegex(RuntimeError, "not confirmed"):
                transport.download(
                    [{"url": "/local/promo_video.mp4", "path": bridge.SOURCE_VM_PATH}],
                    video,
                    Path("/local/promo_video.mp4"),
                )

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 Task044 sources and gated video unavailable")
    def test_original_score_boundaries_and_legitimate_missing_outputs(self):
        task = pinned_task()
        source = b"source"

        def score(
            top,
            *,
            center="0",
            service="crop",
            size=100,
            source_dimensions=(834, 1112),
            export_dimensions=(834, 1112),
            missing=frozenset(),
        ):
            current_task = pinned_task(FakeCv2(source=source_dimensions, export=export_dimensions))
            files = {
                bridge.EXPORT_VM_PATH: b"x" * size,
                bridge.PROJECT_VM_PATH: crop_xml(top, center=center, service=service),
                bridge.SOURCE_VM_PATH: source,
            }
            transport = bridge.FileTransport("http://127.0.0.1:5000")

            def response(_url, *, data, timeout):
                path = data["file_path"]
                if path in missing:
                    return Response(b"", status=404)
                return Response(files[path])

            with (
                tempfile.TemporaryDirectory() as directory,
                patch.object(bridge, "ASSET_SHA256", hashlib.sha256(source).hexdigest()),
                patch.object(bridge.requests, "post", side_effect=response),
            ):
                return bridge.evaluate(current_task, transport, Path(directory))

        self.assertEqual(task.id, "044")
        self.assertEqual(score(75), 1.0)
        self.assertEqual(score(85), 1.0)
        self.assertAlmostEqual(score(90), 0.6)
        self.assertAlmostEqual(score(77, center="1"), 0.8)
        self.assertEqual(score(77, service="qtcrop", center="1"), 1.0)
        self.assertAlmostEqual(score(77, size=1_048_577), 0.8)
        self.assertEqual(score(77, size=1_048_576), 1.0)
        self.assertAlmostEqual(score(77, export_dimensions=(800, 1112)), 0.8)
        self.assertEqual(score(77, missing={bridge.EXPORT_VM_PATH}), 0.0)
        self.assertEqual(score(77, missing={bridge.PROJECT_VM_PATH}), 0.0)
        with self.assertRaisesRegex(RuntimeError, "source video is unavailable; no score"):
            score(77, missing={bridge.SOURCE_VM_PATH})
        with self.assertRaisesRegex(RuntimeError, "cannot decode pinned source video; no score"):
            score(77, source_dimensions=(0, 0))

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 Task044 sources and gated video unavailable")
    def test_getter_broad_catches_do_not_convert_transport_or_cache_errors_to_scores(self):
        task = pinned_task()
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(bridge.requests, "post", side_effect=bridge.requests.ConnectionError("connection refused")),
        ):
            with self.assertRaisesRegex(RuntimeError, "no score"):
                bridge.evaluate(task, transport, Path(directory))

        transport = bridge.FileTransport("http://127.0.0.1:5000")
        with tempfile.TemporaryDirectory() as directory:
            blocked_cache = Path(directory) / "cache-is-a-file"
            blocked_cache.write_text("not a directory")
            with patch.object(bridge.requests, "post", return_value=Response(b"exported")):
                with self.assertRaisesRegex(RuntimeError, "persist.*no score"):
                    bridge.evaluate(task, transport, blocked_cache)


if __name__ == "__main__":
    unittest.main()
