"""Paired typed-action trial contracts; all cluster and child processes are mocked."""

import json
import os
import sys
import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from unittest.mock import MagicMock, patch

from auv_osworld import k8s_typed_action_adapter as adapter


def episode(task_id: str, episode_id: str, port: int) -> dict:
    return {
        "batch_id": "typed-control",
        "episode_id": episode_id,
        "namespace": "bench",
        "kubeconfig": "/fake/kubeconfig",
        "context": "test-context",
        "node": "liet-gpu-1",
        "runtime_pod": f"{episode_id}-vm",
        "runtime_service": f"{episode_id}-svc",
        "proxy_pod": f"{episode_id}-proxy",
        "proxy_image": "registry.example/proxy@sha256:" + "a" * 64,
        "base_pvc": "osworld-v1-hot",
        "base_qcow_sha256": "b" * 64,
        "guest_auv_binary": "/fake/guest-auv",
        "host_auv_binary": "/fake/host-auv",
        "upstream_checkout": "/fake/osworld",
        "setup_local_port": port,
        "auv_local_port": port + 1,
        "task_id": task_id,
    }


class TypedAdapterTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.chrome = episode("2ad9387a-65d8-4e33-ad5b-7580065a27ca", "chrome-typed", 25000)
        self.vlc = episode("5ac2891a-eacd-4954-b339-98abba077adb", "vlc-typed", 25002)
        self.paths = [self.root / "chrome.json", self.root / "vlc.json"]
        for path, value in zip(self.paths, (self.chrome, self.vlc)):
            path.write_text(json.dumps(value))
        self.binary = self.root / "auv-osworld-action"
        self.binary.write_bytes(b"pinned-action-binary")
        self.batch = self.root / "batch.json"
        self.batch.write_text(
            json.dumps(
                {
                    "batch_id": "typed-control",
                    "episodes": [str(path) for path in self.paths],
                    "action_binary": str(self.binary),
                }
            )
        )

    def capture_manifest(self, path):
        config = json.loads(Path(path).read_text())
        return {
            "trust": "operator-audited",
            "batch_id": config["batch_id"],
            "episodes": [
                {
                    "episode_id": config["episode_id"],
                    "identity": {
                        "task_id": config["task_id"],
                        "task_sha256": "upstream-task-hash",
                        "topology": "paired-remote-capture-only-negative-control",
                        "runner_identity": "capture-only",
                    },
                    "phases": {
                        name: {
                            "argv": ["python", "capture", "phase", name, "--config", str(path)],
                            "timeout_seconds": 600 if name == "action" else 180,
                        }
                        for name in ("boot", "install", "setup", "action", "evaluate", "reset")
                    },
                }
            ],
        }

    def test_predeclares_exact_two_independent_typed_episodes(self):
        with (
            patch.object(adapter.capture, "manifest", side_effect=self.capture_manifest),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
        ):
            built = adapter.manifest(self.batch)
        self.assertEqual(
            [e["identity"]["task_id"] for e in built["episodes"]], [self.chrome["task_id"], self.vlc["task_id"]]
        )
        self.assertEqual([e["episode_id"] for e in built["episodes"]], ["chrome-typed", "vlc-typed"])
        self.assertEqual([e["phases"]["action"]["timeout_seconds"] for e in built["episodes"]], [600, 600])
        self.assertEqual([e["phases"]["reset"]["argv"][4] for e in built["episodes"]], ["reset", "reset"])
        self.assertTrue(
            all(
                e["identity"]["topology"] == "paired-remote-typed-action-infrastructure-trial"
                for e in built["episodes"]
            )
        )
        self.assertTrue(all(e["identity"]["action_binary_sha256"] == adapter.ACTION_SHA256 for e in built["episodes"]))
        self.assertTrue(all("action_argv" not in e for e in built["episodes"]))
        for item, path in zip(built["episodes"], self.paths):
            for phase in adapter.capture.PHASES:
                argv = item["phases"][phase]["argv"]
                self.assertEqual(argv[3:7], ["phase", phase, "--config", str(path)])
                self.assertEqual(
                    argv[7:11],
                    [
                        "--config-sha256",
                        item["identity"]["episode_config_sha256"],
                        "--task-id",
                        item["identity"]["task_id"],
                    ],
                )
                self.assertEqual(argv[-2:], ["--action-binary", str(self.binary)])
            self.assertEqual(
                item["identity"]["action_template_sha256"], adapter.TEMPLATES[item["identity"]["task_id"]][1]
            )

    def test_template_bytes_are_pinned_and_not_task_solutions(self):
        self.assertEqual(
            adapter.template(self.chrome["task_id"])["actions"],
            [{"action_type": "MOVE_TO", "x": 600, "y": 500}, "DONE"],
        )
        self.assertEqual(
            adapter.template(self.vlc["task_id"])["actions"], [{"action_type": "MOVE_TO", "x": 620, "y": 520}, "DONE"]
        )
        with patch.object(Path, "read_bytes", return_value=b"{}"):
            with self.assertRaisesRegex(ValueError, "template bytes"):
                adapter.template(self.chrome["task_id"])

    def test_rejects_hash_task_and_unknown_command_fields_before_manifest(self):
        original = json.loads(self.batch.read_text())
        for change in (
            {"action_argv": ["xdotool", "click", "1"]},
            {"episodes": [str(self.paths[0]), str(self.paths[0])]},
        ):
            self.batch.write_text(json.dumps({**original, **change}))
            with patch.object(adapter.capture, "manifest") as capture_manifest:
                with self.assertRaises(ValueError):
                    adapter.manifest(self.batch)
            capture_manifest.assert_not_called()
        self.batch.write_text(json.dumps(original))
        with (
            patch.object(adapter.capture, "manifest", side_effect=self.capture_manifest),
            patch.object(adapter.capture, "sha256", return_value="0" * 64),
        ):
            with self.assertRaisesRegex(ValueError, "action binary"):
                adapter.manifest(self.batch)
        self.vlc["task_id"] = self.chrome["task_id"]
        self.paths[1].write_text(json.dumps(self.vlc))
        with (
            patch.object(adapter.capture, "manifest", side_effect=self.capture_manifest),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
        ):
            with self.assertRaisesRegex(ValueError, "Chrome and one VLC"):
                adapter.manifest(self.batch)

    def test_rejects_cross_episode_resource_and_port_reuse(self):
        self.vlc["proxy_pod"] = self.chrome["proxy_pod"]
        self.vlc["setup_local_port"] = self.chrome["setup_local_port"]
        self.paths[1].write_text(json.dumps(self.vlc))
        with (
            patch.object(adapter.capture, "manifest", side_effect=self.capture_manifest),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
        ):
            with self.assertRaisesRegex(ValueError, "resource|port"):
                adapter.manifest(self.batch)

    def test_episode_command_override_is_rejected_by_existing_config_validator(self):
        self.chrome["action_argv"] = ["python", "-c", "print('not allowed')"]
        self.paths[0].write_text(json.dumps(self.chrome))
        with patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256):
            with self.assertRaisesRegex(ValueError, "config must contain exactly"):
                adapter.manifest(self.batch)

    def test_action_binds_observed_device_without_changing_template_actions(self):
        config = self.chrome
        directory = self.root / config["episode_id"]
        directory.mkdir()
        adapter.capture.write_json(directory / "paired-device.json", {"device_id": "observed-device"})
        (directory / "paired-profiles.json").write_text("secret-profile")
        evidence = {"run_ids": ["run-1"], "final_artifact": {"path": "final-screenshot.png", "sha256": "a" * 64}}
        observed = {}

        def child(argv, **kwargs):
            self.assertEqual(argv[:2], [str(self.binary), "--plan"])
            plan = json.loads(Path(argv[2]).read_text())
            observed.update(plan)
            self.assertEqual(kwargs["check"], False)
            self.assertNotIn("shell", kwargs)
            adapter.capture.write_json(directory / "action_evidence.json", evidence)
            return MagicMock(returncode=0)

        with (
            patch.object(adapter.capture.Episode, "assert_identity"),
            patch.object(adapter.capture.Episode, "forward", return_value=nullcontext()),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
            patch.object(adapter.capture.subprocess, "run", side_effect=child),
        ):
            adapter.action(config, directory, self.binary)
        self.assertEqual(
            observed["context"],
            {
                "kind": "paired",
                "device_id": "observed-device",
                "config_profile": config["episode_id"],
                "profiles_file": str(directory / "paired-profiles.json"),
            },
        )
        self.assertEqual(observed["actions"], adapter.template(config["task_id"])["actions"])
        self.assertEqual(json.loads((directory / "action_evidence.json").read_text()), evidence)

    def test_reset_remains_existing_uid_safe_phase(self):
        with (
            patch.object(adapter.capture, "manifest", side_effect=self.capture_manifest),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
        ):
            built = adapter.manifest(self.batch)
        for item in built["episodes"]:
            self.assertEqual(item["phases"]["reset"]["argv"][3:5], ["phase", "reset"])
        directory = self.root / "chrome-typed"
        directory.mkdir()
        adapter.capture.write_json(
            directory / "k8s_owned.json", [{"kind": "pod", "name": "chrome-typed-vm", "uid": "old"}]
        )
        episode_obj = adapter.capture.Episode(self.chrome, directory)
        with (
            patch.object(episode_obj, "_discover_owned", return_value=[]),
            patch.object(
                episode_obj, "get", return_value={"metadata": {"uid": "new", "labels": episode_obj._labels("qemu")}}
            ),
            patch.object(episode_obj, "request_deletion") as deletion,
        ):
            with self.assertRaisesRegex(ValueError, "replaced"):
                episode_obj.reset()
        deletion.assert_not_called()

    def test_every_phase_checks_pinned_config_before_delegate_including_reset(self):
        directory = self.root / "chrome-typed"
        directory.mkdir()
        argv = [
            "typed",
            "phase",
            "reset",
            "--config",
            str(self.paths[0]),
            "--config-sha256",
            adapter.ACTION_SHA256,
            "--task-id",
            self.chrome["task_id"],
            "--action-binary",
            str(self.binary),
        ]
        with (
            patch.object(sys, "argv", argv),
            patch.dict(os.environ, {"AUV_OSWORLD_EPISODE_DIR": str(directory)}),
            patch.object(adapter.capture, "sha256", return_value="0" * 64),
            patch.object(adapter.capture.Episode, "reset") as reset,
        ):
            with self.assertRaisesRegex(ValueError, "config changed"):
                adapter.main()
            reset.assert_not_called()
        with (
            patch.object(sys, "argv", argv),
            patch.dict(os.environ, {"AUV_OSWORLD_EPISODE_DIR": str(directory)}),
            patch.object(adapter.capture, "sha256", return_value=adapter.ACTION_SHA256),
            patch.object(adapter.capture, "load_config", return_value=self.chrome),
            patch.object(adapter.capture.Episode, "reset") as reset,
        ):
            adapter.main()
            reset.assert_called_once_with()

    def test_reset_dispatches_after_action_binary_disappears(self):
        directory = self.root / "chrome-typed"
        directory.mkdir()
        self.binary.unlink()
        argv = [
            "typed",
            "phase",
            "reset",
            "--config",
            str(self.paths[0]),
            "--config-sha256",
            "config-digest",
            "--task-id",
            self.chrome["task_id"],
            "--action-binary",
            str(self.binary),
        ]

        def measured(path):
            if Path(path) == self.paths[0]:
                return "config-digest"
            raise FileNotFoundError("action binary is gone")

        with (
            patch.object(sys, "argv", argv),
            patch.dict(os.environ, {"AUV_OSWORLD_EPISODE_DIR": str(directory)}),
            patch.object(adapter.capture, "sha256", side_effect=measured),
            patch.object(adapter.capture, "load_config", return_value=self.chrome),
            patch.object(adapter.capture.Episode, "reset") as reset,
        ):
            adapter.main()
            reset.assert_called_once_with()
        argv[2] = "boot"
        with (
            patch.object(sys, "argv", argv),
            patch.dict(os.environ, {"AUV_OSWORLD_EPISODE_DIR": str(directory)}),
            patch.object(adapter.capture, "sha256", side_effect=measured),
            patch.object(adapter.capture, "load_config", return_value=self.chrome),
            patch.object(adapter.capture.Episode, "boot") as boot,
        ):
            with self.assertRaisesRegex(FileNotFoundError, "action binary is gone"):
                adapter.main()
            boot.assert_not_called()


if __name__ == "__main__":
    unittest.main()
