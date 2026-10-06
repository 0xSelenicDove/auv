"""Local process and ledger tests; no guest, Kubernetes, or GUI input."""

import importlib.util
import json
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest


RUNNER_PATH = Path(__file__).resolve().parents[1] / "batch_runner.py"
spec = importlib.util.spec_from_file_location("batch_runner", RUNNER_PATH)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


SUCCESS_ACTION = """
import hashlib, json, os
from pathlib import Path
root = Path(os.environ['AUV_OSWORLD_EPISODE_DIR'])
artifact = root / 'final.png'
artifact.write_bytes(b'local-test-capture')
evidence = {'run_ids': ['run-1', 'run-2'], 'final_artifact': {'path': 'final.png', 'sha256': hashlib.sha256(artifact.read_bytes()).hexdigest()}}
Path(os.environ['AUV_OSWORLD_ACTION_EVIDENCE']).write_text(json.dumps(evidence))
print(json.dumps(evidence), flush=True)
"""

TIMEOUT_ACTION = """
import json, os, subprocess, sys, time
from pathlib import Path
root = Path(os.environ['AUV_OSWORLD_EPISODE_DIR'])
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
(root / 'child.pid').write_text(str(child.pid))
evidence = {'run_ids': ['run-before-timeout'], 'final_artifact': None}
Path(os.environ['AUV_OSWORLD_ACTION_EVIDENCE']).write_text(json.dumps(evidence))
print(json.dumps(evidence), flush=True)
time.sleep(60)
"""


def command(source: str, seconds: float = 2) -> dict:
    return {"argv": [sys.executable, "-u", "-c", source], "timeout_seconds": seconds}


def manifest(action: str = SUCCESS_ACTION, evaluator: str = 'print("{\\"score\\": 0.75}")', reset: str | None = None) -> dict:
    reset = reset or 'print("{\\"removed_resources\\": [\\"test-pod\\"], \\"retained_pvcs_verified\\": [\\"base-pvc\\"]}")'
    identity = {field: f"pinned-{field}" for field in runner.IDENTITY_FIELDS}
    return {
        "trust": "operator-audited", "batch_id": "local-fixture",
        "episodes": [{
            "episode_id": "episode-1", "identity": identity,
            "phases": {
                "boot": command("print('boot')"),
                "install": command("print('install')"),
                "setup": command("print('setup')"),
                "action": command(action, 0.6 if action == TIMEOUT_ACTION else 2),
                "evaluate": command(evaluator),
                "reset": command(reset),
            },
        }],
    }


