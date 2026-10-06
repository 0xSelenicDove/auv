"""Evaluator-only OSWorld V1 boundary for audited Chrome and VLC pilot tasks.

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
import re
import shlex
import subprocess
import time
import traceback
import types
from pathlib import Path
from urllib.parse import urlsplit

UPSTREAM_REV = "b138d348256078fa634fc3b73567a7337c793e6b"
VLC_TASK = "5ac2891a-eacd-4954-b339-98abba077adb"
# NOTICE: The pinned V1 Ubuntu guest runs as /home/user (see the OSWorld
# Kubernetes runbook). Revisit this path if the selected guest image changes.
VLC_CONFIG_PATH = "/home/user/.config/vlc/vlcrc"
TASKS = {
    "2ad9387a-65d8-4e33-ad5b-7580065a27ca": (
        "chrome",
        "4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c",
    ),
    VLC_TASK: ("vlc", "4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da"),
    # TODO: Other V1 task chains remain unreviewed. Add each only after its
    # pinned setup/getter/metric path and a live negative control are audited.
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
    unrelated heavy packages. This exploratory task allowlist executes the
    selected pinned method bodies, not the complete upstream module or provider.
    """
    tree = ast.parse(source.read_text())
    body = tree.body
    if class_name:
        body = next(node.body for node in body if isinstance(node, ast.ClassDef) and node.name == class_name)
    selected = [
        node for node in body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in names
    ]
    if {node.name for node in selected} != set(names):
        raise ValueError(f"missing reviewed members in {source}: {set(names) - {node.name for node in selected}}")
    if class_name:
        selected = [ast.ClassDef(name=class_name, bases=[], keywords=[], body=selected, decorator_list=[])]
    module = ast.fix_missing_locations(ast.Module(body=selected, type_ignores=[]))
    exec(compile(module, str(source), "exec"), namespace)
    return namespace[class_name] if class_name else namespace


