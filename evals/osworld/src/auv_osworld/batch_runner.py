"""Run a predeclared OSWorld batch with bounded processes and durable evidence.

This benchmark-local runner trusts an operator-audited manifest. It can stop
its direct action process group, but cannot prove that a command uses AUV for
GUI input, or that a remote Kubernetes guest has stopped changing state.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import signal
import subprocess
import tempfile
import time
from datetime import UTC, datetime
from pathlib import Path

from .integrity import sha256

PHASES = ("boot", "install", "setup", "action", "evaluate", "reset")
ACTION_TERM_GRACE_SECONDS = 8.0
IDENTITY_FIELDS = (
    "benchmark",
    "benchmark_revision",
    "task_id",
    "task_sha256",
    "topology",
    "runtime_image",
    "qcow2",
    "auv_source",
    "auv_binary_sha256",
    "auv_target",
    "runner_identity",
)


class PhaseInterrupted(KeyboardInterrupt):
    def __init__(self, phase: dict):
        super().__init__("phase interrupted")
        self.phase = phase


def _utc() -> str:
    return datetime.now(UTC).isoformat()


def _write_ledger(path: Path, ledger: dict) -> None:
    """Replace and fsync a complete snapshot after each state transition."""
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, prefix=".ledger-", delete=False
    ) as output:
        temporary = Path(output.name)
        try:
            json.dump(ledger, output, indent=2, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    os.replace(temporary, path)
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def _validate_manifest(manifest: dict) -> None:
    if manifest.get("trust") != "operator-audited":
        raise ValueError("manifest must explicitly declare trust=operator-audited")
    if not isinstance(manifest.get("batch_id"), str) or not manifest["batch_id"]:
        raise ValueError("batch_id is required")
    episodes = manifest.get("episodes")
    if not isinstance(episodes, list) or not episodes:
        raise ValueError("episodes must be a nonempty predeclared list")
    seen = set()
    for episode in episodes:
        if not isinstance(episode, dict):
            raise ValueError("each episode must be an object")
        episode_id = episode.get("episode_id")
        if (
            not isinstance(episode_id, str)
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", episode_id)
            or episode_id in seen
        ):
            raise ValueError(f"invalid or duplicate episode_id: {episode_id!r}")
        seen.add(episode_id)
        identity = episode.get("identity")
        if not isinstance(identity, dict) or any(
            not isinstance(identity.get(field), str) or not identity[field].strip() for field in IDENTITY_FIELDS
        ):
            raise ValueError(f"{episode_id}: missing required identity field")
        if not isinstance(episode.get("phases"), dict) or set(episode["phases"]) != set(PHASES):
            raise ValueError(f"{episode_id}: all six phases are required")
        for name, phase in episode["phases"].items():
            if not isinstance(phase, dict):
                raise ValueError(f"{episode_id}/{name}: phase must be an object")
            argv = phase.get("argv")
            budget = phase.get("timeout_seconds")
            if not isinstance(argv, list) or not argv or any(not isinstance(arg, str) or not arg for arg in argv):
                raise ValueError(f"{episode_id}/{name}: argv must be a nonempty string list")
            if (
                isinstance(budget, bool)
                or not isinstance(budget, (int, float))
                or not math.isfinite(budget)
                or budget <= 0
            ):
                raise ValueError(f"{episode_id}/{name}: timeout_seconds must be positive and finite")


def _group_has_live_members(pgid: int) -> bool | None:
    """Return None when process-group quiescence cannot be verified."""
    observed = subprocess.run(["ps", "-eo", "pgid=,stat="], capture_output=True, text=True, check=False)
    if observed.returncode != 0:
        return None
    for row in observed.stdout.splitlines():
        fields = row.split()
        if len(fields) == 2 and fields[0].isdigit() and int(fields[0]) == pgid and not fields[1].startswith("Z"):
            return True
    return False


def _stop_group(process: subprocess.Popen, grace_seconds: float = 0.2) -> tuple[bool, bool]:
    """TERM, wait for the *whole* Unix group, then KILL if still live.

    NOTICE: A descendant that deliberately detaches into another process group
    cannot be contained by this host-side runner. That needs a task-owned
    container/cgroup adapter before remote batch claims are made.

    Return (group_terminated, term_was_sufficient). A successful parent wait
    alone does not prove that its AUV child finished releasing held input.
    """
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + grace_seconds
    while True:
        process.poll()
        members = _group_has_live_members(process.pid)
        if members is False:
            return True, True
        if members is None or time.monotonic() >= deadline:
            break
        time.sleep(min(0.05, max(0, deadline - time.monotonic())))
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=0.2)
    except subprocess.TimeoutExpired:
        return False, False
    return _group_has_live_members(process.pid) is False, False


def _run_phase(name: str, spec: dict, directory: Path, environment: dict[str, str]) -> dict:
    started = _utc()
    start_ns = time.monotonic_ns()
    stdout_path = directory / f"{name}.stdout"
    stderr_path = directory / f"{name}.stderr"
    result = {
        "started_utc": started,
        "started_monotonic_ns": start_ns,
        "timeout_seconds": spec["timeout_seconds"],
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
    }
    interrupted = False
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        try:
            process = subprocess.Popen(
                spec["argv"],
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                cwd=directory,
                env=environment,
                start_new_session=True,
            )
        except OSError as error:
            result.update(status="spawn_failed", error=str(error), exit_code=None, group_terminated=True)
        else:
            try:
                process.wait(timeout=spec["timeout_seconds"])
            except subprocess.TimeoutExpired:
                stopped, graceful = _stop_group(process, ACTION_TERM_GRACE_SECONDS if name == "action" else 0.2)
                result.update(
                    status="timeout" if stopped else "termination_failed",
                    exit_code=process.poll(),
                    group_terminated=stopped,
                    termination_graceful=graceful,
                )
            except KeyboardInterrupt:
                stopped, graceful = _stop_group(process, ACTION_TERM_GRACE_SECONDS if name == "action" else 0.2)
                result.update(
                    status="interrupted",
                    exit_code=process.poll(),
                    group_terminated=stopped,
                    termination_graceful=graceful,
                )
                interrupted = True
            except BaseException:
                _stop_group(process, ACTION_TERM_GRACE_SECONDS if name == "action" else 0.2)
                raise
            else:
                # An action command must not leave same-group children running.
                stopped, graceful = _stop_group(process, ACTION_TERM_GRACE_SECONDS if name == "action" else 0.2)
                result.update(
                    status="ok"
                    if process.returncode == 0 and stopped
                    else "exit_failed"
                    if stopped
                    else "termination_failed",
                    exit_code=process.returncode,
                    group_terminated=stopped,
                    termination_graceful=graceful,
                )
    result["ended_utc"] = _utc()
    result["elapsed_monotonic_ns"] = time.monotonic_ns() - start_ns
    result["stdout_sha256"] = sha256(stdout_path)
    result["stderr_sha256"] = sha256(stderr_path)
    if interrupted:
        raise PhaseInterrupted(result)
    return result


def _last_json_line(path: Path) -> dict:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines:
        raise ValueError(f"{path.name} has no JSON result")
    value = json.loads(lines[-1])
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} result is not a JSON object")
    return value


def _controller_evidence(directory: Path, phase: dict, policy_sha256: str, run_ids: list[str]) -> dict:
    """Bind a scripted controller's decisions to its observed AUV checkpoints."""
    path = directory / "controller_decisions.json"
    if not path.exists():
        if phase["status"] == "ok":
            raise ValueError("successful scripted action has no controller decision trace")
        return {"status": "absent"}
    decisions = json.loads(path.read_text(encoding="utf-8"))
    if (
        not isinstance(decisions, dict)
        or decisions.get("schema_version") != 1
        or decisions.get("policy_sha256") != policy_sha256
    ):
        raise ValueError("controller decision schema or policy SHA256 differs")
    if decisions.get("run_id") is not None and [decisions["run_id"]] != run_ids:
        raise ValueError("controller Run ID differs from AUV sidecar")
    checks = decisions.get("checks")
    if not isinstance(checks, list) or any(not isinstance(check, dict) for check in checks):
        raise ValueError("controller decision checks are invalid")
    for check in checks:
        artifact = check.get("artifact")
        if (
            not isinstance(artifact, dict)
            or not re.fullmatch(r"checkpoint-[0-9]{4}\.png", str(artifact.get("path")))
            or not re.fullmatch(r"[0-9a-f]{64}", str(artifact.get("sha256")))
        ):
            raise ValueError("controller checkpoint has invalid identity")
        image = (directory / artifact["path"]).resolve(strict=True)
        if not image.is_relative_to(directory.resolve()) or sha256(image) != artifact["sha256"]:
            raise ValueError("controller checkpoint SHA256 differs")
    if phase["status"] == "ok" and (decisions.get("status") != "finished" or not checks):
        raise ValueError("successful scripted action has no finished controller trace")
    return {"status": decisions.get("status"), "path": path.name, "sha256": sha256(path), "checks": len(checks)}


