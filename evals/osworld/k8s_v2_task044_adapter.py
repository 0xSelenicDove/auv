"""Six-phase paired-AUV Kubernetes capture-only control for pinned V2.1 Task044.

The action records one AUV display capture. It does not edit Shotcut, export a
video, or claim to measure an agent's ability to solve the task.
"""

from __future__ import annotations

import argparse
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import k8s_phase_adapter as cluster
import v2_task044_evaluator as task044


TASK_ID = "044"
GUEST_AUV_SOURCE = "1148f382ab441605dc42c722073c8e186147fd24"
GUEST_AUV_SHA256 = "327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e"
V2_BASE_QCOW_SHA256 = "28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a"
V2_HOT_PVC = "osworld-v2-hot"
# NOTICE: The public V2.1 guest needs its own sudo password and AUV runtime
# libraries. Remove these commands when the pinned guest or build ships them.
APT_UPDATE = ["bash", "-lc", "printf '%s\\n' osworld-public-evaluation | sudo -S -p '' env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 update"]
APT_INSTALL = ["bash", "-lc", "printf '%s\\n' osworld-public-evaluation | sudo -S -p '' env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 install -y --no-install-recommends libtesseract4 liblept5 tesseract-ocr-eng"]
EXTRA_FIELDS = ("task_source", "asset", "host_auv_sha256")
HEX_SHA256 = re.compile(r"[0-9a-f]{64}")


def load_config(path: Path, *, reset: bool = False) -> dict:
    config = json.loads(path.read_text())
    if not isinstance(config, dict) or set(config) != set(cluster.CONFIG_FIELDS) | set(EXTRA_FIELDS):
        raise ValueError("V2 Task044 config requires only the pinned Kubernetes, asset, and AUV fields")
    cluster.validate_common_config(config, require_source_files=not reset)
    if config["base_pvc"] != V2_HOT_PVC or config["base_qcow_sha256"] != V2_BASE_QCOW_SHA256:
        raise ValueError("V2 Task044 base qcow2 differs from the measured hot image")
    if not isinstance(config["host_auv_sha256"], str) or not HEX_SHA256.fullmatch(config["host_auv_sha256"]):
        raise ValueError("host_auv_sha256 must be a measured SHA256")
    for name in ("task_source", "asset"):
        if not isinstance(config[name], str) or not Path(config[name]).is_absolute() or \
                (not reset and not Path(config[name]).is_file()):
            raise ValueError(f"{name} must be an absolute file path")
    if reset:
        # Missing binaries/assets must not block UID-safe cleanup.
        return config
    if cluster.sha256(Path(config["guest_auv_binary"])) != GUEST_AUV_SHA256:
        raise ValueError("guest Ubuntu AUV binary differs from the pinned source build")
    if cluster.sha256(Path(config["host_auv_binary"])) != config["host_auv_sha256"]:
        raise ValueError("paired host AUV binary differs from the measured SHA256")
    task044.load_task(Path(config["upstream_checkout"]), Path(config["task_source"]), Path(config["asset"]))
    return config


def manifest(config_path: Path) -> dict:
    config_path = config_path.resolve(strict=True)
    config = load_config(config_path)
    probe = subprocess.run([sys.executable, "-c", "import requests"], capture_output=True, text=True, check=False)
    if probe.returncode:
        raise RuntimeError(f"phase Python {sys.executable} cannot import requests")
    config_hash = cluster.sha256(config_path)
    identity = {
        "benchmark": "OSWorld-V2.1", "benchmark_revision": task044.UPSTREAM_REV,
        "task_id": TASK_ID, "task_sha256": task044.TASK_SHA256,
        "topology": "paired-remote-capture-only-negative-control",
        "runtime_image": cluster.RUNTIME_IMAGE, "qcow2": f"sha256:{V2_BASE_QCOW_SHA256}",
        "auv_source": GUEST_AUV_SOURCE, "auv_binary_sha256": GUEST_AUV_SHA256,
        "auv_target": "paired Device ID acquired at install",
        "runner_identity": "k8s_v2_task044_adapter.py AUV capture-only",
        "asset_sha256": task044.ASSET_SHA256,
        "opencv_python": task044.OPENCV_DIST_VERSION,
        "host_auv_sha256": config["host_auv_sha256"],
        "episode_config_sha256": config_hash,
    }
    script = Path(__file__).resolve()
    phases = {name: {"argv": [sys.executable, str(script), "phase", name, "--config", str(config_path),
                              "--config-sha256", config_hash],
                     "timeout_seconds": cluster.TIMEOUTS[name]} for name in cluster.PHASES}
    return {"trust": "operator-audited", "batch_id": config["batch_id"],
            "episodes": [{"episode_id": config["episode_id"], "identity": identity, "phases": phases}]}


