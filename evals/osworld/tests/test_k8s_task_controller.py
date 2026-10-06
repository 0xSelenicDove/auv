"""Chrome controller predicates and protocol tests; no Kubernetes or GUI input."""

import importlib.util
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from contextlib import nullcontext, redirect_stdout
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("k8s_task_controller", ROOT / "k8s_task_controller.py")
controller = importlib.util.module_from_spec(SPEC)
sys.path.insert(0, str(ROOT))
SPEC.loader.exec_module(controller)
sys.path.pop(0)


ARCHIVED = Path("/private/tmp/auv-osworld-v1-interactive-1006m/chrome-interactive-m")
STATIC_NEGATIVE = Path("/private/tmp/auv-osworld-v1-positive-settle.YmIGAE/output/chrome-positive-settle-l/final-screenshot.png")


class ChromePredicateTest(unittest.TestCase):
    def test_spatial_gates_reject_same_word_in_wrong_place(self):
        words = [
            controller.Word("Add", 1013, 524, 62),
            controller.Word("Favorites", 747, 180, 96),
            controller.Word("New", 680, 136, 96),
            controller.Word("folder", 724, 135, 96),
            controller.Word("Bookmarks", 725, 222, 96),
        ]
        self.assertFalse(controller.matches_gate("add-folder-menu", words))
        self.assertTrue(controller.matches_gate("favorites-dialog", words))
        self.assertFalse(controller.matches_gate("favorites-bookmark-bar", words))

    def test_archived_positive_and_static_negative_gate_matrix(self):
        if not ARCHIVED.is_dir() or not STATIC_NEGATIVE.is_file():
            self.skipTest("private archived AUV checkpoint files unavailable")
        controller.verify_ocr_pins(Path("/opt/homebrew/bin/tesseract"),
                                   Path("/opt/homebrew/share/tessdata/eng.traineddata"))
        observed = {
            "menu": controller.ocr_words(ARCHIVED / "checkpoint-0003.png", Path("/opt/homebrew/bin/tesseract")),
            "modal": controller.ocr_words(ARCHIVED / "checkpoint-0004.png", Path("/opt/homebrew/bin/tesseract")),
            "pre": controller.ocr_words(ARCHIVED / "checkpoint-0005.png", Path("/opt/homebrew/bin/tesseract")),
            "post": controller.ocr_words(ARCHIVED / "checkpoint-0006.png", Path("/opt/homebrew/bin/tesseract")),
            "negative": controller.ocr_words(STATIC_NEGATIVE, Path("/opt/homebrew/bin/tesseract")),
        }
        self.assertTrue(controller.matches_gate("add-folder-menu", observed["menu"]))
        self.assertTrue(controller.matches_gate("new-folder-dialog", observed["modal"]))
        self.assertTrue(controller.matches_gate("favorites-dialog", observed["pre"]))
        self.assertTrue(controller.matches_gate("favorites-bookmark-bar", observed["post"]))
        self.assertFalse(controller.matches_gate("favorites-bookmark-bar", observed["negative"]))
        self.assertTrue(controller.matches_gate("favorites-dialog", observed["negative"]))

    def test_policy_bytes_and_ocr_pins_fail_closed(self):
        value = controller.policy()
        self.assertEqual([step["gate"] for step in value["steps"]], list(controller.GATES))
        with patch.object(controller, "POLICY_SHA256", "0" * 64):
            with self.assertRaisesRegex(ValueError, "policy differs"):
                controller.policy()
        with patch.object(controller, "OCR_SHA256", "0" * 64):
            with self.assertRaisesRegex(ValueError, "Tesseract executable differs"):
                controller.verify_ocr_pins(Path("/opt/homebrew/bin/tesseract"),
                                           Path("/opt/homebrew/share/tessdata/eng.traineddata"))

    def test_capture_response_requires_hash_and_episode_confinement(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            image = directory / "checkpoint-0001.png"
            image.write_bytes(b"sample capture")
            artifact = {"path": image.name, "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}
            (directory / "checkpoints.json").write_text(json.dumps([artifact]))
            self.assertEqual(controller._artifact(directory, {"artifact": artifact}), image.resolve())
            with self.assertRaisesRegex(ValueError, "SHA differs"):
                controller._artifact(directory, {"artifact": {**artifact, "sha256": "0" * 64}})
            with self.assertRaises(ValueError):
                controller._artifact(directory, {"artifact": {**artifact, "path": "../outside.png"}})

    def test_jsonl_pipe_requires_matching_order_and_terminal_shape(self):
        source = """
import json, sys
print(json.dumps({'op':'ready','version':1,'run_id':'run-1'}), flush=True)
for line in sys.stdin:
    request=json.loads(line)
    if request['op']=='action':
        print(json.dumps({'seq':request['seq'],'op':'action','delivery':[{'attempts':[{'succeeded':True}]}]}), flush=True)
    else:
        print(json.dumps({'run_ids':['run-1'],'final_artifact':{'path':'final-screenshot.png','sha256':'a'*64}}), flush=True)
"""
        process = subprocess.Popen([sys.executable, "-u", "-c", source], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        try:
            pipe = controller.InteractivePipe(process)
            self.assertEqual(pipe.read()["run_id"], "run-1")
            self.assertEqual(pipe.request("action", {"action_type": "CLICK", "x": 1, "y": 2})["seq"], 1)
            self.assertEqual(pipe.request("finish")["run_ids"], ["run-1"])
        finally:
            process.stdin.close()
            process.wait(timeout=2)
            process.stdout.close()

    def test_manifest_has_one_fixed_chrome_episode_and_pinned_policy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "episode.json"
            config.write_text(json.dumps({"batch_id": "chrome-one", "task_id": controller.capture.CHROME_TASK}))
            binary = root / "auv-osworld-action"
            binary.write_bytes(b"fake binary")
            tesseract = root / "tesseract"
            tesseract.write_bytes(b"fake OCR")
            data = root / "eng.traineddata"
            data.write_bytes(b"fake model")
            batch = root / "batch.json"
            batch.write_text(json.dumps({"batch_id": "chrome-one", "episode": str(config),
                                         "action_binary": str(binary), "tesseract_binary": str(tesseract),
                                         "eng_traineddata": str(data)}))
            base = {"trust": "operator-audited", "batch_id": "chrome-one", "episodes": [{
                "episode_id": "chrome-1", "identity": {"task_id": controller.capture.CHROME_TASK},
                "phases": {name: {} for name in controller.capture.PHASES}}]}
            with patch.object(controller.capture, "load_config", return_value={"batch_id": "chrome-one", "task_id": controller.capture.CHROME_TASK}), \
                 patch.object(controller.capture, "manifest", return_value=base), \
                 patch.object(controller.capture, "sha256", return_value=controller.ACTION_SHA256), \
                 patch.object(controller, "verify_ocr_pins"), \
                 patch.object(controller, "policy", return_value={"steps": []}):
                built = controller.manifest(batch)
            self.assertEqual(len(built["episodes"]), 1)
            self.assertEqual(built["episodes"][0]["identity"]["controller_policy_sha256"], controller.POLICY_SHA256)
            self.assertEqual(built["episodes"][0]["phases"]["action"]["timeout_seconds"], 600)
            self.assertEqual(built["episodes"][0]["phases"]["reset"]["argv"][3], "reset")

    def test_action_uses_one_run_and_observation_gates_before_next_action(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "paired-device.json").write_text(json.dumps({"device_id": "observed-device"}))
            (directory / "paired-profiles.json").write_text("{}")
            binary = directory / "auv-osworld-action"
            binary.write_bytes(b"pinned action")
            fake_ocr = directory / "tesseract"
            fake_ocr.write_bytes(b"ocr")
            fake_data = directory / "eng.traineddata"
            fake_data.write_bytes(b"data")
            config = {"episode_id": "chrome-one", "task_id": controller.capture.CHROME_TASK}
            seen = []

            class FakeEpisode:
                def assert_identity(self):
                    return {}

                def forward(self, *, auv):
                    self_outer.assertTrue(auv)
                    return nullcontext()

            class FakeProcess:
                returncode = 0

                def poll(self):
                    return 0

                def wait(self, timeout):
                    return 0

            class FakePipe:
                def __init__(self, process):
                    self.sequence = 0

                def read(self):
                    (directory / "action_evidence.json").write_text(json.dumps({"run_ids": ["run-1"], "final_artifact": None}))
                    return {"op": "ready", "version": 1, "run_id": "run-1"}

                def request(self, operation, action=None):
                    self.sequence += 1
                    seen.append((operation, action))
                    if operation == "action":
                        return {"seq": self.sequence, "op": "action", "delivery": [{"attempts": [{"succeeded": True}]}]}
                    if operation == "capture":
                        number = sum(op == "capture" for op, _ in seen)
                        name = f"checkpoint-{number:04}.png"
                        image = directory / name
                        image.write_bytes(name.encode())
                        artifact = {"path": name, "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}
                        index = directory / "checkpoints.json"
                        history = json.loads(index.read_text()) if index.exists() else []
                        index.write_text(json.dumps(history + [artifact]))
                        return {"seq": self.sequence, "op": "capture", "artifact": artifact}
                    image = directory / "final-screenshot.png"
                    image.write_bytes(b"final")
                    terminal = {"run_ids": ["run-1"], "final_artifact": {"path": image.name,
                                                                        "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}}
                    (directory / "action_evidence.json").write_text(json.dumps(terminal))
                    return terminal

            observed_words = [
                [controller.Word("Add", 448, 526, 95), controller.Word("folder...", 481, 526, 89)],
                [controller.Word("New", 680, 136, 96), controller.Word("folder", 724, 135, 95),
                 controller.Word("Bookmarks", 725, 222, 96)],
                [controller.Word("New", 680, 136, 96), controller.Word("folder", 724, 135, 95),
                 controller.Word("Bookmarks", 725, 222, 96), controller.Word("Favorites|", 747, 180, 96)],
                [controller.Word("Favorites", 136, 121, 92)],
            ]
            self_outer = self
            output = io.StringIO()
            original_sha = controller.capture.sha256
            def sha(path):
                return controller.ACTION_SHA256 if path == binary else original_sha(path)
            with patch.object(controller.capture, "Episode", return_value=FakeEpisode()), \
                 patch.object(controller.capture, "sha256", side_effect=sha), \
                 patch.object(controller, "verify_ocr_pins"), \
                 patch.object(controller.subprocess, "Popen", return_value=FakeProcess()), \
                 patch.object(controller, "InteractivePipe", FakePipe), \
                 patch.object(controller, "ocr_words", side_effect=observed_words), \
                 redirect_stdout(output):
                controller.action(config, directory, binary, fake_ocr, fake_data)
            self.assertEqual([operation for operation, _ in seen],
                             ["action", "action", "capture", "action", "capture", "action", "capture", "action", "capture", "finish"])
            self.assertEqual(json.loads(output.getvalue())["run_ids"], ["run-1"])
            trace = json.loads((directory / "controller_decisions.json").read_text())
            self.assertEqual(trace["status"], "finished")
            self.assertEqual(len(trace["checks"]), 4)


if __name__ == "__main__":
    unittest.main()
