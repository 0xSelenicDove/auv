"""Offline Task044 six-phase adapter checks; cluster and AUV edges are mocked."""

from contextlib import nullcontext, redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import k8s_phase_adapter as cluster  # noqa: E402
import k8s_v2_task044_adapter as adapter  # noqa: E402


def configuration(root: Path) -> dict:
    paths = {}
    for name in ("kubeconfig", "guest_auv_binary", "host_auv_binary"):
        path = root / name
        path.write_bytes(name.encode())
        paths[name] = str(path)
    upstream = root / "upstream"
    upstream.mkdir()
    task_source = root / "task_044.py"
    task_source.write_text("# offline fixture\n")
    asset = root / "promo_video.mp4"
    asset.write_bytes(b"fixture")
    return {
        "batch_id": "v2-control", "episode_id": "task044-control", "namespace": "bench",
        "kubeconfig": paths["kubeconfig"], "context": "ihome", "node": "liet-gpu-1",
        "runtime_pod": "task044-vm", "runtime_service": "task044-service", "proxy_pod": "task044-proxy",
        "proxy_image": "example/proxy@sha256:" + "a" * 64,
        "base_pvc": adapter.V2_HOT_PVC, "base_qcow_sha256": adapter.V2_BASE_QCOW_SHA256,
        "guest_auv_binary": paths["guest_auv_binary"], "host_auv_binary": paths["host_auv_binary"],
        "host_auv_sha256": "b" * 64,
        "upstream_checkout": str(upstream), "task_source": str(task_source), "asset": str(asset),
        "setup_local_port": 25044, "auv_local_port": 28044,
    }


