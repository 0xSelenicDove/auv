"""Six-phase, paired-AUV Kubernetes infrastructure trial for pinned V2.1 Task099.

The one fixed double-click opens the supplied image; it is not an agent or a
solution policy. Setup and scoring use only the pinned Task099 file bridge.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import k8s_phase_adapter as cluster
import v2_task099_evaluator as task099


TASK_ID = "099"
GUEST_AUV_SOURCE = "1148f382ab441605dc42c722073c8e186147fd24"
GUEST_AUV_SHA256 = "327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e"
V2_BASE_QCOW_SHA256 = "28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a"
V2_HOT_PVC = "osworld-v2-hot"
# NOTICE: This public V2.1 guest uses a different sudo password from V1.
# Remove these commands when the pinned image or AUV build ships the libraries.
APT_UPDATE = ["bash", "-lc", "printf '%s\\n' osworld-public-evaluation | sudo -S -p '' env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 update"]
APT_INSTALL = ["bash", "-lc", "printf '%s\\n' osworld-public-evaluation | sudo -S -p '' env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 install -y --no-install-recommends libtesseract4 liblept5 tesseract-ocr-eng"]
EXTRA_FIELDS = ("task_source", "asset", "host_auv_sha256", "action_binary", "action_binary_sha256", "action_source_commit")
HEX_SHA256 = re.compile(r"[0-9a-f]{64}")
GIT_COMMIT = re.compile(r"[0-9a-f]{40}")
# Frozen from the fresh V2.1 1920x1080 desktop capture: the supplied image
# icon is at bottom right. An image viewer opening is only input receipt.
FIXED_ACTIONS = [{"action_type": "DOUBLE_CLICK", "x": 1850, "y": 880}, "DONE"]


def load_config(path: Path, *, reset: bool = False) -> dict:
    config = json.loads(path.read_text())
    if not isinstance(config, dict) or set(config) != set(cluster.CONFIG_FIELDS) | set(EXTRA_FIELDS):
        raise ValueError("V2 Task099 config requires only the pinned Kubernetes, asset, and AUV fields")
    cluster.validate_common_config(config, require_source_files=not reset)
    if config["base_pvc"] != V2_HOT_PVC or config["base_qcow_sha256"] != V2_BASE_QCOW_SHA256:
        raise ValueError("V2 Task099 base qcow2 differs from the measured hot image")
    for name in ("host_auv_sha256", "action_binary_sha256"):
        if not isinstance(config[name], str) or not HEX_SHA256.fullmatch(config[name]):
            raise ValueError(f"{name} must be a measured SHA256")
    if not isinstance(config["action_source_commit"], str) or not GIT_COMMIT.fullmatch(config["action_source_commit"]):
        raise ValueError("action_source_commit must identify an audited source commit")
    for name in ("task_source", "asset", "action_binary"):
        if not isinstance(config[name], str) or not Path(config[name]).is_absolute() or \
                (not reset and not Path(config[name]).is_file()):
            raise ValueError(f"{name} must be an absolute file path")
    if reset:
        # Reset needs only the sealed config and ownership journal. A missing
        # action binary or gated asset must never prevent UID-safe cleanup.
        return config
    if not os.access(config["action_binary"], os.X_OK):
        raise ValueError("foreground action binary is not executable")
    if cluster.sha256(Path(config["guest_auv_binary"])) != GUEST_AUV_SHA256:
        raise ValueError("guest Ubuntu AUV binary differs from the pinned source build")
    if cluster.sha256(Path(config["host_auv_binary"])) != config["host_auv_sha256"]:
        raise ValueError("paired host AUV binary differs from the measured SHA256")
    if cluster.sha256(Path(config["action_binary"])) != config["action_binary_sha256"]:
        raise ValueError("foreground action binary differs from the measured SHA256")
    task099.load_task(Path(config["upstream_checkout"]), Path(config["task_source"]), Path(config["asset"]))
    return config


def manifest(config_path: Path) -> dict:
    config_path = config_path.resolve(strict=True)
    config = load_config(config_path)
    probe = subprocess.run([sys.executable, "-c", "import requests"], capture_output=True, text=True, check=False)
    if probe.returncode:
        raise RuntimeError(f"phase Python {sys.executable} cannot import requests")
    config_hash = cluster.sha256(config_path)
    identity = {
        "benchmark": "OSWorld-V2.1", "benchmark_revision": task099.UPSTREAM_REV,
        "task_id": TASK_ID, "task_sha256": task099.TASK_SHA256,
        "topology": "paired-remote-fixed-action-infrastructure-trial",
        "runtime_image": cluster.RUNTIME_IMAGE, "qcow2": f"sha256:{V2_BASE_QCOW_SHA256}",
        "auv_source": GUEST_AUV_SOURCE, "auv_binary_sha256": GUEST_AUV_SHA256,
        "auv_target": "paired Device ID acquired at install",
        "runner_identity": "k8s_v2_task099_adapter.py fixed image-opening action",
        "asset_sha256": task099.ASSET_SHA256,
        "host_auv_sha256": config["host_auv_sha256"],
        "action_binary_sha256": config["action_binary_sha256"],
        # NOTICE: This is the operator's build provenance declaration. The
        # binary hash is checked locally; reproducible source proof is a
        # separate live acceptance gate, not inferred from this string.
        "action_source_commit_operator_declared": config["action_source_commit"],
        "episode_config_sha256": config_hash,
    }
    script = Path(__file__).resolve()
    phases = {name: {"argv": [sys.executable, str(script), "phase", name, "--config", str(config_path),
                              "--config-sha256", config_hash],
                     "timeout_seconds": cluster.TIMEOUTS[name]} for name in cluster.PHASES}
    return {"trust": "operator-audited", "batch_id": config["batch_id"],
            "episodes": [{"episode_id": config["episode_id"], "identity": identity, "phases": phases}]}


class Episode(cluster.Episode):
    # TODO(osworld-v2-guest-local): shared-socket control is a separate
    # topology; add it only with a guest-side transport and its own evidence.
    guest_auv_sha256 = GUEST_AUV_SHA256
    apt_update = APT_UPDATE
    apt_install = APT_INSTALL

    def evaluator(self, phase: str) -> None:
        self.assert_identity()
        paired = json.loads((self.directory / "paired-device.json").read_text())
        if paired.get("guest_auv_sha256") != GUEST_AUV_SHA256:
            raise ValueError("installed guest AUV evidence differs from V2 pin")
        command = [sys.executable, str(Path(__file__).with_name("v2_task099_evaluator.py")),
                   "prepare" if phase == "setup" else "evaluate",
                   "--upstream", self.config["upstream_checkout"],
                   "--task-source", self.config["task_source"], "--asset", self.config["asset"],
                   "--episode-dir", str(self.directory),
                   "--endpoint", f"http://127.0.0.1:{self.config['setup_local_port']}"]
        with self.forward(setup=True):
            output = cluster._run(command, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        self.assert_identity()
        result = json.loads(output.splitlines()[-1])
        if result.get("phase") != ("prepare" if phase == "setup" else "evaluate") or \
                result.get("task_sha256") != task099.TASK_SHA256:
            raise ValueError("pinned V2 Task099 bridge returned a mismatched phase or task")
        if phase == "evaluate":
            raw = result.get("result")
            score = raw.get("score") if isinstance(raw, dict) else None
            if isinstance(score, bool) or not isinstance(score, (int, float)) or not 0 <= score <= 1:
                raise ValueError("pinned V2 Task099 bridge returned no bounded raw score")
            # batch_runner's ledger reads top-level score. Preserve the exact
            # upstream result separately; this projection is not an agent rate.
            result["score"] = score
        print(json.dumps(result, sort_keys=True))

    def action(self) -> None:
        self.assert_identity()
        if Path(os.environ["AUV_OSWORLD_ACTION_EVIDENCE"]) != self.directory / "action_evidence.json":
            raise ValueError("action evidence path must be the runner episode sidecar")
        paired = json.loads((self.directory / "paired-device.json").read_text())
        if paired.get("guest_auv_sha256") != GUEST_AUV_SHA256 or not isinstance(paired.get("device_id"), str) or not paired["device_id"]:
            raise ValueError("paired Device or installed guest AUV evidence differs from V2 pin")
        profiles = self.directory / "paired-profiles.json"
        if not profiles.is_file():
            raise ValueError("paired profile evidence is missing")
        for name in ("fixed-action-plan.json", "action_evidence.json", "input-action-results.json", "final-screenshot.png"):
            if (self.directory / name).exists():
                raise FileExistsError(f"refusing stale action evidence: {name}")
        plan = {"version": 1, "context": {"kind": "paired", "device_id": paired["device_id"],
                                           "config_profile": self.config["episode_id"], "profiles_file": str(profiles)},
                "actions": FIXED_ACTIONS, "final_settle_ms": 1000}
        plan_path = self.directory / "fixed-action-plan.json"
        cluster.write_json(plan_path, plan)
        if json.loads(plan_path.read_text()) != plan:
            raise ValueError("persisted fixed action plan differs from audited actions")
        with self.forward(auv=True):
            child = subprocess.run([self.config["action_binary"], "--plan", str(plan_path)], check=False)
        self.assert_identity()
        if child.returncode:
            raise RuntimeError(f"pinned foreground AUV action exited {child.returncode}")
        evidence = json.loads((self.directory / "action_evidence.json").read_text())
        deliveries = json.loads((self.directory / "input-action-results.json").read_text())
        if not isinstance(deliveries, list) or len(deliveries) != 2 or not isinstance(deliveries[0], list) or \
                len(deliveries[0]) != 1 or deliveries[1] != [] or not isinstance(deliveries[0][0], dict) or \
                not deliveries[0][0].get("selected_path") or not any(
                    attempt.get("succeeded") is True for attempt in deliveries[0][0].get("attempts", [])
                    if isinstance(attempt, dict)):
            raise ValueError("fixed image-opening AUV input lacks successful driver delivery")
        artifact = evidence.get("final_artifact")
        png = self.directory / "final-screenshot.png"
        if not isinstance(evidence.get("run_ids"), list) or len(evidence["run_ids"]) != 1 or \
                not isinstance(evidence["run_ids"][0], str) or not evidence["run_ids"][0] or \
                not isinstance(artifact, dict) or artifact.get("path") != png.name or \
                artifact.get("sha256") != cluster.sha256(png) or png.read_bytes()[:8] != b"\x89PNG\r\n\x1a\n":
            raise ValueError("fixed AUV action has no byte-verified final PNG and Run ID")
        # The child printed the exact sidecar as its final stdout line. Do not
        # print another line: batch_runner compares stdout and sidecar.


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("manifest")
    build.add_argument("--config", type=Path, required=True)
    phase = sub.add_parser("phase")
    phase.add_argument("phase", choices=cluster.PHASES)
    phase.add_argument("--config", type=Path, required=True)
    phase.add_argument("--config-sha256", required=True)
    args = parser.parse_args()
    if args.command == "manifest":
        print(json.dumps(manifest(args.config), sort_keys=True, indent=2))
        return
    if cluster.sha256(args.config) != args.config_sha256:
        raise ValueError("episode config changed after manifest generation")
    config = load_config(args.config, reset=args.phase == "reset")
    directory = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"]).resolve(strict=True)
    if directory.name != config["episode_id"]:
        raise ValueError("runner episode directory and config ID differ")
    episode = Episode(config, directory)
    if args.phase in ("setup", "evaluate"):
        episode.evaluator(args.phase)
    else:
        getattr(episode, args.phase)()


if __name__ == "__main__":
    main()
