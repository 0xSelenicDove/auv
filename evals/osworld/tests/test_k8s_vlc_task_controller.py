"""VLC scripted visual policy tests; no Kubernetes or GUI input."""

import hashlib
import json
import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from unittest.mock import patch

from auv_osworld import k8s_vlc_task_controller as controller

FIXTURE = json.loads((Path(__file__).parent / "fixtures/vlc_visual_frames_v1.json").read_text())
FRAMES = {item["name"]: item for item in FIXTURE["frames"]}
ARCHIVES = {
    "red_stale_1006c": Path("/private/tmp/auv-osworld-vlc-loop-1006c/vlc-loop-c/checkpoint-0003.png"),
    "mixed_ambiguous_1006d": Path("/private/tmp/auv-osworld-vlc-toggle-1006d/vlc-toggle-d/checkpoint-0003.png"),
    "advanced_1006e": Path("/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0003.png"),
    "target_initial_off_1006e": Path("/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0005.png"),
    "target_on_1006e": Path("/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0006.png"),
    "target_final_off_1006e": Path("/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0007.png"),
    "target_failed_on_1006f": Path(
        "/private/tmp/auv-osworld-vlc-scripted-1006f.KHtQ2v/output/vlc-scripted-f/checkpoint-0006.png"
    ),
}


def words(name):
    return [controller.shared.Word(*row) for row in FRAMES[name]["words"]]


