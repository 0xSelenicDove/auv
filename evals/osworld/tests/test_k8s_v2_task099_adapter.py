"""Offline V2.1 Task099 six-phase adapter checks; every cluster edge is mocked."""

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
import k8s_v2_task099_adapter as adapter  # noqa: E402


def configuration(root: Path) -> dict:
    paths = {}
    for name in ("kubeconfig", "guest_auv_binary", "host_auv_binary", "action_binary"):
        path = root / name
        path.write_bytes(name.encode())
        if name == "action_binary":
            path.chmod(0o700)
        paths[name] = str(path)
    upstream = root / "upstream"
    upstream.mkdir()
    task_source = root / "task_099.py"
    task_source.write_text("# offline fixture\n")
    asset = root / "my_image.png"
    asset.write_bytes(b"fixture")
    return {
        "batch_id": "v2-control", "episode_id": "task099-control", "namespace": "bench",
        "kubeconfig": paths["kubeconfig"], "context": "ihome", "node": "liet-gpu-1",
        "runtime_pod": "task099-vm", "runtime_service": "task099-service", "proxy_pod": "task099-proxy",
        "proxy_image": "example/proxy@sha256:" + "a" * 64,
        "base_pvc": "osworld-v2-hot", "base_qcow_sha256": adapter.V2_BASE_QCOW_SHA256,
        "guest_auv_binary": paths["guest_auv_binary"], "host_auv_binary": paths["host_auv_binary"],
        "host_auv_sha256": "b" * 64, "action_binary": paths["action_binary"],
        "action_binary_sha256": "c" * 64, "action_source_commit": "d" * 40,
        "upstream_checkout": str(upstream), "task_source": str(task_source), "asset": str(asset),
        "setup_local_port": 25099, "auv_local_port": 28099,
    }


