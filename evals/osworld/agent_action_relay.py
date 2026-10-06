#!/usr/bin/env python3
"""Attended paired-remote JSONL relay for one OSWorld AUV action Run.

The caller supplies every proposal. This process makes no GUI decision, does
not call the evaluator, and only opens the already-owned Pod's AUV forward.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import sys
import time

from agent_action_gateway import AgentActionGateway, DIGEST
from agent_action_transport import ForegroundActionTransport, MAX_LINE_BYTES, MAX_SESSION_BYTES


# Keep the relay below Rust's 240-second idle and 570-second total limits.
IDLE_SECONDS = 180
SESSION_SECONDS = 540
MAX_PROPOSALS = 65  # 32 captures + 32 actions + one terminal proposal.
PRIOR_OUTPUTS = (
    "agent-context.json", "agent_decisions.json", "action_evidence.json",
    "action-requests.json", "input-action-results.json", "checkpoints.json",
    "final-screenshot.png", "checkpoint-0001.png",
)


def emit(value: dict, output) -> None:
    output.write(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
    output.flush()


def absolute_file(path: Path, label: str) -> Path:
    if not path.is_absolute():
        raise ValueError(f"{label} must be an absolute path")
    resolved = path.resolve(strict=True)
    if not resolved.is_file():
        raise ValueError(f"{label} must be a file")
    return resolved


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def adapter_module(adapter: str):
    if adapter == "v1":
        import k8s_phase_adapter
        return k8s_phase_adapter
    if adapter == "v2-task099":
        import k8s_v2_task099_adapter
        return k8s_v2_task099_adapter
    if adapter == "v2-task044":
        import k8s_v2_task044_adapter
        return k8s_v2_task044_adapter
    raise ValueError("unreviewed Kubernetes episode adapter")


def paired_context(config: dict, directory: Path, guest_auv_sha256: str) -> dict:
    """Check install evidence without printing the paired bearer credential."""
    paired = json.loads((directory / "paired-device.json").read_text(encoding="utf-8"))
    if not isinstance(paired, dict) or not isinstance(paired.get("device_id"), str) or not paired["device_id"]:
        raise ValueError("paired Device ID evidence is missing")
    if paired.get("guest_auv_sha256") != guest_auv_sha256:
        raise ValueError("paired install evidence has a different guest AUV binary")
    profiles_path = (directory / "paired-profiles.json").resolve(strict=True)
    if not profiles_path.is_file() or profiles_path.parent != directory:
        raise ValueError("paired profile must be an episode-local file")
    profiles = json.loads(profiles_path.read_text(encoding="utf-8"))
    profile = profiles.get("profiles", {}).get(config["episode_id"]) if isinstance(profiles, dict) else None
    if not isinstance(profile, dict) or profile.get("device_id") != paired["device_id"]:
        raise ValueError("paired profile does not match install Device ID")
    if profile.get("endpoint") != f"http://127.0.0.1:{config['auv_local_port']}":
        raise ValueError("paired profile does not target the reviewed AUV forward")
    if not isinstance(profile.get("device_credential"), str) or not profile["device_credential"]:
        raise ValueError("paired profile credential is missing")
    return {"version": 1, "context": {"kind": "paired", "device_id": paired["device_id"],
            "config_profile": config["episode_id"], "profiles_file": str(profiles_path)}}


def prepare(config_path: Path, directory_path: Path, binary_path: Path, binary_sha256: str,
            *, adapter: str = "v1") -> tuple[dict, Path, Path, dict]:
    selected = adapter_module(adapter)
    config_path = absolute_file(config_path, "config")
    if not directory_path.is_absolute():
        raise ValueError("episode directory must be an absolute path")
    directory = directory_path.resolve(strict=True)
    if not directory.is_dir():
        raise ValueError("episode directory must be a directory")
    if config_path.parent != directory:
        raise ValueError("config must belong to the selected episode directory")
    config = selected.load_config(config_path)
    if directory.name != config["episode_id"]:
        raise ValueError("episode directory does not match config episode ID")
    binary = absolute_file(binary_path, "action binary")
    if not DIGEST.fullmatch(binary_sha256) or sha256(binary) != binary_sha256:
        raise ValueError("action binary differs from the operator-pinned SHA256")
    if any((directory / name).exists() or (directory / name).is_symlink() for name in PRIOR_OUTPUTS):
        raise FileExistsError("refusing to reuse an existing action Run or trace")
    # The complete checkpoint range is reserved, including remnants of a
    # prior Run whose first checkpoint was removed.
    if any(directory.glob("checkpoint-*.png")):
        raise FileExistsError("refusing to reuse existing checkpoint artifacts")
    return config, directory, binary, paired_context(config, directory, selected.GUEST_AUV_SHA256)


def write_context(directory: Path, context: dict) -> Path:
    path = directory / "agent-context.json"
    # Exclusive creation means a competing relay cannot overwrite this Run's
    # context after the preflight check. A crash leaves evidence for refusal.
    with path.open("x", encoding="utf-8") as output:
        json.dump(context, output, sort_keys=True, separators=(",", ":"))
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    return path


def proposal_lines(input_fd: int, child: ForegroundActionTransport, *, deadline: float):
    """Read bounded raw lines so a partial/slow line cannot keep a Run open."""
    pending = bytearray()
    total = 0
    idle_deadline = time.monotonic() + IDLE_SECONDS
    while True:
        remaining = min(idle_deadline, deadline) - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("proposal idle or session deadline reached")
        if child.process is not None and child.process.poll() is not None:
            raise RuntimeError("interactive AUV child exited while awaiting a proposal")
        readable, _, _ = select.select([input_fd], [], [], min(remaining, 1.0))
        if not readable:
            continue
        chunk = os.read(input_fd, 4096)
        if not chunk:
            if pending:
                raise ValueError("proposal stream ended with an incomplete line")
            raise EOFError("proposal stream closed before terminal receipt")
        total += len(chunk)
        pending.extend(chunk)
        if total > MAX_SESSION_BYTES or len(pending) > MAX_LINE_BYTES + 4096:
            raise ValueError("proposal byte budget exceeded")
        while b"\n" in pending:
            line, _, rest = pending.partition(b"\n")
            pending = bytearray(rest)
            if len(line) > MAX_LINE_BYTES:
                raise ValueError("proposal line exceeds 64 KiB")
            try:
                proposal = json.loads(line)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                raise ValueError("proposal is not valid UTF-8 JSON") from error
            yield proposal
            idle_deadline = time.monotonic() + IDLE_SECONDS
        if len(pending) > MAX_LINE_BYTES:
            raise ValueError("proposal line exceeds 64 KiB")


def relay(config: dict, directory: Path, binary: Path, context: dict, *, max_actions: int,
          max_captures: int, input_fd: int, output, adapter: str = "v1") -> int:
    episode = adapter_module(adapter).Episode(config, directory)
    with episode.forward(auv=True):
        return run_session(directory, binary, context, max_actions=max_actions,
                           max_captures=max_captures, input_fd=input_fd, output=output)


def run_session(directory: Path, binary: Path, context: dict, *, max_actions: int,
                max_captures: int, input_fd: int, output) -> int:
    """Own one foreground AUV Run; topology-specific setup happens before this boundary."""
    context_path = write_context(directory, context)
    gate = None
    try:
        with ForegroundActionTransport(binary, context_path, directory) as child:
            gate = AgentActionGateway(directory, child.ready, child.exchange,
                                      max_actions=max_actions, max_captures=max_captures)
            deadline = time.monotonic() + SESSION_SECONDS
            emit({"op": "ready", "run_id": gate.run_id, "episode_dir": str(directory),
                  "limits": {"actions": max_actions, "captures": max_captures,
                             "proposal_idle_seconds": IDLE_SECONDS,
                             "session_seconds": SESSION_SECONDS},
                  "rules": {"action_checkpoint": "latest_verified_single_use"}}, output)
            for count, proposal in enumerate(proposal_lines(input_fd, child, deadline=deadline), 1):
                if count > MAX_PROPOSALS or time.monotonic() >= deadline:
                    raise TimeoutError("proposal count or session budget exceeded")
                receipt = gate.submit(proposal)
                reply = {"op": "receipt", "seq": proposal["seq"], "receipt": receipt}
                if proposal["op"] == "capture":
                    # Gateway has already verified checkpoint index and PNG bytes.
                    reply["checkpoint_path"] = str(directory / receipt["artifact"]["path"])
                    reply["checkpoint_sha256"] = receipt["artifact"]["sha256"]
                if gate.closed:
                    reply["status"] = gate.trace["status"]
                emit(reply, output)
                if gate.closed:
                    return 0 if gate.trace["status"] == "finished" else 1
    except BaseException as error:
        if gate is not None and not gate.closed:
            gate.closed = True
            gate.trace["status"] = "incomplete-eof" if isinstance(error, EOFError) else "failed-before-terminal"
            gate._persist()
        status = "incomplete_eof" if isinstance(error, EOFError) else "error"
        emit({"op": "session_end", "status": status, "run_id": gate.run_id if gate else None,
              "error": f"{type(error).__name__}: {error}"}, output)
        return 1
    raise AssertionError("proposal loop exited without a terminal state")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        "JSONL ready advertises the total session and proposal-idle deadlines. "
        "Every action requires a fresh AUV capture; a verified action receipt consumes its checkpoint."
    ))
    parser.add_argument("--config", type=Path, required=True, help="absolute reviewed episode config")
    parser.add_argument("--episode-dir", type=Path, required=True, help="absolute fresh episode directory")
    parser.add_argument("--action-binary", type=Path, required=True, help="absolute auv-osworld-action binary")
    parser.add_argument("--action-sha256", required=True, help="operator-measured SHA256 of action binary")
    parser.add_argument("--adapter", choices=("v1", "v2-task099", "v2-task044"), default="v1",
                        help="reviewed Kubernetes episode adapter (default: v1)")
    parser.add_argument("--max-actions", type=int, default=32)
    parser.add_argument("--max-captures", type=int, default=32)
    args = parser.parse_args()
    try:
        if not 1 <= args.max_actions <= 32 or not 1 <= args.max_captures <= 32:
            raise ValueError("relay action and capture budgets must each be 1..32")
        config, directory, binary, context = prepare(args.config, args.episode_dir,
                                                      args.action_binary, args.action_sha256, adapter=args.adapter)
        return relay(config, directory, binary, context, max_actions=args.max_actions,
                     max_captures=args.max_captures, input_fd=sys.stdin.fileno(), output=sys.stdout,
                     adapter=args.adapter)
    except BaseException as error:
        emit({"op": "session_end", "status": "error", "error": f"{type(error).__name__}: {error}"}, sys.stdout)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
