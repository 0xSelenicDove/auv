"""Offline foreground-child protocol checks; no GUI or cluster is contacted."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

from auv_osworld.agent_action_gateway import AgentActionGateway
from auv_osworld.agent_action_transport import ForegroundActionTransport

FAKE_CHILD = r"""
import hashlib
import json
import os
from pathlib import Path
import sys
import time

assert sys.argv[1:3] == ["--interactive", "--context"]
episode = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"])
assert os.environ["AUV_OSWORLD_ACTION_EVIDENCE"] == str(episode / "action_evidence.json")
mode = json.loads(Path(sys.argv[3]).read_text())["mode"]
(episode / "child-pgid.json").write_text(json.dumps(os.getpgrp()))
if mode == "exit-before-ready":
    sys.exit(3)
if mode == "hang-before-ready":
    time.sleep(30)
ready = {"op":"ready","version":1,"run_id":"one-run","limits":{"actions":1000,"captures":32}}
(episode / "action_evidence.json").write_text(json.dumps({"run_ids":["one-run"],"final_artifact":None}))
print(json.dumps(ready), flush=True)
if mode == "exit-after-ready":
    # Exit 1 alone is not an abort receipt without a terminal JSONL line.
    sys.exit(1)
captures = 0
actions = 0
terminal = False
for line in sys.stdin:
    request = json.loads(line)
    path = episode / "action-requests.json"
    requests = json.loads(path.read_text()) if path.exists() else []
    requests.append(request)
    path.write_text(json.dumps(requests))
    if mode == "hang-after-request":
        time.sleep(30)
    if mode == "malformed-response":
        print("not json", flush=True)
        continue
    if mode == "oversized-response":
        print("x" * 65537, flush=True)
        continue
    if request["op"] == "capture":
        captures += 1
        name = f"checkpoint-{captures:04}.png"
        data = b"\x89PNG\r\n\x1a\n" + name.encode()
        (episode / name).write_bytes(data)
        artifact = {"path":name,"sha256":hashlib.sha256(data).hexdigest()}
        path = episode / "checkpoints.json"
        values = json.loads(path.read_text()) if path.exists() else []
        values.append(artifact)
        path.write_text(json.dumps(values))
        response = {"seq":request["seq"],"op":"capture","artifact":artifact}
    elif request["op"] == "action":
        actions += 1
        delivery = [{"attempts":[{"succeeded":True}],"result":"fake"}]
        path = episode / "input-action-results.json"
        values = json.loads(path.read_text()) if path.exists() else []
        values.append(delivery)
        path.write_text(json.dumps(values))
        response = {"seq":request["seq"],"op":"action","delivery":delivery}
    else:
        artifact = None
        if request["op"] == "finish":
            data = b"\x89PNG\r\n\x1a\nfinal"
            (episode / "final-screenshot.png").write_bytes(data)
            artifact = {"path":"final-screenshot.png","sha256":hashlib.sha256(data).hexdigest()}
        response = {"run_ids":["one-run"],"final_artifact":artifact}
        sidecar = response
        if mode == "abort-wrong-sidecar":
            sidecar = {"run_ids":["foreign-run"],"final_artifact":None}
        (episode / "action_evidence.json").write_text(json.dumps(sidecar))
    print(json.dumps(response), flush=True)
    if request["op"] in {"finish", "abort"}:
        terminal = True
        break
if terminal and request["op"] == "abort":
    sys.exit(2 if mode == "abort-crash" else 1)
if not terminal:
    # Rust treats stdin EOF as Run cancellation and emits the sidecar, but
    # there is no terminal request for the transport to accept.
    response = {"run_ids":["one-run"],"final_artifact":None}
    (episode / "action_evidence.json").write_text(json.dumps(response))
    print(json.dumps(response), flush=True)
    sys.exit(1)
