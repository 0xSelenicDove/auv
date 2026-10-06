"""Offline relay checks use one fake AUV child and no Kubernetes connection."""

from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import agent_action_relay as relay  # noqa: E402
from test_agent_action_transport import FAKE_CHILD  # noqa: E402


class FakeEpisode:
    forwards = []

    def __init__(self, config, directory):
        self.config = config
        self.directory = directory

    @contextmanager
    def forward(self, *, setup=False, auv=False):
        self.forwards.append((setup, auv))
        yield


class AgentActionRelayTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.directory = self.root / "episode-1"
        self.directory.mkdir()
        self.config_path = self.directory / "config.json"
        self.config_path.write_text("{}")
        self.config = {"episode_id": "episode-1", "auv_local_port": 38001}
        self.binary = self.root / "fake-action"
        self.binary.write_text(f"#!{sys.executable}\n" + FAKE_CHILD)
        self.binary.chmod(0o700)
        self.binary_hash = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        (self.directory / "paired-device.json").write_text(json.dumps({"device_id": "device-1",
            "guest_auv_sha256": relay.GUEST_AUV_SHA256}))
        self.profile_path = self.directory / "paired-profiles.json"
        self.profile_path.write_text(json.dumps({"profiles": {"episode-1": {
            "device_id": "device-1", "device_name": "guest", "endpoint": "http://127.0.0.1:38001",
            "device_credential": "test-secret"}}}))
        self.config_patch = patch.object(relay, "load_config", return_value=self.config)
        self.config_patch.start()
        self.addCleanup(self.config_patch.stop)

    def prepare(self):
        return relay.prepare(self.config_path, self.directory, self.binary, self.binary_hash)

    def run_relay(self, lines, *, context=None):
        config, directory, binary, prepared = self.prepare()
        # The fake child reads a test-only mode field; the real context from
        # prepare is asserted separately and contains no such field.
        prepared = {**prepared, "mode": "happy"} if context is None else context
        read_fd, write_fd = os.pipe()
        try:
            os.write(write_fd, lines)
        finally:
            os.close(write_fd)
        output = io.StringIO()
        FakeEpisode.forwards.clear()
        with patch.object(relay, "Episode", FakeEpisode):
            try:
                code = relay.relay(config, directory, binary, prepared,
                                   max_actions=2, max_captures=2, input_fd=read_fd, output=output)
            finally:
                os.close(read_fd)
        return code, [json.loads(line) for line in output.getvalue().splitlines()]

    def test_preflight_binds_episode_profile_device_and_binary(self):
        _, directory, _, context = self.prepare()
        self.assertEqual(context["context"], {"kind": "paired", "device_id": "device-1",
            "config_profile": "episode-1", "profiles_file": str(self.profile_path.resolve())})
        self.assertEqual(directory, self.directory.resolve())
        self.assertNotIn("test-secret", json.dumps(context))
        with self.assertRaisesRegex(ValueError, "operator-pinned"):
            relay.prepare(self.config_path, self.directory, self.binary, "0" * 64)
        self.profile_path.write_text(json.dumps({"profiles": {"episode-1": {
            "device_id": "different", "endpoint": "http://127.0.0.1:38001",
            "device_credential": "test-secret"}}}))
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.prepare()

    def test_rejects_existing_trace_and_sidecar_before_child(self):
        for name in ("agent_decisions.json", "action_evidence.json", "checkpoint-0002.png"):
            with self.subTest(name=name):
                path = self.directory / name
                path.write_text("existing")
                with self.assertRaisesRegex(FileExistsError, "reuse"):
                    self.prepare()
                path.unlink()

    def test_rejects_wrong_episode_or_profile_endpoint_before_child(self):
        self.config["episode_id"] = "other-episode"
        with self.assertRaisesRegex(ValueError, "episode directory"):
            self.prepare()
        self.config["episode_id"] = "episode-1"
        profile = json.loads(self.profile_path.read_text())
        profile["profiles"]["episode-1"]["endpoint"] = "http://127.0.0.1:9999"
        self.profile_path.write_text(json.dumps(profile))
        with self.assertRaisesRegex(ValueError, "reviewed AUV forward"):
            self.prepare()

    def test_capture_action_finish_through_one_forward_and_run(self):
        first_name = "checkpoint-0001.png"
        first_digest = hashlib.sha256(b"\x89PNG\r\n\x1a\n" + first_name.encode()).hexdigest()
        lines = [
            {"op": "capture", "seq": 1},
            {"op": "action", "seq": 2, "action": {"action_type": "CLICK", "x": 2, "y": 3},
             "based_on": {"run_id": "one-run", "path": first_name, "sha256": first_digest}},
            {"op": "finish", "seq": 3},
        ]
        code, output = self.run_relay(("\n".join(json.dumps(line) for line in lines) + "\n").encode())
        self.assertEqual(code, 0)
        self.assertEqual(FakeEpisode.forwards, [(False, True)])
        self.assertEqual([item["op"] for item in output], ["ready", "receipt", "receipt", "receipt"])
        self.assertEqual(output[1]["checkpoint_path"], str((self.directory / first_name).resolve()))
        self.assertEqual(output[1]["checkpoint_sha256"], first_digest)
        self.assertEqual(output[-1]["status"], "finished")
        self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"], "finished")
        self.assertEqual([entry["op"] for entry in json.loads((self.directory / "action-requests.json").read_text())],
                         ["capture", "action", "finish"])

    def test_explicit_abort_is_verified_terminal_failure_not_eof(self):
        code, output = self.run_relay(b'{"op":"abort","seq":1}\n')
        self.assertEqual(code, 1)
        self.assertEqual(output[-1]["op"], "receipt")
        self.assertEqual(output[-1]["status"], "aborted")
        self.assertIsNone(output[-1]["receipt"]["final_artifact"])
        self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"], "aborted")

    def test_malformed_proposal_and_eof_close_child_and_trace(self):
        for payload, status in [(b"not json\n", "failed-before-terminal"),
                                (b'{"op":"shell","seq":1}\n', "failed-before-terminal"),
                                (b"", "incomplete-eof")]:
            with self.subTest(payload=payload), tempfile.TemporaryDirectory() as temporary:
                # Give each attempt fresh evidence; the relay forbids reuse.
                original = self.directory
                self.directory = Path(temporary) / "episode-1"
                self.directory.mkdir()
                self.config_path = self.directory / "config.json"
                self.config_path.write_text("{}")
                (self.directory / "paired-device.json").write_text((original / "paired-device.json").read_text())
                (self.directory / "paired-profiles.json").write_text((original / "paired-profiles.json").read_text())
                code, output = self.run_relay(payload)
                self.assertEqual(code, 1)
                self.assertEqual(output[-1]["op"], "session_end")
                self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"], status)
                self.assertEqual(json.loads((self.directory / "action_evidence.json").read_text())["final_artifact"], None)
                self.directory = original

    def test_oversized_line_is_rejected_without_forwarding(self):
        with patch.object(relay, "MAX_LINE_BYTES", 32):
            code, output = self.run_relay(b" " * 40 + b"\n")
        self.assertEqual(code, 1)
        self.assertEqual(output[-1]["status"], "error")
        self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"],
                         "failed-before-terminal")
        self.assertFalse((self.directory / "action-requests.json").exists())


if __name__ == "__main__":
    unittest.main()
