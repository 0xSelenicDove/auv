"""Guest-local relay preflight and fake-child tests; no cluster or GUI."""

import hashlib
import io
import json
import os
from pathlib import Path
import socket
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import agent_action_guest_local as guest  # noqa: E402
from test_agent_action_transport import FAKE_CHILD  # noqa: E402


DEVICE = "a" * 64


class GuestLocalRelayTest(unittest.TestCase):
    def setUp(self):
        platform = patch.object(guest.sys, "platform", "linux")
        platform.start()
        self.addCleanup(platform.stop)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.directory = self.root / "episode"
        self.directory.mkdir()
        self.sock = socket.socket(socket.AF_UNIX)
        self.addCleanup(self.sock.close)
        self.socket_path = self.root / "auv.sock"
        self.sock.bind(str(self.socket_path))
        self.endpoint = "unix://" + str(self.socket_path)
        self.auv = self.root / "auv"
        self.auv.write_text(f"#!{sys.executable}\nimport json\nprint(json.dumps([{{'source':'daemon','local':True,'status':'online','device_id':'{DEVICE}'}}]))\n")
        self.auv.chmod(0o700)
        self.auv_hash = hashlib.sha256(self.auv.read_bytes()).hexdigest()
        self.action = self.root / "auv-osworld-action"
        self.action.write_text(f"#!{sys.executable}\n" + FAKE_CHILD)
        self.action.chmod(0o700)
        self.action_hash = hashlib.sha256(self.action.read_bytes()).hexdigest()

    def prepare(self, *, endpoint=None, device=DEVICE):
        return guest.prepare(self.directory, self.action, self.action_hash, self.auv, self.auv_hash,
                             self.endpoint if endpoint is None else endpoint, device)

    def test_owner_socket_and_online_daemon_bind_context(self):
        directory, action, context = self.prepare()
        self.assertEqual(directory, self.directory)
        self.assertEqual(action, self.action)
        self.assertEqual(context, {"version": 1, "context": {"kind": "guest-local",
            "device_id": DEVICE, "daemon_endpoint": self.endpoint}})

    def test_rejects_tcp_symlink_regular_file_and_wrong_owner(self):
        with self.assertRaisesRegex(ValueError, "Unix absolute"):
            self.prepare(endpoint="http://127.0.0.1:9847")
        regular = self.root / "regular"
        regular.write_text("not a socket")
        with self.assertRaisesRegex(ValueError, "owner Unix socket"):
            self.prepare(endpoint="unix://" + str(regular))
        link = self.root / "linked.sock"
        link.symlink_to(self.socket_path)
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.prepare(endpoint="unix://" + str(link))
        real = guest.owner_socket
        with patch.object(guest.os, "geteuid", return_value=os.geteuid() + 1):
            with self.assertRaisesRegex(ValueError, "desktop owner"):
                self.prepare()
        self.assertEqual(real(self.endpoint, os.geteuid()), self.socket_path)

    def test_rejects_wrong_device_binary_hash_and_old_run(self):
        with self.assertRaisesRegex(ValueError, "unique online local Device"):
            self.prepare(device="b" * 64)
        with self.assertRaisesRegex(ValueError, "operator-pinned"):
            guest.prepare(self.directory, self.action, "0" * 64, self.auv, self.auv_hash,
                          self.endpoint, DEVICE)
        for name in ("agent_decisions.json", "action_evidence.json", "checkpoint-0002.png"):
            with self.subTest(name=name):
                path = self.directory / name
                path.write_text("old")
                with self.assertRaisesRegex(FileExistsError, "reuse"):
                    self.prepare()
                path.unlink()

    def test_socket_replacement_during_device_probe_fails_closed(self):
        replacement = None
        try:
            def replace(_auv, _endpoint, _device):
                nonlocal replacement
                self.socket_path.unlink()
                replacement = socket.socket(socket.AF_UNIX)
                replacement.bind(str(self.socket_path))

            with patch.object(guest, "online_device", side_effect=replace):
                with self.assertRaisesRegex(ValueError, "changed during Device probe"):
                    self.prepare()
        finally:
            if replacement is not None:
                replacement.close()

    def test_eof_cancels_guest_run_and_records_incomplete_trace(self):
        directory, action, context = self.prepare()
        context["mode"] = "happy"
        read_fd, write_fd = os.pipe()
        os.close(write_fd)
        output = io.StringIO()
        try:
            code = guest.run_session(directory, action, context, max_actions=1, max_captures=1,
                                     input_fd=read_fd, output=output)
        finally:
            os.close(read_fd)
        self.assertEqual(code, 1)
        self.assertEqual(json.loads((directory / "agent_decisions.json").read_text())["status"], "incomplete-eof")
        self.assertEqual(json.loads(output.getvalue().splitlines()[-1])["status"], "incomplete_eof")

    def test_main_rejects_excess_budget_before_child(self):
        argv = ["guest", "--episode-dir", str(self.directory), "--action-binary", str(self.action),
                "--action-sha256", self.action_hash, "--auv-binary", str(self.auv),
                "--auv-sha256", self.auv_hash, "--daemon-endpoint", self.endpoint,
                "--device-id", DEVICE, "--max-actions", "33"]
        with patch.object(sys, "argv", argv), patch.object(sys, "stdout", io.StringIO()) as output:
            self.assertEqual(guest.main(), 1)
        self.assertIn("budget", output.getvalue())
        self.assertFalse((self.directory / "agent-context.json").exists())

    def test_one_run_capture_action_finish_uses_shared_gateway(self):
        directory, action, context = self.prepare()
        # The fake child alone consumes this test-only mode; the real guest
        # context produced by prepare does not contain it.
        context["mode"] = "happy"
        name = "checkpoint-0001.png"
        digest = hashlib.sha256(b"\x89PNG\r\n\x1a\n" + name.encode()).hexdigest()
        proposals = [
            {"op": "capture", "seq": 1},
            {"op": "action", "seq": 2, "action": {"action_type": "CLICK", "x": 2, "y": 3},
             "based_on": {"run_id": "one-run", "path": name, "sha256": digest}},
            {"op": "finish", "seq": 3},
        ]
        read_fd, write_fd = os.pipe()
        os.write(write_fd, ("\n".join(json.dumps(value) for value in proposals) + "\n").encode())
        os.close(write_fd)
        output = io.StringIO()
        try:
            code = guest.run_session(directory, action, context, max_actions=2, max_captures=2,
                                     input_fd=read_fd, output=output)
        finally:
            os.close(read_fd)
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(code, 0)
        self.assertEqual([value["op"] for value in replies], ["ready", "receipt", "receipt", "receipt"])
        self.assertEqual(replies[1]["checkpoint_sha256"], digest)
        self.assertEqual(replies[-1]["status"], "finished")
        self.assertEqual(json.loads((directory / "agent_decisions.json").read_text())["status"], "finished")


if __name__ == "__main__":
    unittest.main()