def _action_evidence(directory: Path, phase: dict, identity: dict | None = None) -> dict:
    """Compare the producer's terminal stdout with its durable sidecar.

    The producer writes its AUV Run IDs progressively to the sidecar. On a
    timeout stdout may be absent; on a successful exit it must agree exactly.
    """
    path = directory / "action_evidence.json"
    if not path.exists():
        if phase["status"] == "ok":
            raise ValueError("successful action has no durable AUV evidence sidecar")
        if Path(phase["stdout"]).stat().st_size:
            try:
                terminal = _last_json_line(Path(phase["stdout"]))
            except (ValueError, json.JSONDecodeError):
                terminal = None
            if terminal is not None and "run_ids" in terminal:
                raise ValueError("action stdout reports AUV Run IDs but sidecar is absent")
        return {"run_ids": [], "final_artifact": None, "evidence_status": "absent"}
    evidence = json.loads(path.read_text(encoding="utf-8"))
    run_ids = evidence.get("run_ids")
    if not isinstance(run_ids, list) or any(not isinstance(item, str) or not item for item in run_ids):
        raise ValueError("action sidecar has invalid AUV Run IDs")
    if len(set(run_ids)) != len(run_ids):
        raise ValueError("action sidecar repeats an AUV Run ID")
    artifact = evidence.get("final_artifact")
    if artifact is not None:
        if (
            not isinstance(artifact, dict)
            or not isinstance(artifact.get("path"), str)
            or not isinstance(artifact.get("sha256"), str)
        ):
            raise ValueError("action sidecar has invalid final artifact")
        artifact_path = (directory / artifact["path"]).resolve(strict=True)
        if not artifact_path.is_relative_to(directory.resolve()):
            raise ValueError("final artifact must be inside the episode directory")
        if sha256(artifact_path) != artifact["sha256"]:
            raise ValueError("final artifact SHA256 mismatch")
    if phase["status"] == "ok":
        terminal = _last_json_line(Path(phase["stdout"]))
        if terminal.get("run_ids") != run_ids or terminal.get("final_artifact") != artifact:
            raise ValueError("action stdout and sidecar AUV Run IDs/artifact disagree")
        if not run_ids or artifact is None:
            raise ValueError("successful action must record an AUV Run and final screenshot")
    elif Path(phase["stdout"]).stat().st_size:
        try:
            terminal = _last_json_line(Path(phase["stdout"]))
        except (ValueError, json.JSONDecodeError):
            terminal = None  # A killed process can leave a truncated line.
        if terminal is not None and (terminal.get("run_ids") != run_ids or terminal.get("final_artifact") != artifact):
            raise ValueError("action stdout and sidecar AUV Run IDs/artifact disagree")
    result = {
        "run_ids": run_ids,
        "final_artifact": artifact,
        "evidence_status": "verified",
        "sidecar_sha256": sha256(path),
    }
    if identity is not None and "controller_policy_sha256" in identity:
        result["controller"] = _controller_evidence(directory, phase, identity["controller_policy_sha256"], run_ids)
    return result