class VlcPredicateTest(unittest.TestCase):
    def test_archived_spatial_fixture_states(self):
        for name, frame in FRAMES.items():
            with self.subTest(name=name):
                self.assertRegex(frame["source_sha256"], r"^[0-9a-f]{64}$")
                self.assertEqual(controller.classify(words(name))[0], frame["verdict"])
                if "state" in frame:
                    self.assertEqual(controller.checkbox_state(frame["dark_lt_100"]), frame["state"])
        self.assertFalse(controller.classify(words("mixed_ambiguous_1006d"))[0] == "GREEN_ADVANCED")
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            controller.checkbox_state({"target": 7, "checked_control": 20, "unchecked_control": 0})
        with self.assertRaisesRegex(ValueError, "controls"):
            controller.checkbox_state({"target": 20, "checked_control": 0, "unchecked_control": 0})

    def test_optional_full_auv_archives_match_fixture_measurements(self):
        if not all(path.is_file() for path in ARCHIVES.values()):
            self.skipTest("task-owned archived AUV PNGs unavailable")
        tesseract = Path("/opt/homebrew/bin/tesseract")
        eng = Path("/opt/homebrew/share/tessdata/eng.traineddata")
        controller.shared.verify_ocr_pins(tesseract, eng)
        for name, image in ARCHIVES.items():
            with self.subTest(name=name):
                self.assertEqual(controller.capture.sha256(image), FRAMES[name]["source_sha256"])
                self.assertEqual(
                    controller.classify(controller.shared.ocr_words(image, tesseract, eng))[0], FRAMES[name]["verdict"]
                )
                if "state" in FRAMES[name]:
                    self.assertEqual(controller.checkbox_counts(image), FRAMES[name]["dark_lt_100"])
        search = Path("/private/tmp/auv-osworld-vlc-toggle-1006e/vlc-toggle-e/checkpoint-0004.png")
        self.assertTrue(controller.query_visible(search, tesseract, eng))

    def test_policy_bytes_are_pinned(self):
        self.assertEqual(controller.policy()["task_id"], controller.VLC_TASK)
        with patch.object(controller, "POLICY_SHA256", "0" * 64):
            with self.assertRaisesRegex(ValueError, "policy differs"):
                controller.policy()

    def test_manifest_has_fixed_vlc_task_and_six_adapter_phases(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "episode.json"
            config.write_text("{}")
            paths = (config,) + tuple(Path(temporary) / name for name in ("action", "ocr", "eng"))
            base = {
                "episodes": [
                    {
                        "identity": {"task_id": controller.VLC_TASK},
                        "phases": {name: {} for name in controller.capture.PHASES},
                    }
                ]
            }
            with (
                patch.object(controller, "_batch", return_value=paths),
                patch.object(controller.capture, "manifest", return_value=base),
            ):
                built = controller.manifest(Path(temporary) / "batch.json")
            episode = built["episodes"][0]
            self.assertEqual(set(episode["phases"]), set(controller.capture.PHASES))
            self.assertEqual(episode["identity"]["controller_policy_sha256"], controller.POLICY_SHA256)
            self.assertEqual(episode["phases"]["action"]["timeout_seconds"], 600)
            wrong = {"episodes": [{"identity": {"task_id": controller.capture.CHROME_TASK}}]}
            with (
                patch.object(controller, "_batch", return_value=paths),
                patch.object(controller.capture, "manifest", return_value=wrong),
            ):
                with self.assertRaisesRegex(ValueError, "task identity changed"):
                    controller.manifest(Path(temporary) / "batch.json")


class VlcActionTest(unittest.TestCase):
    def run_sequence(self, frames, counts, *, expected_error=None):
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
            seen = []
            frame_iter = iter(frames)
            count_iter = iter(counts)

            class FakeEpisode:
                def assert_identity(self):
                    return {}

                def forward(self, *, auv):
                    if not auv:
                        raise AssertionError("paired AUV forward required")
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
                    (directory / "action_evidence.json").write_text(
                        json.dumps({"run_ids": ["run-1"], "final_artifact": None})
                    )
                    return {"op": "ready", "version": 1, "run_id": "run-1"}

                def request(self, operation, action=None):
                    self.sequence += 1
                    seen.append((operation, action))
                    if operation == "action":
                        return {"seq": self.sequence, "op": "action", "delivery": [{"attempts": [{"succeeded": True}]}]}
                    if operation == "capture":
                        number = sum(op == "capture" for op, _ in seen)
                        image = directory / f"checkpoint-{number:04}.png"
                        image.write_bytes(str(number).encode())
                        artifact = {"path": image.name, "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}
                        index = directory / "checkpoints.json"
                        history = json.loads(index.read_text()) if index.exists() else []
                        index.write_text(json.dumps(history + [artifact]))
                        return {"seq": self.sequence, "op": "capture", "artifact": artifact}
                    image = directory / "final-screenshot.png"
                    image.write_bytes(b"final")
                    terminal = {
                        "run_ids": ["run-1"],
                        "final_artifact": {
                            "path": image.name,
                            "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                        },
                    }
                    (directory / "action_evidence.json").write_text(json.dumps(terminal))
                    return terminal

            def ocr(_image, _tesseract, _eng):
                name = next(frame_iter)
                if name == "main":
                    return [
                        controller.shared.Word("VLC", 886, 341, 95),
                        controller.shared.Word("media", 923, 341, 95),
                        controller.shared.Word("player", 974, 341, 95),
                    ]
                if name == "simple":
                    return [
                        controller.shared.Word("Simple", 866, 200, 90),
                        controller.shared.Word("Preferences", 940, 200, 90),
                        controller.shared.Word("Interface", 560, 322, 90),
                        controller.shared.Word("Settings", 655, 322, 90),
                    ]
                if name == "search":
                    return words("advanced_1006e") + [controller.shared.Word("Playlist", 616, 296, 96)]
                if name == "blank":
                    return []
                return words(name)

            original_sha = controller.capture.sha256

            def sha(path):
                return controller.ACTION_SHA256 if path == binary else original_sha(path)

            with (
                patch.object(controller.capture, "Episode", return_value=FakeEpisode()),
                patch.object(controller.capture, "sha256", side_effect=sha),
                patch.object(controller.shared, "verify_ocr_pins"),
                patch.object(controller.subprocess, "Popen", return_value=FakeProcess()),
                patch.object(controller.shared, "InteractivePipe", FakePipe),
                patch.object(controller.shared, "ocr_words", side_effect=ocr),
                patch.object(controller, "query_visible", return_value=True),
                patch.object(controller, "checkbox_counts", side_effect=count_iter),
                patch.object(controller.time, "sleep"),
            ):
                if expected_error:
                    with self.assertRaisesRegex(ValueError, expected_error):
                        controller.action(
                            {"episode_id": "vlc-one", "task_id": controller.VLC_TASK},
                            directory,
                            binary,
                            fake_ocr,
                            fake_data,
                        )
                else:
                    controller.action(
                        {"episode_id": "vlc-one", "task_id": controller.VLC_TASK},
                        directory,
                        binary,
                        fake_ocr,
                        fake_data,
                    )
            decisions = json.loads((directory / "controller_decisions.json").read_text())
            return seen, decisions

    def test_save_after_initial_off_never_toggles_target(self):
        frames = ["main", "simple", "advanced_1006e", "search", "target_initial_off_1006e", "main"]
        counts = [FRAMES["target_initial_off_1006e"]["dark_lt_100"]]
        seen, decisions = self.run_sequence(frames, counts)
        actions = [action for operation, action in seen if operation == "action"]
        self.assertEqual(actions[-1], controller.policy()["actions"]["save"])
        self.assertFalse(any(action == {"action_type": "CLICK", "x": 906, "y": 395} for action in actions))
        self.assertEqual([item["state"] for item in decisions["target_states"]], ["unchecked"])
        self.assertEqual(decisions["status"], "finished")
        self.assertEqual(seen[-1][0], "finish")

    def test_red_and_mixed_frames_reopen_but_never_target_or_save_without_green(self):
        frames = [
            "main",
            "simple",
            "red_stale_1006c",
            "main",
            "simple",
            "mixed_ambiguous_1006d",
            "mixed_ambiguous_1006d",
            "mixed_ambiguous_1006d",
            "mixed_ambiguous_1006d",
            "main",
            "simple",
            "red_stale_1006c",
            "main",
        ]
        seen, decisions = self.run_sequence(frames, [], expected_error="no full Advanced")
        actions = [action for operation, action in seen if operation == "action"]
        self.assertEqual(actions.count(controller.policy()["actions"]["close_preferences"]), 3)
        self.assertFalse(any(action == {"action_type": "CLICK", "x": 906, "y": 395} for action in actions))
        self.assertNotIn(controller.policy()["actions"]["save"], actions)
        self.assertEqual(decisions["status"], "failed")

    def test_initially_checked_target_aborts_without_toggle_or_save(self):
        frames = ["main", "simple", "advanced_1006e", "search", "target_on_1006e"]
        seen, decisions = self.run_sequence(
            frames, [FRAMES["target_on_1006e"]["dark_lt_100"]], expected_error="initial-off expected unchecked"
        )
        actions = [action for operation, action in seen if operation == "action"]
        self.assertFalse(any(action == {"action_type": "CLICK", "x": 906, "y": 395} for action in actions))
        self.assertNotIn(controller.policy()["actions"]["save"], actions)
        self.assertEqual(decisions["status"], "failed")

    def test_ambiguous_target_pixels_abort_without_save(self):
        frames = ["main", "simple", "advanced_1006e", "search", "target_initial_off_1006e"]
        seen, decisions = self.run_sequence(
            frames,
            [{"target": 7, "checked_control": 20, "unchecked_control": 0}],
            expected_error="pixels are ambiguous",
        )
        actions = [action for operation, action in seen if operation == "action"]
        self.assertNotIn(controller.policy()["actions"]["save"], actions)
        self.assertEqual(decisions["status"], "failed")

    def test_ambiguous_without_advanced_title_never_sends_escape(self):
        frames = ["main", "simple", "blank", "blank", "blank", "blank"]
        seen, decisions = self.run_sequence(frames, [], expected_error="unverified Advanced Preferences")
        actions = [action for operation, action in seen if operation == "action"]
        self.assertNotIn(controller.policy()["actions"]["close_preferences"], actions)
        self.assertNotIn(controller.policy()["actions"]["save"], actions)
        self.assertEqual(decisions["status"], "failed")


if __name__ == "__main__":
    unittest.main()
