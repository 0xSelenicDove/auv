"""Foreground JSONL exchange for one existing AUV interactive action Run.

The caller owns model decisions and ``AgentActionGateway``. This transport
only starts the reviewed action binary, exchanges one bounded line at a time,
and reaps that same child. It does not provide a GUI or OSWorld setup relay.
"""

from __future__ import annotations

import json
import os
import select
import subprocess
import time
from pathlib import Path

MAX_LINE_BYTES = 64 * 1024
MAX_SESSION_BYTES = 1024 * 1024
RESPONSE_TIMEOUT_SECONDS = 20
REAP_TIMEOUT_SECONDS = 5


class ForegroundActionTransport:
    """A single, non-restarting child in the runner's own process group."""

    def __init__(
        self, binary: Path, context: Path, directory: Path, *, response_timeout: float = RESPONSE_TIMEOUT_SECONDS
    ):
        self.binary = binary.resolve(strict=True)
        self.context = context.resolve(strict=True)
        self.directory = directory.resolve(strict=True)
        if not self.binary.is_file() or not self.context.is_file() or not self.directory.is_dir():
            raise ValueError("action binary, context, and episode directory must exist")
        if response_timeout <= 0:
            raise ValueError("response timeout must be positive")
        self.response_timeout = response_timeout
        self.process: subprocess.Popen | None = None
        self.pending = bytearray()
        self.response_bytes = 0
        self.request_bytes = 0
        self.closed = False
        self.ready: dict | None = None

    def __enter__(self) -> ForegroundActionTransport:
        if self.process is not None or self.closed:
            raise ValueError("interactive AUV child cannot be restarted")
        environment = os.environ.copy()
        environment["AUV_OSWORLD_EPISODE_DIR"] = str(self.directory)
        environment["AUV_OSWORLD_ACTION_EVIDENCE"] = str(self.directory / "action_evidence.json")
        # No start_new_session/preexec_fn: the child remains in the runner's
        # process group, so runner cancellation can reach it as well.
        self.process = subprocess.Popen(
            [str(self.binary), "--interactive", "--context", str(self.context)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=None,
            cwd=self.directory,
            env=environment,
            bufsize=0,
        )
        try:
            # A pipe can report writable with less room than the next chunk.
            # Nonblocking writes keep the exchange deadline authoritative.
            os.set_blocking(self.process.stdin.fileno(), False)
            self.ready = self._read_response()
            if self.ready.get("op") != "ready":
                raise ValueError("interactive AUV did not emit ready first")
            return self
        except BaseException:
            self.close()
            raise

    def __exit__(self, _type, _value, _traceback) -> None:
        self.close()

    def _read_response(self) -> dict:
        assert self.process is not None and self.process.stdout is not None
        deadline = time.monotonic() + self.response_timeout
        while b"\n" not in self.pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("interactive AUV response deadline reached")
            readable, _, _ = select.select([self.process.stdout], [], [], remaining)
            if not readable:
                raise TimeoutError("interactive AUV response deadline reached")
            chunk = os.read(self.process.stdout.fileno(), 4096)
            if not chunk:
                raise EOFError("interactive AUV child exited before response")
            self.pending.extend(chunk)
            self.response_bytes += len(chunk)
            if len(self.pending) > MAX_LINE_BYTES or self.response_bytes > MAX_SESSION_BYTES:
                raise ValueError("interactive AUV response byte limit reached")
        line, _, rest = self.pending.partition(b"\n")
        self.pending = bytearray(rest)
        if len(line) > MAX_LINE_BYTES:
            raise ValueError("interactive AUV response line limit reached")
        response = json.loads(line)
        if not isinstance(response, dict):
            raise ValueError("interactive AUV response is not an object")
        return response

    def _write_request(self, request: dict) -> None:
        assert self.process is not None and self.process.stdin is not None
        encoded = json.dumps(request, separators=(",", ":")).encode() + b"\n"
        if len(encoded) > MAX_LINE_BYTES or self.request_bytes + len(encoded) > MAX_SESSION_BYTES:
            raise ValueError("interactive AUV request byte limit reached")
        deadline = time.monotonic() + self.response_timeout
        sent = 0
        while sent < len(encoded):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("interactive AUV request deadline reached")
            _, writable, _ = select.select([], [self.process.stdin], [], remaining)
            if not writable:
                raise TimeoutError("interactive AUV request deadline reached")
            # A small write after select is bounded even on a narrow pipe.
            try:
                count = os.write(self.process.stdin.fileno(), encoded[sent : sent + 4096])
            except BlockingIOError:
                continue
            if not count:
                raise BrokenPipeError("interactive AUV child closed stdin")
            sent += count
        self.request_bytes += len(encoded)

    def exchange(self, request: dict) -> dict:
        """Send exactly one request and return exactly the next response."""
        if self.process is None or self.closed:
            raise ValueError("interactive AUV child is not running")
        try:
            self._write_request(request)
            response = self._read_response()
            if request.get("op") in {"finish", "abort"}:
                self.process.wait(timeout=REAP_TIMEOUT_SECONDS)
                # Rust intentionally exits 1 after an explicit abort: the Run
                # is canceled, not successful. This only permits the response
                # to reach AgentActionGateway, which must still verify its
                # exact Run ID and durable terminal sidecar before accepting
                # the abort. No other nonzero exit is a terminal receipt.
                expected_exit = 1 if request["op"] == "abort" else 0
                if self.process.returncode != expected_exit:
                    raise RuntimeError(f"interactive AUV child exited {self.process.returncode}")
                self.close()
            return response
        except BaseException:
            self.close()
            raise

    def close(self) -> None:
        """Close the pipe and reap, escalating only this child if needed."""
        if self.closed:
            return
        self.closed = True
        process = self.process
        if process is None:
            return
        if process.stdin is not None:
            process.stdin.close()
        try:
            process.wait(timeout=REAP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                process.wait(timeout=REAP_TIMEOUT_SECONDS)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=REAP_TIMEOUT_SECONDS)
        if process.stdout is not None:
            process.stdout.close()
