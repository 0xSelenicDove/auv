"""Offline Task099 boundary tests; no guest, GUI input, or model calls."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError


BRIDGE_PATH = Path(__file__).resolve().parents[1] / "v2_task099_evaluator.py"
spec = importlib.util.spec_from_file_location("v2_task099_evaluator", BRIDGE_PATH)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)

PILOT = Path("/tmp/auv-osworld-batch-20261005")
UPSTREAM = PILOT / "v2"
TASK_SOURCE = PILOT / "v2-task-classes" / "task_099.py"
ASSET = PILOT / "v2-assets-pilot" / "task_099" / "my_image.png"
HAS_PINNED_SOURCES = UPSTREAM.exists() and TASK_SOURCE.exists() and ASSET.exists()


class Response:
    status = 200

    def __init__(self, content: bytes):
        self.content = content

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self):
        return self.content


class Task099BridgeTest(unittest.TestCase):
    def test_unpinned_revision_and_task_bytes_are_rejected_before_guest_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            task = root / "task_099.py"
            task.write_text("# mutated")
            with patch.object(bridge.subprocess, "check_output", return_value="wrong-revision\n"):
                with self.assertRaisesRegex(ValueError, "not pinned"):
                    bridge.load_task(root, task, root / "image.png")
            with patch.object(bridge.subprocess, "check_output", side_effect=[bridge.UPSTREAM_REV + "\n", ""]):
                with self.assertRaisesRegex(ValueError, "SHA256"):
                    bridge.load_task(root, task, root / "image.png")

    def test_transport_rejects_non_loopback_and_unreviewed_paths(self):
        with self.assertRaisesRegex(ValueError, "loopback"):
            bridge.FileTransport("http://example.com:5000")
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        with self.assertRaisesRegex(ValueError, "unreviewed"):
            transport.get_file("/tmp/arbitrary")

    def test_http_404_is_missing_file_but_connection_failure_is_not(self):
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        with patch.object(bridge, "urlopen", side_effect=HTTPError("/file", 404, "Not Found", {}, None)):
            self.assertIsNone(transport.get_file(bridge.POSITION_VM_PATH))
            self.assertIsNone(transport.transport_error)
        with patch.object(bridge, "urlopen", side_effect=URLError("connection refused")):
            with self.assertRaisesRegex(RuntimeError, "transport failed"):
                transport.get_file(bridge.POSITION_VM_PATH)
        self.assertIsNotNone(transport.transport_error)

    def test_upload_confirms_exact_guest_path_and_readback_hash(self):
        transport = bridge.FileTransport("http://127.0.0.1:5000")
        image = b"test-image"
        with patch.object(bridge, "ASSET_SHA256", hashlib.sha256(image).hexdigest()), \
                patch.object(bridge, "ASSET_BYTES", len(image)), \
                patch.object(bridge, "urlopen", side_effect=[
                    Response(f"File Uploaded: {len(image)} bytes".encode()), Response(image),
                ]) as urlopen:
            transport.download([{"url": "/local/my_image.png", "path": bridge.IMAGE_VM_PATH}],
                               image, Path("/local/my_image.png"))
        upload = urlopen.call_args_list[0].args[0]
        self.assertEqual(upload.full_url, "http://127.0.0.1:5000/setup/upload")
        self.assertIn(bridge.IMAGE_VM_PATH.encode(), upload.data)
        self.assertIn(image, upload.data)
        self.assertEqual(urlopen.call_args_list[1].args[0].full_url, "http://127.0.0.1:5000/file")
        with self.assertRaisesRegex(ValueError, "unreviewed"):
            transport.download([{"url": "/local/my_image.png", "path": "/tmp/wrong"}],
                               image, Path("/local/my_image.png"))

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 sources and gated Task099 asset not available")
    def test_exact_pins_setup_path_and_original_source_bodies(self):
        task, image = bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)
        self.assertEqual(hashlib.sha256(image).hexdigest(), bridge.ASSET_SHA256)
        self.assertEqual(task.setup.__code__.co_filename, str(TASK_SOURCE))
        self.assertEqual(task.evaluate.__code__.co_filename, str(TASK_SOURCE))
        self.assertEqual(len(image), bridge.ASSET_BYTES)

        class Setup:
            files = None

            def download(self, files):
                self.files = files

        setup = Setup()
        task.setup(setup)
        self.assertEqual(setup.files, [{"url": str(ASSET), "path": bridge.IMAGE_VM_PATH}])

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 sources and gated Task099 asset not available")
    def test_original_score_and_legitimate_missing_answer(self):
        task, _ = bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)
        with tempfile.TemporaryDirectory() as directory:
            transport = bridge.FileTransport("http://127.0.0.1:5000")
            with patch.object(bridge, "urlopen", return_value=Response(b"3.1541992,101.717366\n")) as urlopen:
                result = bridge.evaluate(task, transport, Path(directory))
            self.assertEqual(result["score"], 1.0)
            self.assertEqual(result["partial_scores"]["distance"]["score"], 1.0)
            self.assertEqual(urlopen.call_args.args[0].full_url, "http://127.0.0.1:5000/file")
            self.assertIn(b"position.txt", urlopen.call_args.args[0].data)

            transport = bridge.FileTransport("http://127.0.0.1:5000")
            with patch.object(bridge, "urlopen", side_effect=HTTPError("/file", 404, "Not Found", {}, None)):
                result = bridge.evaluate(task, transport, Path(directory))
            self.assertEqual(result["score"], 0.0)
            self.assertEqual(result["partial_scores"]["distance"]["weight"], 1.0)

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 sources and gated Task099 asset not available")
    def test_upstream_broad_catch_cannot_turn_transport_failure_into_zero_score(self):
        task, _ = bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)
        with tempfile.TemporaryDirectory() as directory:
            transport = bridge.FileTransport("http://127.0.0.1:5000")
            with patch.object(bridge, "urlopen", side_effect=URLError("connection refused")):
                with self.assertRaisesRegex(RuntimeError, "no score"):
                    bridge.evaluate(task, transport, Path(directory))

    @unittest.skipUnless(HAS_PINNED_SOURCES, "pinned V2.1 sources and gated Task099 asset not available")
    def test_getter_cache_failure_cannot_turn_valid_answer_into_zero_score(self):
        task, _ = bridge.load_task(UPSTREAM, TASK_SOURCE, ASSET)
        with tempfile.TemporaryDirectory() as directory:
            blocked_cache = Path(directory) / "cache-is-a-file"
            blocked_cache.write_text("not a directory")
            transport = bridge.FileTransport("http://127.0.0.1:5000")
            with patch.object(bridge, "urlopen", return_value=Response(b"3.1541992,101.717366\n")):
                with self.assertRaisesRegex(RuntimeError, "persist the fetched answer; no score"):
                    bridge.evaluate(task, transport, blocked_cache)


if __name__ == "__main__":
    unittest.main()
