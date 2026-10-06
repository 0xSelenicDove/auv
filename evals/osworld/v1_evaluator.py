"""Evaluator-only OSWorld V1 boundary for one audited Chrome pilot task.

The external Kubernetes guest is already running. This module never calls
DesktopEnv.step(), starts a provider, or delivers GUI input. Run `prepare`,
perform all GUI actions through AUV, then run `evaluate` against the same guest.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import logging
import math
import os
from pathlib import Path
import shlex
import subprocess
import time
import traceback
import types
from urllib.parse import urlsplit


UPSTREAM_REV = "b138d348256078fa634fc3b73567a7337c793e6b"
TASKS = {
    "2ad9387a-65d8-4e33-ad5b-7580065a27ca": (
        "chrome", "4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c"
    ),
    # TODO: VLC is intentionally omitted: its setup/getter/metric methods are
    # not in this Chrome-only slice. Add it after a reviewed pinned method chain
    # and a live negative control both pass.
}


def load_task(upstream: Path, task_id: str) -> tuple[dict, str]:
    """Reject unreviewed task configs and modified upstream evaluator code."""
    app, expected_hash = TASKS[task_id]
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if revision != UPSTREAM_REV:
        raise ValueError(f"OSWorld V1 revision {revision} is not pinned {UPSTREAM_REV}")
    if subprocess.check_output(["git", "-C", str(upstream), "status", "--porcelain"], text=True).strip():
        raise ValueError("OSWorld V1 checkout has uncommitted changes")
    task_path = upstream / "evaluation_examples" / "examples" / app / f"{task_id}.json"
    raw = task_path.read_bytes()
    actual_hash = hashlib.sha256(raw).hexdigest()
    if actual_hash != expected_hash:
        raise ValueError(f"task JSON SHA256 {actual_hash} is not pinned {expected_hash}")
    task = json.loads(raw)
    if task.get("id") != task_id:
        raise ValueError("task JSON ID differs from the selected ID")
    return task, actual_hash


def _pinned_members(source: Path, class_name: str | None, names: tuple[str, ...], namespace: dict):
    """Compile only reviewed original V1 definitions, avoiding unrelated imports.

    NOTICE: V1's module imports every provider and evaluator dependency, including
    unrelated heavy packages. This exploratory Chrome-only boundary executes the
    selected pinned method bodies, not the complete upstream module or provider.
    """
    tree = ast.parse(source.read_text())
    body = tree.body
    if class_name:
        body = next(node.body for node in body if isinstance(node, ast.ClassDef) and node.name == class_name)
    selected = [node for node in body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in names]
    if {node.name for node in selected} != set(names):
        raise ValueError(f"missing reviewed members in {source}: {set(names) - {node.name for node in selected}}")
    if class_name:
        selected = [ast.ClassDef(name=class_name, bases=[], keywords=[], body=selected, decorator_list=[])]
    module = ast.fix_missing_locations(ast.Module(body=selected, type_ignores=[]))
    exec(compile(module, str(source), "exec"), namespace)
    return namespace[class_name] if class_name else namespace


def _pinned_runtime(upstream: Path):
    """Assemble only the audited Chrome evaluator methods from V1 source."""
    # Every selected definition is from the clean checkout verified by load_task.
    # The fixed globals mirror the pinned modules' imports/constants used here.
    import requests

    root = upstream / "desktop_env"
    ns = {
        "__builtins__": __builtins__, "requests": requests, "json": json,
        "logging": logging, "os": os, "shlex": shlex, "time": time,
        "traceback": traceback, "MAX_RETRIES": 20,
        "CHROME_STDERR_LOG": "/tmp/osworld_chrome_stderr.log",
        "logger": logging.getLogger("desktopenv.evaluator-only"),
    }
    # PythonController's prefix is copied from its pinned source assignment.
    controller_source = root / "controllers" / "python.py"
    controller_ast = ast.parse(controller_source.read_text())
    prefix = next(node for node in controller_ast.body if isinstance(node, ast.Assign)
                  and any(isinstance(target, ast.Name) and target.id == "PYAUTOGUI_PKGS_PREFIX" for target in node.targets))
    exec(compile(ast.fix_missing_locations(ast.Module(body=[prefix], type_ignores=[])), str(controller_source), "exec"), ns)
    controller = _pinned_members(controller_source, "PythonController", (
        "__init__", "get_file", "execute_python_command", "get_vm_platform", "get_vm_machine"
    ), ns)
    setup_source = root / "controllers" / "setup.py"
    _pinned_members(setup_source, None, ("_wrap_chrome_launch_for_stderr_capture",), ns)
    setup = _pinned_members(setup_source, "SetupController", (
        "__init__", "reset_cache_dir", "setup", "_launch_setup", "_sleep_setup"
    ), ns)
    getters = types.SimpleNamespace()
    metrics = types.SimpleNamespace()
    ns["getters"] = getters
    ns["metrics"] = metrics
    getter_chrome = root / "evaluators" / "getters" / "chrome.py"
    _pinned_members(getter_chrome, None, ("_is_arm_architecture", "get_bookmarks"), ns)
    getters.get_bookmarks = ns["get_bookmarks"]
    getter_misc = root / "evaluators" / "getters" / "misc.py"
    _pinned_members(getter_misc, None, ("get_rule",), ns)
    getters.get_rule = ns["get_rule"]
    metric_chrome = root / "evaluators" / "metrics" / "chrome.py"
    _pinned_members(metric_chrome, None, ("is_expected_bookmarks",), ns)
    metrics.is_expected_bookmarks = ns["is_expected_bookmarks"]
    desktop = _pinned_members(root / "desktop_env.py", "DesktopEnv", (
        "_set_task_info", "_set_evaluator_info", "evaluate", "vm_platform", "vm_machine"
    ), ns)
    return desktop, controller, setup


def external_env(task: dict, episode_dir: Path, host: str, port: int, chromium_port: int, upstream: Path):
    """Bind pinned evaluator methods to a Kubernetes-owned guest, never step."""
    desktop, controller, setup = _pinned_runtime(upstream)
    env = desktop.__new__(desktop)
    env.vm_ip = host
    env.server_port = port
    env.chromium_port = chromium_port
    env.screen_width = 1920
    env.screen_height = 1080
    env.client_password = "password"
    env.enable_proxy = False
    env.cache_dir_base = str(episode_dir / "cache")
    env.controller = controller(vm_ip=host, server_port=port)
    env.setup_controller = setup(
        vm_ip=host,
        server_port=port,
        chromium_port=chromium_port,
        cache_dir=env.cache_dir_base,
        client_password=env.client_password,
        screen_width=env.screen_width,
        screen_height=env.screen_height,
    )
    # NOTICE: The allowlisted task has an ordinary metric, not the infeasible
    # FAIL-control protocol. Reopen action-history bridging only when that
    # task class is approved and AUV control signals are recorded durably.
    env.action_history = []
    env.is_environment_used = False
    env._set_task_info(task)
    env.setup_controller.reset_cache_dir(env.cache_dir)
    return env


def prepare(env) -> None:
    if not env.setup_controller.setup(env.config, False):
        raise RuntimeError("upstream SetupController.setup returned false")


def evaluate(env) -> float:
    """Call the exact pinned DesktopEnv.evaluate, including postconfig/getters."""
    score = env.evaluate()
    if not isinstance(score, (int, float)) or not math.isfinite(score):
        raise TypeError(f"upstream evaluator returned non-numeric score: {score!r}")
    return float(score)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["prepare", "evaluate"])
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--task-id", choices=sorted(TASKS), required=True)
    parser.add_argument("--episode-dir", type=Path, required=True)
    parser.add_argument("--endpoint", default="http://127.0.0.1:5000")
    parser.add_argument("--chromium-port", type=int, default=9222)
    args = parser.parse_args()

    endpoint = urlsplit(args.endpoint)
    if (
        endpoint.scheme != "http"
        or not endpoint.hostname
        or not endpoint.port
        or endpoint.path not in ("", "/")
        or endpoint.query
        or endpoint.fragment
        or endpoint.username
        or endpoint.password
    ):
        parser.error("--endpoint must be an HTTP origin with an explicit port")
    upstream = args.upstream.resolve(strict=True)
    episode_dir = args.episode_dir.resolve()
    task, task_hash = load_task(upstream, args.task_id)
    marker = episode_dir / "prepared.json"
    # TODO: An endpoint and local marker cannot prove that a reconnected port
    # forward still targets the same guest. The external operator must pin and
    # verify the Pod UID for both phases until a durable batch scheduler owns
    # guest identity and reset semantics.
    identity = {
        "boundary": "evaluator-only; GUI actions must use AUV",
        "upstream_revision": UPSTREAM_REV,
        "task_id": args.task_id,
        "task_sha256": task_hash,
        "endpoint": args.endpoint,
        "chromium_port": args.chromium_port,
    }
    if args.phase == "prepare":
        if marker.exists():
            raise FileExistsError(f"episode already prepared: {marker}")
    else:
        if not marker.exists() or json.loads(marker.read_text()) != identity:
            raise ValueError("evaluate requires a matching prepare marker and the same guest endpoint")

    env = external_env(task, episode_dir, endpoint.hostname, endpoint.port, args.chromium_port, upstream)
    if args.phase == "prepare":
        prepare(env)
        marker.write_text(json.dumps(identity, sort_keys=True) + "\n")
        # NOTICE: Upstream _launch_setup logs some HTTP failures without
        # raising. A true SetupController.setup return means its loop ended,
        # not that Chrome is visibly ready; the operator must verify readiness.
        print(json.dumps({**identity, "phase": "prepare", "setup": "upstream_returned_true"}, sort_keys=True))
    else:
        score = evaluate(env)
        print(json.dumps({**identity, "phase": "evaluate", "score": score}, sort_keys=True))


if __name__ == "__main__":
    main()