class Episode(cluster.Episode):
    # TODO(osworld-v2-guest-local): shared-socket control needs a separate
    # owner-approved transport and evidence; this episode is paired only.
    guest_auv_sha256 = GUEST_AUV_SHA256
    apt_update = APT_UPDATE
    apt_install = APT_INSTALL

    def evaluator(self, phase: str) -> None:
        self.assert_identity()
        paired = json.loads((self.directory / "paired-device.json").read_text())
        if paired.get("guest_auv_sha256") != GUEST_AUV_SHA256:
            raise ValueError("installed guest AUV evidence differs from V2 pin")
        command = [sys.executable, str(Path(__file__).with_name("v2_task044_evaluator.py")),
                   "prepare" if phase == "setup" else "evaluate",
                   "--upstream", self.config["upstream_checkout"],
                   "--task-source", self.config["task_source"], "--asset", self.config["asset"],
                   "--episode-dir", str(self.directory),
                   "--endpoint", f"http://127.0.0.1:{self.config['setup_local_port']}"]
        with self.forward(setup=True):
            output = cluster._run(command, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
        self.assert_identity()
        lines = output.splitlines(keepends=True)
        if not lines:
            raise ValueError("pinned V2 Task044 bridge returned no result")
        result = json.loads(lines[-1])
        if result.get("phase") != ("prepare" if phase == "setup" else "evaluate") or \
                result.get("task_sha256") != task044.TASK_SHA256 or \
                result.get("upstream_revision") != task044.UPSTREAM_REV or \
                result.get("asset_sha256") != task044.ASSET_SHA256 or \
                result.get("opencv_python") != task044.OPENCV_DIST_VERSION:
            raise ValueError("pinned V2 Task044 bridge returned mismatched identity")
        if phase == "evaluate":
            raw = result.get("result")
            score = result.get("score")
            if isinstance(raw, bool) or not isinstance(raw, (int, float)) or \
                    isinstance(score, bool) or not isinstance(score, (int, float)) or \
                    not 0 <= raw <= 1 or score != raw:
                raise ValueError("pinned V2 Task044 bridge returned no bounded raw float score")
            # The original scorer prints diagnostic lines before the bridge's
            # JSON receipt. Preserve them only after validating the receipt;
            # batch_runner still consumes the last JSON line as the score.
            print("".join(lines[:-1]), end="")
        print(json.dumps(result, sort_keys=True))

    def action(self) -> None:
        """Use inherited paired AUV capture only, then verify recorded bytes."""
        self.assert_identity()
        paired = json.loads((self.directory / "paired-device.json").read_text())
        if paired.get("guest_auv_sha256") != GUEST_AUV_SHA256 or \
                not isinstance(paired.get("device_id"), str) or not paired["device_id"]:
            raise ValueError("paired Device or installed guest AUV evidence differs from V2 pin")
        for name in ("action_evidence.json", "final-screenshot.png"):
            if (self.directory / name).exists():
                raise FileExistsError(f"refusing stale action evidence: {name}")
        # The shared capture path prints its sidecar. Hold that line until the
        # Task044-specific byte check passes, so a failed action has no stdout
        # that could be mistaken for a successful runner receipt.
        output = io.StringIO()
        with redirect_stdout(output):
            super().action()
        evidence = json.loads((self.directory / "action_evidence.json").read_text())
        png = self.directory / "final-screenshot.png"
        artifact = evidence.get("final_artifact")
        if not isinstance(evidence.get("run_ids"), list) or len(evidence["run_ids"]) != 1 or \
                not isinstance(evidence["run_ids"][0], str) or not evidence["run_ids"][0] or \
                not isinstance(artifact, dict) or artifact.get("path") != png.name or \
                artifact.get("sha256") != cluster.sha256(png) or png.read_bytes()[:8] != b"\x89PNG\r\n\x1a\n":
            raise ValueError("AUV capture has no byte-verified PNG and Run ID")
        if json.loads(output.getvalue()) != evidence:
            raise ValueError("AUV capture stdout and sidecar differ")
        print(output.getvalue(), end="")


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
