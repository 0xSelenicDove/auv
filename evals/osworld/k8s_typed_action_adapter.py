"""Fixed two-episode paired-AUV typed-action infrastructure trial.

This is a separate manifest/action entry. The capture-only adapter owns all
Kubernetes boot, install, setup, evaluator, and UID-safe reset behavior.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import k8s_phase_adapter as capture


ACTION_ENTRY_SOURCE = "349e5c18812337ac9ee7088418eed9ecce53bde4"
ACTION_SHA256 = "48eedb94c99f7296060aa88d55a9da0a41e2bac36b6c02bad307aa5d54bcd9e9"
# NOTICE: These fixed byte hashes identify infrastructure-only typed moves,
# not task-solving scripts. New actions require an operator-audited slice.
TEMPLATES = {
    capture.CHROME_TASK: ("chrome-infrastructure-v1.json", "a3e3d6042eab72c64ce01ce960aafcfeb2f7089883b42306438f21fd34fd5e53"),
    capture.VLC_TASK: ("vlc-infrastructure-v1.json", "ef06daed0d5c5479413d4f5217a4dc95e4dba45c30f228348d7609ca287d0ed0"),
}
TOPOLOGY = "paired-remote-typed-action-infrastructure-trial"


def template(task_id: str) -> dict:
    if task_id not in TEMPLATES:
        raise ValueError("only pinned Chrome and VLC action templates are allowed")
    name, digest = TEMPLATES[task_id]
    path = Path(__file__).with_name("action_templates") / name
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("action template bytes differ from audited SHA256")
    value = json.loads(data)
    if not isinstance(value, dict) or set(value) != {"version", "actions"} or value["version"] != 1 or not isinstance(value["actions"], list):
        raise ValueError("audited action template has an invalid schema")
    return value


def _batch(path: Path) -> tuple[str, list[Path], Path]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict) or set(value) != {"batch_id", "episodes", "action_binary"}:
        raise ValueError("batch requires exactly batch_id, episodes, action_binary; no command fields")
    if not isinstance(value["batch_id"], str) or not value["batch_id"]:
        raise ValueError("batch_id must be nonempty")
    paths = value["episodes"]
    if not isinstance(paths, list) or len(paths) != 2 or any(not isinstance(item, str) for item in paths):
        raise ValueError("exactly two episode config paths are required")
    configs = [Path(item) for item in paths]
    if any(not item.is_absolute() or not item.is_file() for item in configs) or len(set(configs)) != 2:
        raise ValueError("two distinct existing absolute episode config paths are required")
    binary = value["action_binary"]
    if not isinstance(binary, str) or not Path(binary).is_absolute() or not Path(binary).is_file():
        raise ValueError("action binary must be an existing absolute file")
    return value["batch_id"], configs, Path(binary)


def _config_identity(batch_id: str, paths: list[Path]) -> list[dict]:
    configs = [json.loads(path.read_text()) for path in paths]
    if any(not isinstance(item, dict) or item.get("batch_id") != batch_id or item.get("task_id") not in TEMPLATES for item in configs):
        raise ValueError("both episode configs require this batch and explicit pinned task IDs")
    if {item["task_id"] for item in configs} != set(TEMPLATES):
        raise ValueError("exactly one Chrome and one VLC task are required")
    names = [item.get(name) for item in configs for name in ("runtime_pod", "runtime_service", "proxy_pod")]
    if any(not isinstance(name, str) for name in names) or len(set(names)) != len(names):
        raise ValueError("cross-episode Kubernetes resource names must differ")
    ports = [item.get(name) for item in configs for name in ("setup_local_port", "auv_local_port")]
    if any(isinstance(port, bool) or not isinstance(port, int) for port in ports) or len(set(ports)) != len(ports):
        raise ValueError("cross-episode local ports must differ")
    ids = [item.get("episode_id") for item in configs]
    if any(not isinstance(item, str) for item in ids) or len(set(ids)) != 2:
        raise ValueError("episode IDs must differ")
    return configs


def manifest(batch_path: Path) -> dict:
    batch_id, paths, binary = _batch(batch_path)
    configs = _config_identity(batch_id, paths)
    if capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("action binary differs from pinned host build")
    episodes = []
    for path, config in zip(paths, configs):
        task_id = config["task_id"]
        template(task_id)
        # Reuse the existing fail-closed config, dependency, task, and phase
        # validation. Replace only action in this separate manifest.
        built = capture.manifest(path)["episodes"][0]
        if built["episode_id"] != config["episode_id"] or built["identity"]["task_id"] != task_id:
            raise ValueError("capture manifest identity differs from batch config")
        identity = built["identity"]
        config_sha256 = capture.sha256(path)
        identity.update({"topology": TOPOLOGY, "runner_identity": "k8s_typed_action_adapter.py fixed paired action",
                         "action_binary_sha256": ACTION_SHA256, "action_entry_source": ACTION_ENTRY_SOURCE,
                         "action_template_sha256": TEMPLATES[task_id][1], "episode_config_sha256": config_sha256})
        for phase_name in capture.PHASES:
            built["phases"][phase_name] = {
                "argv": [sys.executable, str(Path(__file__).resolve()), "phase", phase_name,
                         "--config", str(path), "--config-sha256", config_sha256,
                         "--task-id", task_id, "--action-binary", str(binary)],
                "timeout_seconds": capture.TIMEOUTS[phase_name],
            }
        episodes.append(built)
    return {"trust": "operator-audited", "batch_id": batch_id, "episodes": episodes}


def action(config: dict, directory: Path, binary: Path) -> None:
    task_id = config.get("task_id")
    audited = template(task_id)
    if not binary.is_absolute() or capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("action binary differs from pinned host build")
    episode = capture.Episode(config, directory)
    episode.assert_identity()
    paired = json.loads((directory / "paired-device.json").read_text())
    device_id = paired.get("device_id") if isinstance(paired, dict) else None
    if not isinstance(device_id, str) or not device_id:
        raise ValueError("install did not record an observed Device ID")
    profiles = directory / "paired-profiles.json"
    if not profiles.is_file():
        raise ValueError("install did not record paired profiles")
    plan = {"version": 1, "context": {"kind": "paired", "device_id": device_id,
                                      "config_profile": config["episode_id"], "profiles_file": str(profiles)},
            "actions": audited["actions"]}
    plan_path = directory / "typed-action-plan.json"
    if plan_path.exists():
        raise FileExistsError("refusing to reuse typed action plan")
    capture.write_json(plan_path, plan)
    persisted = json.loads(plan_path.read_text())
    if persisted.get("actions") != audited["actions"]:
        raise ValueError("persisted typed actions differ from audited template")
    with episode.forward(auv=True):
        # Foreground child remains in the batch runner's process group.
        result = subprocess.run([str(binary), "--plan", str(plan_path)], check=False)
    episode.assert_identity()
    if result.returncode:
        raise RuntimeError(f"pinned action binary exited {result.returncode}")
    # The binary owns atomic sidecar updates and final stdout. The batch runner
    # validates their equality, screenshot bytes, and failure layer.


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("manifest")
    build.add_argument("--batch", type=Path, required=True)
    run = sub.add_parser("phase")
    run.add_argument("phase", choices=capture.PHASES)
    run.add_argument("--config", type=Path, required=True)
    run.add_argument("--config-sha256", required=True)
    run.add_argument("--task-id", choices=tuple(TEMPLATES), required=True)
    run.add_argument("--action-binary", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "manifest":
        print(json.dumps(manifest(args.batch), sort_keys=True, indent=2))
        return
    if capture.sha256(args.config) != args.config_sha256:
        raise ValueError("episode config changed after typed manifest generation")
    # NOTICE: Reset uses only config-pinned, UID-checked Kubernetes ownership.
    # A lost action binary must not prevent cleanup after a timed-out action.
    if args.phase != "reset" and capture.sha256(args.action_binary) != ACTION_SHA256:
        raise ValueError("action binary changed after typed manifest generation")
    config = capture.load_config(args.config)
    if config.get("task_id") != args.task_id:
        raise ValueError("typed action task differs from predeclared manifest")
    directory = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"]).resolve(strict=True)
    if directory.name != config["episode_id"]:
        raise ValueError("runner episode directory and config ID differ")
    episode = capture.Episode(config, directory)
    if args.phase == "action":
        action(config, directory, args.action_binary)
    elif args.phase in ("setup", "evaluate"):
        episode.evaluator(args.phase)
    else:
        getattr(episode, args.phase)()


if __name__ == "__main__":
    main()
