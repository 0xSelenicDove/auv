"""Offline-checkable decision gate for one AUV interactive action Run.

The caller owns the model and the foreground JSONL pipe. This gate accepts
model proposals, binds each GUI action to a verified AUV checkpoint, and
forwards only the original typed action to ``auv-osworld-action``. The Rust
entry remains the authority for OSWorld action parameters and delivery.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
from collections.abc import Callable
from pathlib import Path

from .integrity import sha256

CHECKPOINT = re.compile(r"checkpoint-[0-9]{4}\.png\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
GUI_CONTROL_ACTIONS = {"WAIT", "DONE", "FAIL", "EXECUTE"}


class AgentActionGateway:
    """Validate model decisions and AUV receipts without owning GUI input.

    ``exchange`` must send one request to the already-started interactive
    binary and return its next JSONL response. It must not restart or retry the
    child. The gate becomes unusable after any ambiguous exchange failure.
    """

    def __init__(
        self, directory: Path, ready: dict, exchange: Callable[[dict], dict], *, max_actions: int, max_captures: int
    ):
        self.directory = directory.resolve(strict=True)
        if not self.directory.is_dir():
            raise ValueError("episode directory is not a directory")
        if not isinstance(ready, dict) or ready.get("op") != "ready" or ready.get("version") != 1:
            raise ValueError("interactive AUV ready receipt is invalid")
        self.run_id = ready.get("run_id")
        limits = ready.get("limits")
        if not isinstance(self.run_id, str) or not self.run_id or not isinstance(limits, dict):
            raise ValueError("interactive AUV Run ID or limits are missing")
        for name, requested in (("actions", max_actions), ("captures", max_captures)):
            advertised = limits.get(name)
            if type(requested) is not int or requested < 1 or type(advertised) is not int or requested > advertised:
                raise ValueError(f"agent {name} budget exceeds interactive AUV limit")
        self.exchange = exchange
        self.max_actions = max_actions
        self.max_captures = max_captures
        self.actions = 0
        self.captures = 0
        self.next_seq = 1
        self.latest_checkpoint: dict | None = None
        self.closed = False
        self.trace_path = self.directory / "agent_decisions.json"
        if self.trace_path.exists():
            raise FileExistsError("refusing to reuse agent decision trace")
        self.trace = {
            "version": 1,
            "run_id": self.run_id,
            "limits": {"actions": max_actions, "captures": max_captures},
            "receipts": [],
            "pending": None,
        }
        self._persist()

    def _persist(self) -> None:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=self.directory, delete=False) as output:
            path = Path(output.name)
            try:
                json.dump(self.trace, output, sort_keys=True, separators=(",", ":"))
                output.flush()
                os.fsync(output.fileno())
                os.replace(path, self.trace_path)
            finally:
                path.unlink(missing_ok=True)

    def _checkpoint(self, artifact: object) -> dict:
        if not isinstance(artifact, dict) or set(artifact) != {"path", "sha256"}:
            raise ValueError("AUV checkpoint receipt has invalid artifact schema")
        name, digest = artifact["path"], artifact["sha256"]
        if (
            not isinstance(name, str)
            or not CHECKPOINT.fullmatch(name)
            or not isinstance(digest, str)
            or not DIGEST.fullmatch(digest)
        ):
            raise ValueError("AUV checkpoint receipt has invalid identity")
        image = (self.directory / name).resolve(strict=True)
        if not image.is_relative_to(self.directory) or sha256(image) != digest:
            raise ValueError("AUV checkpoint bytes differ from receipt")
        index = json.loads((self.directory / "checkpoints.json").read_text(encoding="utf-8"))
        if not isinstance(index, list) or len(index) != self.captures or index[-1] != artifact:
            raise ValueError("AUV checkpoint index differs from receipt")
        return {"run_id": self.run_id, "path": name, "sha256": digest}

    def _terminal(self, op: str, response: dict) -> None:
        if set(response) != {"run_ids", "final_artifact"} or response["run_ids"] != [self.run_id]:
            raise ValueError("interactive AUV terminal receipt has a different Run or schema")
        artifact = response["final_artifact"]
        if op == "finish":
            if (
                not isinstance(artifact, dict)
                or set(artifact) != {"path", "sha256"}
                or artifact["path"] != "final-screenshot.png"
            ):
                raise ValueError("interactive AUV finish receipt lacks its final screenshot")
            digest = artifact["sha256"]
            if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
                raise ValueError("interactive AUV final screenshot digest is invalid")
            image = (self.directory / "final-screenshot.png").resolve(strict=True)
            if not image.is_relative_to(self.directory) or sha256(image) != digest:
                raise ValueError("interactive AUV final screenshot bytes differ from receipt")
        elif artifact is not None:
            raise ValueError("interactive AUV abort receipt unexpectedly has a final screenshot")
        sidecar = json.loads((self.directory / "action_evidence.json").read_text(encoding="utf-8"))
        if sidecar != response:
            raise ValueError("interactive AUV terminal receipt differs from durable sidecar")

    def submit(self, proposal: dict) -> dict:
        """Validate, forward exactly once, and durably record a sequential receipt."""
        if self.closed:
            raise ValueError("agent gateway is closed")
        if not isinstance(proposal, dict) or proposal.get("op") not in {"capture", "action", "finish", "abort"}:
            raise ValueError("agent proposal operation is forbidden")
        op = proposal["op"]
        expected_fields = {"op", "seq", "action", "based_on"} if op == "action" else {"op", "seq"}
        if set(proposal) != expected_fields or type(proposal.get("seq")) is not int or proposal["seq"] != self.next_seq:
            raise ValueError("agent proposal schema or sequence is invalid")
        if op == "capture" and self.captures >= self.max_captures:
            raise ValueError("agent screenshot budget exhausted")
        if op == "action":
            if self.actions >= self.max_actions:
                raise ValueError("agent action budget exhausted")
            action = proposal["action"]
            if (
                not isinstance(action, dict)
                or not isinstance(action.get("action_type"), str)
                or not action["action_type"]
                or action["action_type"] in GUI_CONTROL_ACTIONS
            ):
                raise ValueError("agent action is not a typed GUI action")
            # NOTICE: Parameter semantics remain in Rust parse_action; the
            # gateway only rejects an absent/forbidden action shape here.
            if self.latest_checkpoint is None or proposal["based_on"] != self.latest_checkpoint:
                raise ValueError("agent action lacks the latest verified AUV screenshot provenance")
            # Recheck at decision time so a checkpoint changed after capture
            # cannot authorize input using an earlier valid digest.
            if (
                self._checkpoint({"path": self.latest_checkpoint["path"], "sha256": self.latest_checkpoint["sha256"]})
                != self.latest_checkpoint
            ):
                raise ValueError("agent screenshot provenance changed before action")
        request = {"op": op, "seq": self.next_seq}
        if op == "action":
            request["action"] = proposal["action"]
        self.trace["pending"] = {"proposal": proposal, "request": request}
        self._persist()
        try:
            response = self.exchange(request)
            if not isinstance(response, dict):
                raise ValueError("interactive AUV response is not an object")
            if op in {"capture", "action"} and (response.get("op") != op or response.get("seq") != self.next_seq):
                raise ValueError("interactive AUV response sequence or operation differs")
            if op == "capture":
                self.captures += 1
                self.latest_checkpoint = self._checkpoint(response.get("artifact"))
            elif op == "action":
                if not isinstance(response.get("delivery"), list):
                    raise ValueError("interactive AUV action delivery receipt is missing")
                try:
                    deliveries = json.loads((self.directory / "input-action-results.json").read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError) as error:
                    raise ValueError("interactive AUV durable action results are missing or invalid") from error
                if (
                    not isinstance(deliveries, list)
                    or len(deliveries) != self.actions + 1
                    or deliveries[-1] != response["delivery"]
                ):
                    raise ValueError("interactive AUV action delivery differs from durable results")
                self.actions += 1
                self.latest_checkpoint = None
            else:
                self._terminal(op, response)
                self.trace["status"] = "finished" if op == "finish" else "aborted"
                self.closed = True
            self.trace["receipts"].append({"proposal": proposal, "request": request, "response": response})
            self.trace["pending"] = None
            self._persist()
            self.next_seq += 1
            return response
        except Exception:
            self.closed = True
            self.trace["status"] = "failed-after-forward"
            self._persist()
            raise


# TODO: A model policy and tool sandbox remain out of this transport slice;
# add them only with an owner-approved agent/batch identity. The gateway and
# foreground child transport alone do not isolate a model's other tools.
