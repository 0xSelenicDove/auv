"""The agent gate rejects stale decisions before the AUV JSONL pipe is called."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from auv_osworld.agent_action_gateway import AgentActionGateway


class FakeAuv:
    def __init__(self, directory):
        self.directory = directory
        self.requests = []
        self.bad_receipt = False

    def __call__(self, request):
        self.requests.append(request)
        seq = request["seq"]
        if request["op"] == "capture":
            name = f"checkpoint-{len([item for item in self.requests if item['op'] == 'capture']):04}.png"
            image = self.directory / name
            image.write_bytes(b"\x89PNG\r\n\x1a\n" + name.encode())
            artifact = {"path": name, "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}
            index = self.directory / "checkpoints.json"
            items = json.loads(index.read_text()) if index.exists() else []
            items.append(artifact)
            index.write_text(json.dumps(items))
            return {"seq": seq + (1 if self.bad_receipt else 0), "op": "capture", "artifact": artifact}
        if request["op"] == "action":
            delivery = [{"result": "fake"}]
            path = self.directory / "input-action-results.json"
            results = json.loads(path.read_text()) if path.exists() else []
            results.append(delivery)
            path.write_text(json.dumps(results))
            return {"seq": seq, "op": "action", "delivery": delivery}
        artifact = None
        if request["op"] == "finish":
            image = self.directory / "final-screenshot.png"
            image.write_bytes(b"\x89PNG\r\n\x1a\nfinal")
            artifact = {"path": image.name, "sha256": hashlib.sha256(image.read_bytes()).hexdigest()}
        terminal = {"run_ids": ["run-1"], "final_artifact": artifact}
        (self.directory / "action_evidence.json").write_text(json.dumps(terminal))
        return terminal


class AgentActionGatewayTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.auv = FakeAuv(self.directory)
        self.ready = {"op": "ready", "version": 1, "run_id": "run-1", "limits": {"actions": 1000, "captures": 32}}
        self.gateway = AgentActionGateway(self.directory, self.ready, self.auv, max_actions=2, max_captures=2)

    def capture(self, seq=1):
        return self.gateway.submit({"op": "capture", "seq": seq})

    def test_action_requires_verified_checkpoint_and_persists_sequential_receipts(self):
        click = {"action_type": "CLICK", "x": 10, "y": 20}
        with self.assertRaisesRegex(ValueError, "screenshot provenance"):
            self.gateway.submit({"op": "action", "seq": 1, "action": click, "based_on": None})
        self.assertEqual(self.auv.requests, [])
        frame = self.capture()["artifact"]
        provenance = {"run_id": "run-1", **frame}
        result = self.gateway.submit({"op": "action", "seq": 2, "action": click, "based_on": provenance})
        self.assertEqual(result["seq"], 2)
        self.assertEqual(self.auv.requests[1], {"op": "action", "seq": 2, "action": click})
        recorded = json.loads((self.directory / "agent_decisions.json").read_text())
        self.assertEqual(recorded["run_id"], "run-1")
        self.assertEqual(recorded["receipts"][1]["proposal"]["based_on"], provenance)
        self.assertIsNone(recorded["pending"])
        with self.assertRaisesRegex(ValueError, "screenshot provenance"):
            self.gateway.submit({"op": "action", "seq": 3, "action": click, "based_on": provenance})
        self.assertEqual(len(self.auv.requests), 2)

    def test_forbidden_operation_malformed_action_and_replay_do_not_forward(self):
        self.capture()
        provenance = self.gateway.latest_checkpoint
        invalid = [
            {"op": "shell", "seq": 2, "command": "xdotool click 1"},
            {"op": "action", "seq": 2, "action": "pyautogui.click(1, 2)", "based_on": provenance},
            {"op": "action", "seq": 2, "action": {"command": "xdotool click 1"}, "based_on": provenance},
            {
                "op": "action",
                "seq": 2,
                "action": {"action_type": "EXECUTE", "command": "xdotool click 1"},
                "based_on": provenance,
            },
            {"op": "action", "seq": 2, "action": {"action_type": "WAIT"}, "based_on": provenance},
            {"op": "action", "seq": 1, "action": {"action_type": "CLICK", "x": 1, "y": 2}, "based_on": provenance},
            {"op": "capture", "seq": True},
        ]
        for proposal in invalid:
            with self.subTest(proposal=proposal), self.assertRaises(ValueError):
                self.gateway.submit(proposal)
        self.assertEqual(len(self.auv.requests), 1)

    def test_budget_exhaustion_precedes_forward(self):
        self.capture()
        first = self.gateway.latest_checkpoint
        action = {"action_type": "CLICK", "x": 1, "y": 2}
        self.gateway.submit({"op": "action", "seq": 2, "action": action, "based_on": first})
        self.capture(3)
        second = self.gateway.latest_checkpoint
        self.gateway.submit({"op": "action", "seq": 4, "action": action, "based_on": second})
        with self.assertRaisesRegex(ValueError, "screenshot budget"):
            self.gateway.submit({"op": "capture", "seq": 5})
        with self.assertRaisesRegex(ValueError, "action budget"):
            self.gateway.submit({"op": "action", "seq": 5, "action": action, "based_on": second})
        self.assertEqual(len(self.auv.requests), 4)

    def test_wrong_run_stale_or_tampered_checkpoint_does_not_authorize_action(self):
        self.capture()
        observed = self.gateway.latest_checkpoint.copy()
        for changed in ({**observed, "run_id": "other"}, {**observed, "sha256": "0" * 64}):
            with self.assertRaisesRegex(ValueError, "screenshot provenance"):
                self.gateway.submit({"op": "action", "seq": 2, "action": {"action_type": "CLICK"}, "based_on": changed})
        self.assertEqual(len(self.auv.requests), 1)
        self.auv.bad_receipt = True
        # A response mismatch closes the gateway; forwarding may already have
        # happened, so no retry is permitted on the same Run.
        with self.assertRaisesRegex(ValueError, "response sequence"):
            self.capture(2)
        self.assertTrue(self.gateway.closed)
        self.assertEqual(
            json.loads((self.directory / "agent_decisions.json").read_text())["status"], "failed-after-forward"
        )

    def test_tampered_capture_bytes_fail_closed_after_forward(self):
        def tampered(request):
            response = self.auv(request)
            (self.directory / response["artifact"]["path"]).write_bytes(b"not the captured bytes")
            return response

        self.gateway.exchange = tampered
        with self.assertRaisesRegex(ValueError, "checkpoint bytes"):
            self.capture()
        self.assertTrue(self.gateway.closed)
        self.assertEqual(len(self.auv.requests), 1)

    def test_capture_changed_after_receipt_cannot_authorize_action(self):
        self.capture()
        provenance = self.gateway.latest_checkpoint.copy()
        (self.directory / provenance["path"]).write_bytes(b"changed after capture")
        with self.assertRaisesRegex(ValueError, "checkpoint bytes"):
            self.gateway.submit(
                {"op": "action", "seq": 2, "action": {"action_type": "CLICK", "x": 1, "y": 2}, "based_on": provenance}
            )
        self.assertEqual(len(self.auv.requests), 1)

    def test_rejects_invalid_budget_and_existing_trace(self):
        with self.assertRaisesRegex(ValueError, "budget"):
            AgentActionGateway(self.directory, self.ready, self.auv, max_actions=1001, max_captures=1)
        with self.assertRaisesRegex(FileExistsError, "reuse"):
            AgentActionGateway(self.directory, self.ready, self.auv, max_actions=1, max_captures=1)

    def test_finish_and_abort_have_distinct_verified_terminal_states(self):
        final = self.gateway.submit({"op": "finish", "seq": 1})
        self.assertEqual(final["run_ids"], ["run-1"])
        self.assertEqual(json.loads((self.directory / "agent_decisions.json").read_text())["status"], "finished")
        with self.assertRaisesRegex(ValueError, "closed"):
            self.gateway.submit({"op": "capture", "seq": 2})

        second = tempfile.TemporaryDirectory()
        self.addCleanup(second.cleanup)
        directory = Path(second.name)
        aborted = AgentActionGateway(directory, self.ready, FakeAuv(directory), max_actions=1, max_captures=1)
        self.assertIsNone(aborted.submit({"op": "abort", "seq": 1})["final_artifact"])
        self.assertEqual(json.loads((directory / "agent_decisions.json").read_text())["status"], "aborted")

    def test_malformed_or_foreign_terminal_receipt_fails_closed(self):
        for response_change, error in [
            (lambda response: {}, "different Run or schema"),
            (lambda response: {**response, "run_ids": ["other-run"]}, "different Run or schema"),
            (lambda response: {**response, "final_artifact": None}, "lacks its final screenshot"),
            (
                lambda response: {**response, "final_artifact": {"path": "../other.png", "sha256": "0" * 64}},
                "lacks its final screenshot",
            ),
        ]:
            with self.subTest(error=error):
                temporary = tempfile.TemporaryDirectory()
                self.addCleanup(temporary.cleanup)
                directory = Path(temporary.name)
                auv = FakeAuv(directory)

                def exchanged(request):
                    return response_change(auv(request))

                gateway = AgentActionGateway(directory, self.ready, exchanged, max_actions=1, max_captures=1)
                with self.assertRaisesRegex(ValueError, error):
                    gateway.submit({"op": "finish", "seq": 1})
                trace = json.loads((directory / "agent_decisions.json").read_text())
                self.assertEqual(trace["status"], "failed-after-forward")
                self.assertEqual(len(trace["receipts"]), 0)
                self.assertEqual(len(auv.requests), 1)

    def test_terminal_sidecar_and_final_bytes_must_match(self):
        for tamper, error in [
            (lambda directory: (directory / "action_evidence.json").write_text("{}"), "durable sidecar"),
            (lambda directory: (directory / "final-screenshot.png").write_bytes(b"changed"), "final screenshot bytes"),
        ]:
            with self.subTest(error=error):
                temporary = tempfile.TemporaryDirectory()
                self.addCleanup(temporary.cleanup)
                directory = Path(temporary.name)
                auv = FakeAuv(directory)

                def exchanged(request):
                    response = auv(request)
                    tamper(directory)
                    return response

                gateway = AgentActionGateway(directory, self.ready, exchanged, max_actions=1, max_captures=1)
                with self.assertRaisesRegex(ValueError, error):
                    gateway.submit({"op": "finish", "seq": 1})
                self.assertTrue(gateway.closed)

    def test_action_delivery_must_match_durable_result_and_count(self):
        for tamper, error in [
            (lambda path: path.unlink(), "missing or invalid"),
            (lambda path: path.write_text(json.dumps([[{"result": "different"}]])), "differs from durable"),
            (lambda path: path.write_text(json.dumps([])), "differs from durable"),
        ]:
            with self.subTest(error=error):
                temporary = tempfile.TemporaryDirectory()
                self.addCleanup(temporary.cleanup)
                directory = Path(temporary.name)
                auv = FakeAuv(directory)

                def exchanged(request):
                    response = auv(request)
                    if request["op"] == "action":
                        tamper(directory / "input-action-results.json")
                    return response

                gateway = AgentActionGateway(directory, self.ready, exchanged, max_actions=1, max_captures=1)
                artifact = gateway.submit({"op": "capture", "seq": 1})["artifact"]
                with self.assertRaisesRegex(ValueError, error):
                    gateway.submit(
                        {
                            "op": "action",
                            "seq": 2,
                            "action": {"action_type": "CLICK", "x": 1, "y": 2},
                            "based_on": {"run_id": "run-1", **artifact},
                        }
                    )
                self.assertTrue(gateway.closed)
                trace = json.loads((directory / "agent_decisions.json").read_text())
                self.assertEqual(trace["status"], "failed-after-forward")
                self.assertEqual(len(trace["receipts"]), 1)


if __name__ == "__main__":
    unittest.main()