"""


class ForegroundActionTransportTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.binary = self.directory / "fake-auv-osworld-action"
        self.binary.write_text(f"#!{sys.executable}\n" + FAKE_CHILD)
        self.binary.chmod(0o700)
        self.context = self.directory / "context.json"
        self.context.write_text(json.dumps({"mode": "happy"}))

    def transport(self, *, response_timeout=2.0):
        return ForegroundActionTransport(self.binary, self.context, self.directory, response_timeout=response_timeout)

    def mode(self, value):
        self.context.write_text(json.dumps({"mode": value}))

    def test_one_run_capture_action_capture_finish_with_provenance(self):
        with self.transport() as child:
            self.assertEqual(json.loads((self.directory / "child-pgid.json").read_text()), os.getpgrp())
            gateway = AgentActionGateway(self.directory, child.ready, child.exchange, max_actions=1, max_captures=2)
            first = gateway.submit({"op": "capture", "seq": 1})["artifact"]
            provenance = {"run_id": "one-run", **first}
            with self.assertRaisesRegex(ValueError, "provenance"):
                gateway.submit(
                    {"op": "action", "seq": 2, "action": {"action_type": "CLICK", "x": 5, "y": 6}, "based_on": None}
                )
            gateway.submit(
                {"op": "action", "seq": 2, "action": {"action_type": "CLICK", "x": 5, "y": 6}, "based_on": provenance}
            )
            second = gateway.submit({"op": "capture", "seq": 3})["artifact"]
            self.assertNotEqual(first, second)
            terminal = gateway.submit({"op": "finish", "seq": 4})
            self.assertEqual(terminal["run_ids"], ["one-run"])
            self.assertEqual(child.process.returncode, 0)
        self.assertEqual(json.loads((self.directory / "action_evidence.json").read_text()), terminal)
        self.assertEqual(len(json.loads((self.directory / "checkpoints.json").read_text())), 2)
        self.assertEqual(len(json.loads((self.directory / "input-action-results.json").read_text())), 1)
        self.assertEqual(
            [request["op"] for request in json.loads((self.directory / "action-requests.json").read_text())],
            ["capture", "action", "capture", "finish"],
        )
        trace = json.loads((self.directory / "agent_decisions.json").read_text())
        self.assertEqual(trace["status"], "finished")
        self.assertEqual(len(trace["receipts"]), 4)
        self.assertEqual(trace["receipts"][1]["proposal"]["based_on"], provenance)
        self.assertEqual(
            {receipt["response"].get("run_ids", ["one-run"])[0] for receipt in trace["receipts"]}, {"one-run"}
        )

    def test_abort_is_terminal_and_reaped(self):
        with self.transport() as child:
            gateway = AgentActionGateway(self.directory, child.ready, child.exchange, max_actions=1, max_captures=1)
            self.assertIsNone(gateway.submit({"op": "abort", "seq": 1})["final_artifact"])
            self.assertEqual(child.process.returncode, 1)
            self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"], "aborted")
        self.assertFalse((self.directory / "final-screenshot.png").exists())

    def test_abort_nonzero_requires_exact_expected_exit_and_verified_sidecar(self):
        for mode, error in [("abort-crash", RuntimeError), ("abort-wrong-sidecar", ValueError)]:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                self.directory = Path(temporary)
                self.binary = self.directory / "fake-auv-osworld-action"
                self.binary.write_text(f"#!{sys.executable}\n" + FAKE_CHILD)
                self.binary.chmod(0o700)
                self.context = self.directory / "context.json"
                self.mode(mode)
                with self.transport() as child:
                    gateway = AgentActionGateway(
                        self.directory, child.ready, child.exchange, max_actions=1, max_captures=1
                    )
                    with self.assertRaises(error):
                        gateway.submit({"op": "abort", "seq": 1})
                    self.assertEqual(
                        json.loads((self.directory / "agent_decisions.json").read_text())["status"],
                        "failed-after-forward",
                    )
                    self.assertIsNotNone(child.process.returncode)

    def test_cancellation_closes_stdin_and_reaps_without_terminal_request(self):
        with self.assertRaisesRegex(KeyboardInterrupt, "cancel"):
            with self.transport() as child:
                pid = child.process.pid
                raise KeyboardInterrupt("cancel")
        self.assertIsNotNone(child.process.returncode)
        self.assertEqual(child.process.returncode, 1)
        self.assertEqual(
            json.loads((self.directory / "action_evidence.json").read_text()),
            {"run_ids": ["one-run"], "final_artifact": None},
        )
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_child_exits_before_ready_or_before_response(self):
        self.mode("exit-before-ready")
        with self.assertRaisesRegex(EOFError, "before response"):
            with self.transport():
                pass
        self.mode("exit-after-ready")
        with self.transport() as child:
            gateway = AgentActionGateway(self.directory, child.ready, child.exchange, max_actions=1, max_captures=1)
            with self.assertRaises((EOFError, BrokenPipeError, TimeoutError)):
                gateway.submit({"op": "capture", "seq": 1})
            self.assertTrue(gateway.closed)
            self.assertEqual(
                json.loads((self.directory / "agent_decisions.json").read_text())["status"], "failed-after-forward"
            )
            self.assertEqual(child.process.returncode, 1)

    def test_timeout_and_malformed_response_close_and_reap(self):
        for mode, error in [
            ("hang-before-ready", TimeoutError),
            ("hang-after-request", TimeoutError),
            ("malformed-response", json.JSONDecodeError),
            ("oversized-response", ValueError),
        ]:
            with self.subTest(mode=mode):
                with tempfile.TemporaryDirectory() as temporary:
                    self.directory = Path(temporary)
                    self.binary = self.directory / "fake-auv-osworld-action"
                    self.binary.write_text(f"#!{sys.executable}\n" + FAKE_CHILD)
                    self.binary.chmod(0o700)
                    self.context = self.directory / "context.json"
                    self.mode(mode)
                    child = self.transport(response_timeout=0.05 if mode == "hang-before-ready" else 2.0)
                    if mode == "hang-before-ready":
                        with self.assertRaises(error):
                            child.__enter__()
                    else:
                        with child:
                            if mode == "hang-after-request":
                                child.response_timeout = 0.05
                            with self.assertRaises(error):
                                child.exchange({"op": "capture", "seq": 1})
                    self.assertTrue(child.closed)
                    self.assertIsNotNone(child.process.returncode)


if __name__ == "__main__":
    unittest.main()