class BatchRunnerTest(unittest.TestCase):
    def run_fixture(self, config: dict) -> tuple[dict, Path]:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        destination = Path(self.temporary.name) / "batch"
        result = runner.run_batch(config, destination)
        self.assertEqual(result, json.loads((destination / "ledger.json").read_text()))
        return result, destination

    def test_predeclared_batch_records_success_and_verified_artifact(self):
        config = manifest()
        config["episodes"].append({**config["episodes"][0], "episode_id": "episode-2"})
        result, directory = self.run_fixture(config)
        self.assertEqual(result["denominator"], 2)
        self.assertEqual([episode["status"] for episode in result["episodes"]], ["completed", "completed"])
        for episode in result["episodes"]:
            self.assertEqual(episode["score"], 0.75)
            self.assertEqual(episode["auv"]["run_ids"], ["run-1", "run-2"])
            self.assertEqual(episode["auv"]["evidence_status"], "verified")
            self.assertEqual(episode["cleanup"]["report"]["retained_pvcs_verified"], ["base-pvc"])
            self.assertEqual(set(episode["phases"]), set(runner.PHASES))
            self.assertTrue((directory / episode["episode_id"] / "final.png").exists())
            for phase in episode["phases"].values():
                self.assertTrue(phase["started_utc"].endswith("+00:00"))
                self.assertTrue(phase["ended_utc"].endswith("+00:00"))
                self.assertGreaterEqual(phase["elapsed_monotonic_ns"], 0)

    def test_action_deadline_stops_group_but_still_scores_deadline_state(self):
        # ROOT CAUSE:
        # If action budget expires, treating timeout as a skipped evaluation
        # loses the benchmark score for the deadline state. The action process
        # group must be stopped before the independent evaluator can run.
        result, directory = self.run_fixture(manifest(action=TIMEOUT_ACTION))
        episode = result["episodes"][0]
        self.assertEqual(episode["phases"]["action"]["status"], "timeout")
        self.assertTrue(episode["phases"]["action"]["group_terminated"])
        self.assertEqual(episode["score"], 0.75)
        self.assertEqual(episode["evaluator_output"], {"score": 0.75})
        self.assertEqual(episode["failure_layers"], [{"layer": "action", "reason": "timeout"}])
        self.assertEqual(episode["auv"]["run_ids"], ["run-before-timeout"])
        self.assertEqual(episode["cleanup"]["status"], "ok")
        child_pid = int((directory / "episode-1" / "child.pid").read_text())
        for _ in range(20):
            observed = subprocess.run(["ps", "-o", "stat=", "-p", str(child_pid)], capture_output=True, text=True, check=False)
            if observed.returncode != 0 or observed.stdout.strip().startswith("Z"):
                break
            time.sleep(0.05)
        else:
            self.fail("the action child is still executing after the deadline")

    def test_evaluator_failure_keeps_score_absent(self):
        result, _ = self.run_fixture(manifest(evaluator="raise RuntimeError('evaluation unavailable')"))
        episode = result["episodes"][0]
        self.assertNotIn("score", episode)
        self.assertNotIn("evaluator_output", episode)
        self.assertEqual(episode["failure_layers"], [{"layer": "evaluate", "reason": "exit_failed"}])
        self.assertEqual(episode["cleanup"]["status"], "ok")

    def test_evaluator_deadline_is_independent_and_score_remains_absent(self):
        config = manifest(evaluator="import time; time.sleep(60)")
        config["episodes"][0]["phases"]["evaluate"]["timeout_seconds"] = 0.2
        result, _ = self.run_fixture(config)
        episode = result["episodes"][0]
        self.assertEqual(episode["phases"]["action"]["status"], "ok")
        self.assertEqual(episode["phases"]["evaluate"]["status"], "timeout")
        self.assertTrue(episode["phases"]["evaluate"]["group_terminated"])
        self.assertNotIn("score", episode)
        self.assertEqual(episode["cleanup"]["status"], "ok")

    def test_boot_deadline_skips_following_work_but_runs_cleanup(self):
        config = manifest()
        config["episodes"][0]["phases"]["boot"] = command("import time; time.sleep(60)", 0.2)
        result, _ = self.run_fixture(config)
        episode = result["episodes"][0]
        self.assertEqual(set(episode["phases"]), {"boot", "reset"})
        self.assertEqual(episode["phases"]["boot"]["status"], "timeout")
        self.assertEqual(episode["failure_layers"], [{"layer": "boot", "reason": "timeout"}])
        self.assertNotIn("score", episode)

    def test_cleanup_failure_does_not_mask_action_failure(self):
        result, _ = self.run_fixture(manifest(action=TIMEOUT_ACTION, reset="raise RuntimeError('cleanup unavailable')"))
        episode = result["episodes"][0]
        self.assertEqual(episode["score"], 0.75)
        self.assertEqual(episode["failure_layers"], [
            {"layer": "action", "reason": "timeout"},
            {"layer": "reset", "reason": "exit_failed"},
        ])
        self.assertEqual(episode["cleanup"]["status"], "exit_failed")

    def test_stdout_sidecar_disagreement_is_evidence_failure(self):
        action = SUCCESS_ACTION + "print(json.dumps({'run_ids': ['other'], 'final_artifact': evidence['final_artifact']}), flush=True)\n"
        result, _ = self.run_fixture(manifest(action=action))
        episode = result["episodes"][0]
        self.assertIsNone(episode["auv"]["final_artifact"])
        self.assertEqual(episode["failure_layers"][0]["layer"], "action_evidence")
        self.assertEqual(episode["score"], 0.75)

    def test_setup_failure_skips_action_evaluation_but_runs_cleanup(self):
        config = manifest()
        config["episodes"][0]["phases"]["setup"] = command("raise RuntimeError('setup failed')")
        result, _ = self.run_fixture(config)
        episode = result["episodes"][0]
        self.assertNotIn("action", episode["phases"])
        self.assertNotIn("evaluate", episode["phases"])
        self.assertNotIn("score", episode)
        self.assertEqual(episode["failure_layers"], [{"layer": "setup", "reason": "exit_failed"}])
        self.assertEqual(episode["cleanup"]["status"], "ok")

    def test_rejects_untrusted_manifest_without_starting_batch(self):
        config = manifest()
        config["trust"] = "unknown"
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "batch"
            with self.assertRaisesRegex(ValueError, "operator-audited"):
                runner.run_batch(config, destination)
            self.assertFalse(destination.exists())

    def test_rejects_path_traversal_and_non_string_identity(self):
        for episode_id in (".", "..", "../outside", "nested/child"):
            with self.subTest(episode_id=episode_id), tempfile.TemporaryDirectory() as temporary:
                config = manifest()
                config["episodes"][0]["episode_id"] = episode_id
                with self.assertRaisesRegex(ValueError, "episode_id"):
                    runner.run_batch(config, Path(temporary) / "batch")
        config = manifest()
        config["episodes"][0]["identity"]["auv_target"] = 42
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "identity"):
                runner.run_batch(config, Path(temporary) / "batch")

    def test_guest_local_and_paired_target_identity_are_preserved(self):
        config = manifest()
        config["episodes"][0]["identity"].update(topology="guest-local-shared-socket", auv_target="unix:///home/user/auv.sock")
        paired = json.loads(json.dumps(config["episodes"][0]))
        paired["episode_id"] = "paired"
        paired["identity"].update(topology="paired-remote", auv_target="device:0123456789abcdef")
        config["episodes"].append(paired)
        result, _ = self.run_fixture(config)
        self.assertEqual([episode["identity"]["auv_target"] for episode in result["episodes"]],
                         ["unix:///home/user/auv.sock", "device:0123456789abcdef"])

    def test_action_spawn_failure_is_not_scored(self):
        config = manifest()
        config["episodes"][0]["phases"]["action"]["argv"] = ["/does-not-exist/auv-agent"]
        result, _ = self.run_fixture(config)
        episode = result["episodes"][0]
        self.assertEqual(episode["phases"]["action"]["status"], "spawn_failed")
        self.assertNotIn("evaluate", episode["phases"])
        self.assertNotIn("score", episode)
        self.assertEqual(episode["failure_layers"], [{"layer": "action", "reason": "spawn_failed"}])
        self.assertEqual(episode["cleanup"]["status"], "ok")

    def test_phase_io_exception_still_runs_reset_and_preserves_failure(self):
        config = manifest()
        config["episodes"][0]["phases"]["action"] = command(SUCCESS_ACTION)
        # The setup command creates a directory at the action stdout path;
        # opening it as a file then raises before spawning the action child.
        config["episodes"][0]["phases"]["setup"] = command("from pathlib import Path; Path('action.stdout').mkdir()")
        result, _ = self.run_fixture(config)
        episode = result["episodes"][0]
        self.assertEqual(episode["failure_layers"][0]["layer"], "runner")
        self.assertEqual(episode["failure_layers"][0]["reason"], "phase_exception")
        self.assertEqual(episode["cleanup"]["status"], "ok")
        self.assertNotIn("evaluate", episode["phases"])

    def test_keyboard_interrupt_stops_action_runs_reset_and_leaves_rest_scheduled(self):
        config = manifest(action=TIMEOUT_ACTION)
        config["episodes"][0]["phases"]["action"]["timeout_seconds"] = 30
        config["episodes"].append({**config["episodes"][0], "episode_id": "episode-2"})
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(config))
            directory = root / "batch"
            process = subprocess.Popen(
                [sys.executable, str(RUNNER_PATH), "--manifest", str(manifest_path), "--output-dir", str(directory)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            try:
                evidence = directory / "episode-1" / "action_evidence.json"
                deadline = time.monotonic() + 5
                while not evidence.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertTrue(evidence.exists(), "action process never started")
                process.send_signal(signal.SIGINT)
                stdout, stderr = process.communicate(timeout=5)
                self.assertEqual(process.returncode, 1, stderr)
                self.assertTrue(stdout)
                ledger = json.loads((directory / "ledger.json").read_text())
                self.assertEqual(ledger["denominator"], 2)
                self.assertTrue(ledger["stopped_early"])
                first, second = ledger["episodes"]
                self.assertEqual(first["phases"]["action"]["status"], "interrupted")
                self.assertTrue(first["phases"]["action"]["group_terminated"])
                self.assertEqual(first["auv"]["run_ids"], ["run-before-timeout"])
                self.assertEqual(first["cleanup"]["status"], "ok")
                self.assertNotIn("score", first)
                self.assertEqual(second["status"], "scheduled")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate()


if __name__ == "__main__":
    unittest.main()
