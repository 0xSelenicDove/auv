#!/usr/bin/env python3
"""Guest-side AUV-only JSONL relay for one OSWorld owner-socket action Run.

This process runs as the unprivileged desktop owner inside the Ubuntu guest.
The host operator supplies proposals over a host-key-verified SSH channel and
copies AUV-produced PNGs over SFTP. No Kubernetes or OSWorld setup API is used.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

from agent_action_gateway import DIGEST
from agent_action_relay import PRIOR_OUTPUTS, absolute_file, emit, run_session, sha256


DEVICE_ID = re.compile(r"[0-9a-f]{64}\Z")


def owner_socket(endpoint: str, uid: int) -> Path:
    """Require an actual, directly owned AF_UNIX pathname, not a URL relay."""
    # NOTICE: UID ownership is the local trust boundary, not cryptographic
    # daemon attestation against another same-UID process. Revisit if AUV
    # exposes an authenticated local-daemon identity for this socket.
    if not endpoint.startswith("unix:///") or endpoint.count("?") or endpoint.count("#"):
        raise ValueError("guest daemon endpoint must be a Unix absolute pathname")
    raw = Path(endpoint[len("unix://"):])
    if not raw.is_absolute() or str(raw) == "/" or ".." in raw.parts:
        raise ValueError("guest daemon socket path must be absolute and normalized")
    if raw.is_symlink() or raw.parent.is_symlink() or raw.resolve(strict=True) != raw:
        raise ValueError("guest daemon socket path must not traverse a symlink")
    info = raw.lstat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != uid:
        raise ValueError("guest daemon endpoint is not an owner Unix socket")
    parent = raw.parent.stat()
    if parent.st_uid != uid:
        raise ValueError("guest daemon socket parent must belong to desktop owner")
    return raw


def online_device(auv_binary: Path, endpoint: str, expected: str) -> None:
    """Verify the canonical ID from the local daemon, not a copied profile."""
    if not DEVICE_ID.fullmatch(expected):
        raise ValueError("expected Device ID must be canonical 64-character lowercase hex")
    env = os.environ.copy()
    # The explicit endpoint is authoritative; profile/endpoint overrides must
    # not silently select a different target during this read-only probe.
    for name in ("AUV_ENDPOINT", "AUV_DEVICE_ID", "AUV_DEVICE", "AUV_CONFIG_PROFILE"):
        env.pop(name, None)
    result = subprocess.run([str(auv_binary), "devices", "list", "--endpoint", endpoint, "--json"],
                            check=True, capture_output=True, text=True, timeout=15, env=env)
    values = json.loads(result.stdout)
    if not isinstance(values, list):
        raise ValueError("AUV Device list is not a JSON array")
    local = [value for value in values if isinstance(value, dict) and value.get("source") == "daemon"
             and value.get("local") is True and value.get("status") == "online"]
    if len(local) != 1 or local[0].get("device_id") != expected:
        raise ValueError("owner socket did not report the expected unique online local Device ID")


def prepare(directory_path: Path, action_path: Path, action_digest: str,
            auv_path: Path, auv_digest: str, endpoint: str, expected_device: str) -> tuple[Path, Path, dict]:
    if sys.platform != "linux" or os.geteuid() == 0:
        raise ValueError("guest-local relay requires an unprivileged Linux owner")
    if not directory_path.is_absolute():
        raise ValueError("episode directory must be absolute")
    directory = directory_path.resolve(strict=True)
    if not directory.is_dir() or directory.stat().st_uid != os.geteuid():
        raise ValueError("episode directory must belong to desktop owner")
    if any((directory / name).exists() or (directory / name).is_symlink() for name in PRIOR_OUTPUTS):
        raise FileExistsError("refusing to reuse an existing action Run or trace")
    if any(directory.glob("checkpoint-*.png")):
        raise FileExistsError("refusing to reuse existing checkpoint artifacts")
    action = absolute_file(action_path, "action binary")
    auv = absolute_file(auv_path, "AUV binary")
    for label, path, digest in (("action", action, action_digest), ("AUV", auv, auv_digest)):
        if not DIGEST.fullmatch(digest) or sha256(path) != digest:
            raise ValueError(f"{label} binary differs from the operator-pinned SHA256")
        if not os.access(path, os.X_OK):
            raise ValueError(f"{label} binary is not executable")
    socket = owner_socket(endpoint, os.geteuid())
    identity = (socket.lstat().st_dev, socket.lstat().st_ino)
    online_device(auv, endpoint, expected_device)
    # Fail closed if the daemon replaced its socket during the identity probe.
    socket = owner_socket(endpoint, os.geteuid())
    if (socket.lstat().st_dev, socket.lstat().st_ino) != identity:
        raise ValueError("guest daemon socket changed during Device probe")
    return directory, action, {"version": 1, "context": {"kind": "guest-local",
            "device_id": expected_device, "daemon_endpoint": endpoint}}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--episode-dir", type=Path, required=True)
    parser.add_argument("--action-binary", type=Path, required=True)
    parser.add_argument("--action-sha256", required=True)
    parser.add_argument("--auv-binary", type=Path, required=True)
    parser.add_argument("--auv-sha256", required=True)
    parser.add_argument("--daemon-endpoint", required=True)
    parser.add_argument("--device-id", required=True)
    parser.add_argument("--max-actions", type=int, default=32)
    parser.add_argument("--max-captures", type=int, default=32)
    args = parser.parse_args()
    try:
        if not 1 <= args.max_actions <= 32 or not 1 <= args.max_captures <= 32:
            raise ValueError("relay action and capture budgets must each be 1..32")
        directory, action, context = prepare(args.episode_dir, args.action_binary, args.action_sha256,
                                             args.auv_binary, args.auv_sha256, args.daemon_endpoint,
                                             args.device_id)
        return run_session(directory, action, context, max_actions=args.max_actions,
                           max_captures=args.max_captures, input_fd=sys.stdin.fileno(), output=sys.stdout)
    except BaseException as error:
        emit({"op": "session_end", "status": "error", "error": f"{type(error).__name__}: {error}"}, sys.stdout)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