def _score(phase: dict) -> tuple[dict, float]:
    raw = _last_json_line(Path(phase["stdout"]))
    score = raw.get("score")
    if isinstance(score, bool) or not isinstance(score, (int, float)) or not math.isfinite(score):
        raise ValueError("evaluator returned no finite numeric score")
    return raw, float(score)


def run_batch(manifest: dict, output_dir: Path) -> dict:
    """Run one new batch and return the ledger saved at output_dir/ledger.json.

    The manifest is trusted/operator-audited. Each phase is a foreground host
    command with no shell; evaluator and reset must return one JSON final line.
    TODO: Pin guest Pod UID and verify task-owned reset through a real K8s
    adapter before using this for unattended cluster batches.
    """
    _validate_manifest(manifest)
    output_dir = output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=False)
    path = output_dir / "ledger.json"
    ledger = {
        "schema_version": 1,
        "batch_id": manifest["batch_id"],
        "manifest_sha256": hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest(),
        "trust": "operator-audited; runner does not verify AUV-only GUI delivery",
        "denominator": len(manifest["episodes"]),
        "started_utc": _utc(),
        "episodes": [
            {
                "episode_id": spec["episode_id"],
                "identity": spec["identity"],
                "status": "scheduled",
                "phases": {},
                "failure_layers": [],
                "auv": {"run_ids": [], "final_artifact": None, "evidence_status": "not_run"},
                "cleanup": {"status": "not_run"},
            }
            for spec in manifest["episodes"]
        ],
    }
    _write_ledger(path, ledger)
    for spec, episode in zip(manifest["episodes"], ledger["episodes"]):
        directory = output_dir / spec["episode_id"]
        directory.mkdir()
        episode["status"] = "running"
        episode["started_utc"] = _utc()
        _write_ledger(path, ledger)
        environment = {
            **os.environ,
            "AUV_OSWORLD_EPISODE_DIR": str(directory),
            "AUV_OSWORLD_ACTION_EVIDENCE": str(directory / "action_evidence.json"),
        }
        interrupted = False
        try:
            setup_complete = True
            for name in PHASES[:4]:
                phase = _run_phase(name, spec["phases"][name], directory, environment)
                episode["phases"][name] = phase
                _write_ledger(path, ledger)
                if name == "action":
                    try:
                        episode["auv"] = _action_evidence(directory, phase, spec["identity"])
                    except (OSError, ValueError, json.JSONDecodeError) as error:
                        episode["failure_layers"].append({"layer": "action_evidence", "detail": str(error)})
                    if phase.get("termination_graceful") is False:
                        episode["failure_layers"].append(
                            {"layer": "action_release", "reason": "forced_kill_unverified"}
                        )
                    _write_ledger(path, ledger)
                if phase["status"] != "ok":
                    episode["failure_layers"].append({"layer": name, "reason": phase["status"]})
                    _write_ledger(path, ledger)
                    if name != "action":
                        setup_complete = False
                        break
            # Action timeout is scored against the deadline state, if setup
            # completed and the action process group really stopped.
            action = episode["phases"].get("action")
            if setup_complete and action and action["group_terminated"] and action["status"] != "spawn_failed":
                name = "evaluate"
                phase = _run_phase("evaluate", spec["phases"]["evaluate"], directory, environment)
                episode["phases"]["evaluate"] = phase
                _write_ledger(path, ledger)
                if phase["status"] == "ok":
                    try:
                        raw, score = _score(phase)
                        episode["evaluator_output"] = raw
                        episode["score"] = score
                    except (OSError, ValueError, json.JSONDecodeError) as error:
                        episode["failure_layers"].append(
                            {"layer": "evaluate", "reason": "invalid_output", "detail": str(error)}
                        )
                else:
                    episode["failure_layers"].append({"layer": "evaluate", "reason": phase["status"]})
                _write_ledger(path, ledger)
        except PhaseInterrupted as error:
            interrupted = True
            episode["phases"][name] = error.phase
            if name == "action":
                try:
                    episode["auv"] = _action_evidence(directory, error.phase, spec["identity"])
                except (OSError, ValueError, json.JSONDecodeError) as evidence_error:
                    episode["failure_layers"].append({"layer": "action_evidence", "detail": str(evidence_error)})
                if error.phase.get("termination_graceful") is False:
                    episode["failure_layers"].append({"layer": "action_release", "reason": "forced_kill_unverified"})
            episode["failure_layers"].append({"layer": "runner", "reason": "interrupted"})
            _write_ledger(path, ledger)
        except KeyboardInterrupt:
            interrupted = True
            episode["failure_layers"].append({"layer": "runner", "reason": "interrupted"})
            _write_ledger(path, ledger)
        except Exception as error:
            # Phase file I/O and evidence failures must not bypass reset.
            episode["failure_layers"].append({"layer": "runner", "reason": "phase_exception", "detail": str(error)})
            _write_ledger(path, ledger)
        finally:
            try:
                name = "reset"
                cleanup = _run_phase("reset", spec["phases"]["reset"], directory, environment)
                episode["phases"]["reset"] = cleanup
                episode["cleanup"] = {"status": cleanup["status"]}
                if cleanup["status"] == "ok":
                    try:
                        report = _last_json_line(Path(cleanup["stdout"]))
                        if not isinstance(report.get("removed_resources"), list) or not isinstance(
                            report.get("retained_pvcs_verified"), list
                        ):
                            raise ValueError("reset output must list removed_resources and retained_pvcs_verified")
                        episode["cleanup"]["report"] = report
                    except (OSError, ValueError, json.JSONDecodeError) as error:
                        episode["cleanup"].update(status="evidence_failed", detail=str(error))
                if episode["cleanup"]["status"] != "ok":
                    episode["failure_layers"].append({"layer": "reset", "reason": episode["cleanup"]["status"]})
            except PhaseInterrupted as error:
                interrupted = True
                episode["phases"]["reset"] = error.phase
                episode["cleanup"] = {"status": "interrupted"}
                episode["failure_layers"].append({"layer": "reset", "reason": "interrupted"})
            except KeyboardInterrupt:
                interrupted = True
                episode["cleanup"] = {"status": "interrupted"}
                episode["failure_layers"].append({"layer": "reset", "reason": "interrupted"})
            except Exception as error:
                episode["cleanup"] = {"status": "phase_exception", "detail": str(error)}
                episode["failure_layers"].append({"layer": "reset", "reason": "phase_exception", "detail": str(error)})
            episode["ended_utc"] = _utc()
            episode["status"] = "failed" if episode["failure_layers"] else "completed"
            _write_ledger(path, ledger)
        if interrupted:
            ledger["stopped_early"] = True
            break
    ledger["ended_utc"] = _utc()
    _write_ledger(path, ledger)
    return ledger


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    ledger = run_batch(manifest, args.output_dir.resolve())
    print(
        json.dumps(
            {
                "batch_id": ledger["batch_id"],
                "ledger": str(args.output_dir.resolve() / "ledger.json"),
                "denominator": ledger["denominator"],
                "completed": sum(e["status"] == "completed" for e in ledger["episodes"]),
            }
        )
    )
    if any(episode["status"] != "completed" for episode in ledger["episodes"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