class Task099AdapterTest(unittest.TestCase):
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
        path = str(path)
        return {self.config["guest_auv_binary"]: adapter.GUEST_AUV_SHA256,
                self.config["host_auv_binary"]: self.config["host_auv_sha256"],
                self.config["action_binary"]: self.config["action_binary_sha256"],
                str(self.config_path.resolve()): "e" * 64}[path]

    def test_manifest_pins_all_six_phases_and_cannot_select_gui_command(self):
        with patch.object(cluster, "sha256", side_effect=self.digests), \
             patch.object(adapter.task099, "load_task") as load_task, \
             patch.object(adapter.subprocess, "run", return_value=MagicMock(returncode=0)):
            manifest = adapter.manifest(self.config_path)
        load_task.assert_called_once()
        episode = manifest["episodes"][0]
        self.assertEqual(episode["identity"]["benchmark"], "OSWorld-V2.1")
        self.assertEqual(episode["identity"]["task_id"], "099")
        self.assertEqual(episode["identity"]["auv_source"], adapter.GUEST_AUV_SOURCE)
        self.assertEqual(episode["identity"]["action_binary_sha256"], self.config["action_binary_sha256"])
        self.assertEqual(episode["identity"]["qcow2"], "sha256:" + adapter.V2_BASE_QCOW_SHA256)
        self.assertEqual(set(episode["phases"]), set(cluster.PHASES))
        for name in cluster.PHASES:
            self.assertEqual(episode["phases"][name]["argv"][2:4], ["phase", name])
            self.assertEqual(episode["phases"][name]["argv"][-2:], ["--config-sha256", "e" * 64])
        self.assertEqual(adapter.FIXED_ACTIONS, [{"action_type": "DOUBLE_CLICK", "x": 1850, "y": 880}, "DONE"])

    def test_config_rejects_unlisted_commands_and_wrong_base_before_source_read(self):
        self.config["action_argv"] = ["xdotool", "click", "1"]
        self.config_path.write_text(json.dumps(self.config))
        with patch.object(adapter.task099, "load_task") as load_task:
            with self.assertRaisesRegex(ValueError, "only the pinned"):
                adapter.load_config(self.config_path)
        load_task.assert_not_called()
        del self.config["action_argv"]
        self.config["base_qcow_sha256"] = "0" * 64
        self.config_path.write_text(json.dumps(self.config))
        with self.assertRaisesRegex(ValueError, "V2 Task099 base"):
            adapter.load_config(self.config_path)
        self.config["base_qcow_sha256"] = adapter.V2_BASE_QCOW_SHA256
        self.config["base_pvc"] = "osworld-v1-hot"
        self.config_path.write_text(json.dumps(self.config))
        with self.assertRaisesRegex(ValueError, "V2 Task099 base"):
            adapter.load_config(self.config_path)

    def test_guest_host_action_and_task_source_pins_fail_closed(self):
        with patch.object(cluster, "sha256", side_effect=lambda path: "0" * 64):
            with self.assertRaisesRegex(ValueError, "guest Ubuntu AUV"):
                adapter.load_config(self.config_path)
        with patch.object(cluster, "sha256", side_effect=lambda path: adapter.GUEST_AUV_SHA256 if str(path) == self.config["guest_auv_binary"] else "0" * 64):
            with self.assertRaisesRegex(ValueError, "paired host AUV"):
                adapter.load_config(self.config_path)
        with patch.object(cluster, "sha256", side_effect=lambda path: self.digests(path) if str(path) != self.config["action_binary"] else "0" * 64):
            with self.assertRaisesRegex(ValueError, "foreground action binary"):
                adapter.load_config(self.config_path)
        with patch.object(cluster, "sha256", side_effect=self.digests), \
             patch.object(adapter.task099, "load_task", side_effect=ValueError("task source SHA256 mismatch")):
            with self.assertRaisesRegex(ValueError, "task source SHA256"):
                adapter.load_config(self.config_path)

    def test_v2_install_overrides_only_guest_pin_and_public_password(self):
        self.assertEqual(cluster.Episode.guest_auv_sha256, cluster.GUEST_AUV_SHA256)
        self.assertEqual(adapter.Episode.guest_auv_sha256, adapter.GUEST_AUV_SHA256)
        self.assertNotEqual(adapter.APT_UPDATE, cluster.GUEST_APT_UPDATE)
        self.assertIn("osworld-public-evaluation", adapter.APT_UPDATE[-1])
        self.assertEqual(self.episode.apt_install, adapter.APT_INSTALL)
        with patch.object(self.episode, "_post", return_value={"status": "success", "output": "", "error": "", "returncode": 0}) as post:
            self.episode.guest_control(adapter.APT_UPDATE)
            post.assert_called_once()
            with self.assertRaisesRegex(ValueError, "forbidden"):
                self.episode.guest_control(cluster.GUEST_APT_UPDATE)

    def test_install_retry_after_owner_token_failure_reaches_pairing(self):
        # ROOT CAUSE:
        # If owner-socket token creation fails after a successful upload and
        # launch, install retries upload to the same guest path. In the live
        # episode that retry returned HTTP 500 before pairing. The fix must
        # verify the installed bytes and avoid a second upload.
        uploads = 0
        token_calls = 0
        launched = False

        def run(command, **_kwargs):
            nonlocal uploads
            if command[0] == "curl":
                self.assertIn("file_path=/home/user/auv", command)
                self.assertIn(f"file_data=@{self.config['guest_auv_binary']}", command)
                uploads += 1
                if uploads > 1:
                    raise RuntimeError("curl exited 22: HTTP 500 /setup/upload")
                return ""
            self.assertEqual(command[0], self.config["host_auv_binary"])
            return json.dumps({"device_id": "paired-device"})

        def post(route, value):
            nonlocal token_calls, launched
            if route == "/setup/launch":
                launched = True
                self.assertIn("serve", value["command"])
                return "/home/user/auv serve launched successfully"
            self.assertEqual(route, "/setup/execute")
            command = value["command"]
            output = ""
            if command == ["test", "-e", "/home/user/auv"]:
                return {"status": "success", "output": "", "error": "", "returncode": 0 if uploads else 1}
            if command == ["sha256sum", "/home/user/auv"]:
                output = adapter.GUEST_AUV_SHA256 + "  /home/user/auv\n"
            elif command == ["/home/user/auv", "--version"]:
                output = "auv 0.0.28\n"
            elif command[-3:] == ["devices", "pair", "create-token"]:
                self.assertTrue(launched)
                token_calls += 1
                if token_calls == 1:
                    return {"status": "success", "output": "", "error": "owner socket unavailable",
                            "returncode": 1}
                output = "fixture-token\n"
            return {"status": "success", "output": output, "error": "", "returncode": 0}

        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run", side_effect=run), \
             patch.object(self.episode, "_post", side_effect=post), \
             redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "returncode=1"):
                self.episode.install()
            self.assertEqual(uploads, 1)
            self.assertEqual(token_calls, 1)
            self.assertFalse((self.directory / "paired-profiles.json").exists())
            self.episode.install()

        self.assertEqual(token_calls, 2)
        self.assertEqual(uploads, 1)
        self.assertEqual(json.loads((self.directory / "paired-device.json").read_text())["device_id"], "paired-device")

    def test_install_rejects_existing_guest_binary_with_wrong_hash_before_chmod_or_upload(self):
        commands = []

        def post(route, value):
            self.assertEqual(route, "/setup/execute")
            commands.append(value["command"])
            if value["command"] == ["test", "-e", "/home/user/auv"]:
                return {"status": "success", "output": "", "error": "", "returncode": 0}
            if value["command"] == ["sha256sum", "/home/user/auv"]:
                return {"status": "success", "output": "0" * 64 + "  /home/user/auv\n",
                        "error": "", "returncode": 0}
            self.fail("unexpected guest mutation before hash validation")

        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run") as upload, \
             patch.object(self.episode, "_post", side_effect=post):
            with self.assertRaisesRegex(ValueError, "guest-installed AUV bytes differ"):
                self.episode.install()
        upload.assert_not_called()
        self.assertEqual(commands, [["test", "-e", "/home/user/auv"], ["sha256sum", "/home/user/auv"]])

    def test_evaluator_projects_raw_zero_without_masking_failure(self):
        cluster.write_json(self.directory / "paired-device.json", {"guest_auv_sha256": adapter.GUEST_AUV_SHA256})
        raw = {"phase": "evaluate", "task_sha256": adapter.task099.TASK_SHA256,
               "result": {"score": 0.0, "partial_scores": {"distance": {"score": 0.0}}}}
        output = io.StringIO()
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run", return_value=json.dumps(raw)) as run, redirect_stdout(output):
            self.episode.evaluator("evaluate")
        result = json.loads(output.getvalue())
        self.assertEqual(result["score"], 0.0)
        self.assertEqual(result["result"], raw["result"])
        self.assertIn("v2_task099_evaluator.py", run.call_args.args[0][1])
        with patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(cluster, "_run", side_effect=RuntimeError("file transport failed")):
            with self.assertRaisesRegex(RuntimeError, "file transport failed"):
                self.episode.evaluator("evaluate")

    def test_action_requires_driver_delivery_run_and_byte_verified_png(self):
        cluster.write_json(self.directory / "paired-device.json", {"guest_auv_sha256": adapter.GUEST_AUV_SHA256,
                                                                 "device_id": "observed-device"})
        (self.directory / "paired-profiles.json").write_text("{}")
        png = b"\x89PNG\r\n\x1a\nfixture"

        def child(*_args, **_kwargs):
            (self.directory / "final-screenshot.png").write_bytes(png)
            cluster.write_json(self.directory / "action_evidence.json", {"run_ids": ["run-1"],
                "final_artifact": {"path": "final-screenshot.png", "sha256": cluster.sha256(self.directory / "final-screenshot.png")}})
            (self.directory / "input-action-results.json").write_text(json.dumps([
                [{"selected_path": "foreground_system_events", "attempts": [{"succeeded": True}], "verified": False}], []]))
            return MagicMock(returncode=0)

        with patch.dict(os.environ, {"AUV_OSWORLD_ACTION_EVIDENCE": str(self.directory / "action_evidence.json")}), \
             patch.object(self.episode, "assert_identity"), \
             patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter.subprocess, "run", side_effect=child) as run:
            self.episode.action()
        plan = json.loads((self.directory / "fixed-action-plan.json").read_text())
        self.assertEqual(plan["actions"], adapter.FIXED_ACTIONS)
        self.assertEqual(plan["context"]["device_id"], "observed-device")
        run.assert_called_once_with([self.config["action_binary"], "--plan", str(self.directory / "fixed-action-plan.json")], check=False)
        with patch.dict(os.environ, {"AUV_OSWORLD_ACTION_EVIDENCE": str(self.directory / "action_evidence.json")}), \
             patch.object(self.episode, "assert_identity"), patch.object(self.episode, "forward", return_value=nullcontext()):
            with self.assertRaisesRegex(FileExistsError, "stale action evidence"):
                self.episode.action()

    def test_action_rejects_success_without_input_delivery(self):
        cluster.write_json(self.directory / "paired-device.json", {"guest_auv_sha256": adapter.GUEST_AUV_SHA256,
                                                                 "device_id": "observed-device"})
        (self.directory / "paired-profiles.json").write_text("{}")

        def child(*_args, **_kwargs):
            (self.directory / "input-action-results.json").write_text("[]")
            (self.directory / "action_evidence.json").write_text('{}')
            return MagicMock(returncode=0)

        with patch.dict(os.environ, {"AUV_OSWORLD_ACTION_EVIDENCE": str(self.directory / "action_evidence.json")}), \
             patch.object(self.episode, "assert_identity"), patch.object(self.episode, "forward", return_value=nullcontext()), \
             patch.object(adapter.subprocess, "run", side_effect=child):
            with self.assertRaisesRegex(ValueError, "driver delivery"):
                self.episode.action()

    def test_reset_keeps_retained_pvc_and_rejects_replaced_uid_even_if_binaries_disappear(self):
        self.assertIs(adapter.Episode.reset, cluster.Episode.reset)
        self.assertIs(adapter.Episode._retained_pvc, cluster.Episode._retained_pvc)
        for name in ("guest_auv_binary", "host_auv_binary", "action_binary", "task_source", "asset"):
            Path(self.config[name]).unlink()
        with patch.object(adapter.task099, "load_task") as task:
            self.assertEqual(adapter.load_config(self.config_path, reset=True), self.config)
        task.assert_not_called()
        cluster.write_json(self.episode.owned_path, [{"kind": "pod", "name": self.config["runtime_pod"], "uid": "old"}])
        with patch.object(self.episode, "api_proxy", return_value=nullcontext("http://127.0.0.1:1")), \
             patch.object(self.episode, "get", return_value={"metadata": {"uid": "new", "labels": self.episode._labels("qemu")}}), \
             patch.object(self.episode, "request_deletion") as delete:
            with self.assertRaisesRegex(ValueError, "replaced"):
                self.episode.reset()
        delete.assert_not_called()


if __name__ == "__main__":
    unittest.main()