def _pinned_runtime(upstream: Path, task: dict):
    """Assemble only the audited task evaluator methods from V1 source."""
    # Every selected definition is from the clean checkout verified by load_task.
    # The fixed globals mirror the pinned modules' imports/constants used here.
    import requests

    if task["id"] == VLC_TASK:
        # NOTICE: Upstream _execute_setup accepts HTTP 200 without checking
        # the guest command's returncode. The pinned VLC setup writes the
        # baseline answer through that endpoint; reject false success here.
        # The same guard constrains getter-side /execute to read-only queries.
        source_post = requests.post
        source_get = requests.get

        def audited_get(url, **kwargs):
            if not url.endswith("/terminal"):
                raise ValueError("VLC evaluator attempted an unreviewed guest endpoint")
            try:
                response = source_get(url, **kwargs)
            except Exception:
                raise RuntimeError("VLC guest control request failed") from None
            if response.status_code != 200:
                raise RuntimeError("VLC guest control endpoint is not ready")
            return response

        # ns receives the exact pinned prefix below before either request
        # function can be called by a constructed controller.
        def audited_post(url, **kwargs):
            if url.endswith("/setup/launch"):
                if json.loads(kwargs["data"]) != task["config"][0]["parameters"]:
                    raise ValueError("VLC launch differs from pinned task setup")
            elif url.endswith("/setup/execute"):
                if json.loads(kwargs["data"]) != {**task["config"][1]["parameters"], "shell": False}:
                    raise ValueError("VLC setup command differs from pinned task setup")
            elif url.endswith("/execute"):
                command = json.loads(kwargs["data"])["command"]
                allowed = [
                    ["python", "-c", ns["PYAUTOGUI_PKGS_PREFIX"].format(command=query)]
                    for query in (
                        "import platform; print(platform.system())",
                        "import os; print(os.path.expanduser('~/.config/vlc/vlcrc'))",
                    )
                ]
                if command not in allowed:
                    raise ValueError("VLC evaluator attempted an unreviewed guest command")
            elif url.endswith("/file"):
                if kwargs.get("data") != {"file_path": VLC_CONFIG_PATH}:
                    raise ValueError("unexpected VLC config path")
            else:
                raise ValueError("VLC evaluator attempted an unreviewed guest endpoint")
            try:
                response = source_post(url, **kwargs)
            except Exception:
                raise RuntimeError("VLC guest request failed") from None
            if url.endswith("/setup/launch"):
                expected = f"{task['config'][0]['parameters']['command']} launched successfully"
                if response.status_code != 200 or response.text != expected:
                    raise RuntimeError("VLC launch was not confirmed by the pinned setup endpoint")
            elif url.endswith(("/setup/execute", "/execute")):
                try:
                    result = response.json() if response.status_code == 200 else None
                except Exception:
                    raise RuntimeError("VLC guest command returned invalid JSON") from None
                if (
                    not isinstance(result, dict)
                    or set(result) != {"status", "output", "error", "returncode"}
                    or result.get("returncode") != 0
                    or isinstance(result.get("returncode"), bool)
                    or not isinstance(result.get("output"), str)
                    or not isinstance(result.get("error"), str)
                    or result.get("status") != "success"
                ):
                    raise RuntimeError("VLC guest command did not complete successfully")
                if urlsplit(url).path == "/execute":
                    path_query = command[2].endswith("expanduser('~/.config/vlc/vlcrc'))")
                    expected = VLC_CONFIG_PATH if path_query else "Linux"
                    if result["output"].strip() != expected or result["error"]:
                        raise ValueError("unexpected VLC evaluator query result")
                elif result["output"] or result["error"]:
                    raise RuntimeError("VLC setup command returned unexpected output")
            return response

        requests = types.SimpleNamespace(post=audited_post, get=audited_get, exceptions=requests.exceptions)

    root = upstream / "desktop_env"
    ns = {
        "__builtins__": __builtins__,
        "requests": requests,
        "json": json,
        "logging": logging,
        "os": os,
        "re": re,
        "shlex": shlex,
        "time": time,
        "traceback": traceback,
        "MAX_RETRIES": 20,
        "CHROME_STDERR_LOG": "/tmp/osworld_chrome_stderr.log",
        "logger": logging.getLogger("desktopenv.evaluator-only"),
    }
    # PythonController's prefix is copied from its pinned source assignment.
    controller_source = root / "controllers" / "python.py"
    controller_ast = ast.parse(controller_source.read_text())
    prefix = next(
        node
        for node in controller_ast.body
        if isinstance(node, ast.Assign)
        and any(isinstance(target, ast.Name) and target.id == "PYAUTOGUI_PKGS_PREFIX" for target in node.targets)
    )
    exec(
        compile(ast.fix_missing_locations(ast.Module(body=[prefix], type_ignores=[])), str(controller_source), "exec"),
        ns,
    )
    controller = _pinned_members(
        controller_source,
        "PythonController",
        ("__init__", "get_file", "execute_python_command", "get_vm_platform", "get_vm_machine"),
        ns,
    )
    setup_source = root / "controllers" / "setup.py"
    _pinned_members(setup_source, None, ("_wrap_chrome_launch_for_stderr_capture",), ns)
    setup_methods = ("__init__", "reset_cache_dir", "setup", "_launch_setup", "_sleep_setup")
    if task["id"] == VLC_TASK:
        setup_methods += ("_execute_setup",)
    setup = _pinned_members(setup_source, "SetupController", setup_methods, ns)
    if task["id"] == VLC_TASK:
        _pinned_members(setup_source, None, ("_redact_command_for_log",), ns)
    getters = types.SimpleNamespace()
    metrics = types.SimpleNamespace()
    ns["getters"] = getters
    ns["metrics"] = metrics
    getter_misc = root / "evaluators" / "getters" / "misc.py"
    _pinned_members(getter_misc, None, ("get_rule",), ns)
    getters.get_rule = ns["get_rule"]
    if task["id"] == VLC_TASK:
        getter_vlc = root / "evaluators" / "getters" / "vlc.py"
        _pinned_members(getter_vlc, None, ("get_vlc_config",), ns)
        getters.get_vlc_config = ns["get_vlc_config"]
        metric_vlc = root / "evaluators" / "metrics" / "vlc.py"
        _pinned_members(metric_vlc, None, ("check_play_and_exit",), ns)
        metrics.check_play_and_exit = ns["check_play_and_exit"]
    else:
        getter_chrome = root / "evaluators" / "getters" / "chrome.py"
        _pinned_members(getter_chrome, None, ("_is_arm_architecture", "get_bookmarks"), ns)
        getters.get_bookmarks = ns["get_bookmarks"]
        metric_chrome = root / "evaluators" / "metrics" / "chrome.py"
        _pinned_members(metric_chrome, None, ("is_expected_bookmarks",), ns)
        metrics.is_expected_bookmarks = ns["is_expected_bookmarks"]
    desktop = _pinned_members(
        root / "desktop_env.py",
        "DesktopEnv",
        ("_set_task_info", "_set_evaluator_info", "evaluate", "vm_platform", "vm_machine"),
        ns,
    )
    return desktop, controller, setup


def external_env(task: dict, episode_dir: Path, host: str, port: int, chromium_port: int, upstream: Path):
    """Bind pinned evaluator methods to a Kubernetes-owned guest, never step."""
    task_id = task.get("id")
    if task_id not in TASKS:
        raise ValueError("task is not in the pinned V1 allowlist")
    if task != load_task(upstream, task_id)[0]:
        raise ValueError(f"task differs from the pinned {TASKS[task_id][0].upper()} task")
    desktop, controller, setup = _pinned_runtime(upstream, task)
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
    # NOTICE: Both allowlisted tasks have ordinary metrics, not the infeasible
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
    if env.task_id == VLC_TASK:
        # The pinned metric treats a missing key as the successful default 0.
        # Check the setup's opposite baseline in the fetched guest file so an
        # HTTP 200 without a real write cannot become a false task success.
        try:
            config_path = env.result_getter(env, env.evaluator["result"])
        except Exception:
            raise RuntimeError("VLC setup file verification failed") from None
        contents = Path(config_path).read_text()
        if (
            not any(line.strip() == "play-and-exit=1" for line in contents.splitlines())
            or env.metric(config_path, {"expected_play_and_exit": 1}) != 1
        ):
            raise RuntimeError("VLC setup did not establish play-and-exit=1")
        # NOTICE: /setup/launch confirms process spawn, not that VLC rendered
        # or is ready for GUI input; live acceptance must check that separately.


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