class Task044AdapterTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.config = configuration(self.root)
        self.config_path = self.root / "config.json"
        self.config_path.write_text(json.dumps(self.config))
        self.directory = self.root / self.config["episode_id"]
        self.directory.mkdir()
        self.episode = adapter.Episode(self.config, self.directory)

    def digests(self, path):
        return {self.config["guest_auv_binary"]: adapter.GUEST_AUV_SHA256,
                self.config["host_auv_binary"]: self.config["host_auv_sha256"],
                str(self.config_path.resolve()): "e" * 64}[str(path)]

    def test_manifest_pins_six_phases_and_capture_only(self):
        with patch.object(cluster, "sha256", side_effect=self.digests), \
             patch.object(adapter.task044, "load_task") as load_task, \
             patch.object(adapter.subprocess, "run", return_value=MagicMock(returncode=0)):
            manifest = adapter.manifest(self.config_path)
        load_task.assert_called_once()
        episode = manifest["episodes"][0]
        identity = episode["identity"]
        self.assertEqual(identity["benchmark"], "OSWorld-V2.1")
        self.assertEqual(identity["task_id"], "044")
        self.assertEqual(identity["topology"], "paired-remote-capture-only-negative-control")
        self.assertEqual(identity["asset_sha256"], adapter.task044.ASSET_SHA256)
        self.assertEqual(identity["opencv_python"], adapter.task044.OPENCV_DIST_VERSION)
        self.assertEqual(identity["qcow2"], "sha256:" + adapter.V2_BASE_QCOW_SHA256)
        self.assertEqual(set(episode["phases"]), set(cluster.PHASES))
        for name in cluster.PHASES:
            self.assertEqual(episode["phases"][name]["argv"][2:4], ["phase", name])
            self.assertEqual(episode["phases"][name]["argv"][-2:], ["--config-sha256", "e" * 64])

    def test_config_rejects_action_command_wrong_base_and_source_drift(self):
        self.config["action_argv"] = ["xdotool", "click", "1"]
        self.config_path.write_text(json.dumps(self.config))
        with patch.object(adapter.task044, "load_task") as load_task:
            with self.assertRaisesRegex(ValueError, "only the pinned"):
                adapter.load_config(self.config_path)
        load_task.assert_not_called()
        del self.config["action_argv"]
        self.config["base_qcow_sha256"] = "0" * 64
        self.config_path.write_text(json.dumps(self.config))
        with self.assertRaisesRegex(ValueError, "V2 Task044 base"):
            adapter.load_config(self.config_path)
        self.config["base_qcow_sha256"] = adapter.V2_BASE_QCOW_SHA256
        self.config_path.write_text(json.dumps(self.config))
        with patch.object(cluster, "sha256", side_effect=self.digests), \
             patch.object(adapter.task044, "load_task", side_effect=ValueError("asset SHA256 mismatch")):
            with self.assertRaisesRegex(ValueError, "asset SHA256 mismatch"):
                adapter.load_config(self.config_path)

    def test_guest_host_and_decoder_pin_fail_closed(self):
        with patch.object(cluster, "sha256", return_value="0" * 64):
            with self.assertRaisesRegex(ValueError, "guest Ubuntu AUV"):
                adapter.load_config(self.config_path)
        with patch.object(cluster, "sha256", side_effect=lambda path: adapter.GUEST_AUV_SHA256 if str(path) == self.config["guest_auv_binary"] else "0" * 64):
            with self.assertRaisesRegex(ValueError, "paired host AUV"):
                adapter.load_config(self.config_path)
        with patch.object(cluster, "sha256", side_effect=self.digests), \
             patch.object(adapter.task044, "load_task", side_effect=RuntimeError("OpenCV mismatch")):
            with self.assertRaisesRegex(RuntimeError, "OpenCV mismatch"):
                adapter.load_config(self.config_path)

    def test_evaluator_preserves_raw_float_and_rejects_mismatched_bridge(self):
        cluster.write_json(self.directory / "paired-device.json", {"guest_auv_sha256": adapter.GUEST_AUV_SHA256})
        raw = {"phase": "evaluate", "task_sha256": adapter.task044.TASK_SHA256,
               "upstream_revision": adapter.task044.UPSTREAM_REV,
               "asset_sha256": adapter.task044.ASSET_SHA256,
               "opencv_python": adapter.task044.OPENCV_DIST_VERSION,
               "result": 0.6000000000000001, "score": 0.6000000000000001}
        output = io.StringIO()
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run", return_value=json.dumps(raw)) as run, redirect_stdout(output):
            self.episode.evaluator("evaluate")
        self.assertEqual(json.loads(output.getvalue())["result"], raw["result"])
        self.assertIn("v2_task044_evaluator.py", run.call_args.args[0][1])
        self.assertEqual(run.call_args.args[0][0], sys.executable)
        raw["score"] = 0.5
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run", return_value=json.dumps(raw)):
            with self.assertRaisesRegex(ValueError, "raw float score"):
                self.episode.evaluator("evaluate")

    def test_action_is_inherited_auv_capture_with_byte_checked_png(self):
        cluster.write_json(self.directory / "paired-device.json", {"guest_auv_sha256": adapter.GUEST_AUV_SHA256,
                                                                 "device_id": "observed-device"})
        png = self.directory / "final-screenshot.png"

        def capture(_episode):
            png.write_bytes(b"\x89PNG\r\n\x1a\nfixture")
            evidence = {"run_ids": ["run-1"],
                        "final_artifact": {"path": png.name, "sha256": cluster.sha256(png)}}
            cluster.write_json(self.directory / "action_evidence.json", evidence)
            print(json.dumps(evidence))

        output = io.StringIO()
        with patch.object(self.episode, "assert_identity"), \
             patch.object(cluster.Episode, "action", autospec=True, side_effect=capture) as inherited, \
             redirect_stdout(output):
            self.episode.action()
        inherited.assert_called_once_with(self.episode)
        self.assertEqual(json.loads(output.getvalue()), json.loads((self.directory / "action_evidence.json").read_text()))
        with patch.object(self.episode, "assert_identity"), \
             patch.object(cluster.Episode, "action", autospec=True) as inherited:
            with self.assertRaisesRegex(FileExistsError, "stale action evidence"):
                self.episode.action()
        inherited.assert_not_called()

    def test_v2_install_and_reset_keep_shared_uid_safe_lifecycle(self):
        self.assertIs(adapter.Episode.boot, cluster.Episode.boot)
        self.assertIs(adapter.Episode.install, cluster.Episode.install)
        self.assertIs(adapter.Episode.reset, cluster.Episode.reset)
        self.assertEqual(adapter.Episode.guest_auv_sha256, adapter.GUEST_AUV_SHA256)
        self.assertIn("osworld-public-evaluation", adapter.APT_UPDATE[-1])
        for name in ("guest_auv_binary", "host_auv_binary", "task_source", "asset"):
            Path(self.config[name]).unlink()
        with patch.object(adapter.task044, "load_task") as load_task:
            self.assertEqual(adapter.load_config(self.config_path, reset=True), self.config)
        load_task.assert_not_called()
        cluster.write_json(self.episode.owned_path, [{"kind": "pod", "name": self.config["runtime_pod"], "uid": "old"}])
        with patch.object(self.episode, "api_proxy", return_value=nullcontext("http://127.0.0.1:1")), \
             patch.object(self.episode, "get", return_value={"metadata": {"uid": "new", "labels": self.episode._labels("qemu")}}), \
             patch.object(self.episode, "request_deletion") as delete:
            with self.assertRaisesRegex(ValueError, "replaced"):
                self.episode.reset()
        delete.assert_not_called()


if __name__ == "__main__":
    unittest.main()
