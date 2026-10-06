"""Pinned, scripted Chrome V1 visual baseline over one interactive AUV Run.

The controller may only inspect AUV checkpoint PNGs and send the fixed typed
actions in its SHA-pinned policy. It is not an autonomous agent or an OSWorld
GUI relay. The separate evaluator still owns the task score.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import time
from typing import NamedTuple

import k8s_phase_adapter as capture


ACTION_ENTRY_SOURCE = "bf5c6c048ddabdf9592bce06ef641ae0adc4b41a"
# NOTICE: This is the measured local build used by the fixed two-task replay;
# the source commit identifies reviewed code, not a reproducible-build proof.
ACTION_SHA256 = "ee84942d59239f274b2b35d6a56707e621e29fff3680552d29ebaa2082f30bb0"
OCR_VERSION = "tesseract 5.5.2"
OCR_SHA256 = "6855d30ee1e9e97de11a58624973d2c7eb115a050df64fc3ba88b4077e153997"
ENG_SHA256 = "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2"
POLICY_NAME = "chrome-favorites-controller-v1.json"
POLICY_SHA256 = "4e7c4fde21e452b2007ced6f25e2649e40b1c37094bee2b064da6c9ba73a28dc"
GATES = (None, "add-folder-menu", "new-folder-dialog", "favorites-dialog", "favorites-bookmark-bar")
MAX_CAPTURE_ATTEMPTS = 3
CAPTURE_RETRY_SECONDS = 0.4
RESPONSE_TIMEOUT_SECONDS = 20
MAX_RESPONSE_LINE_BYTES = 65536
TOPOLOGY = "paired-remote-scripted-visual-chrome-v1"
# TODO(osworld-vlc-controller): VLC's Advanced Preferences redraw has no
# archived, validated visual gate; add its own audited policy only after a
# fresh AUV checkpoint sequence shows a stable state transition.


class Word(NamedTuple):
    text: str
    x: int
    y: int
    confidence: float


def _normalized(text: str) -> str:
    return re.sub("[^a-z]", "", text.lower())


def _word(words: list[Word], text: str, bounds: tuple[int, int, int, int]) -> bool:
    left, top, right, bottom = bounds
    return any(_normalized(word.text) == text and word.confidence >= 60
               and left <= word.x <= right and top <= word.y <= bottom for word in words)


def matches_gate(gate: str, words: list[Word]) -> bool:
    """Spatial OCR checks derived from archived AUV images, not the evaluator."""
    title = _word(words, "new", (660, 125, 735, 150)) and _word(words, "folder", (710, 125, 800, 150))
    selector = _word(words, "bookmarks", (700, 210, 800, 245))
    if gate == "add-folder-menu":
        return _word(words, "add", (430, 510, 480, 540)) and _word(words, "folder", (470, 510, 550, 540))
    if gate == "new-folder-dialog":
        return title and selector
    if gate == "favorites-dialog":
        return title and selector and _word(words, "favorites", (720, 165, 850, 205))
    if gate == "favorites-bookmark-bar":
        return _word(words, "favorites", (100, 105, 250, 145)) and not title
    raise ValueError(f"unrecognized visual gate: {gate}")


def verify_ocr_pins(binary: Path, eng_data: Path) -> None:
    if not binary.is_absolute() or not binary.is_file() or capture.sha256(binary) != OCR_SHA256:
        raise ValueError("Tesseract executable differs from the pinned build")
    if not eng_data.is_absolute() or eng_data.name != "eng.traineddata" or not eng_data.is_file() or capture.sha256(eng_data) != ENG_SHA256:
        raise ValueError("Tesseract English model differs from the pinned data")
    version = subprocess.run([str(binary), "--version"], capture_output=True, text=True, timeout=5, check=False)
    if version.returncode or version.stdout.splitlines()[0] != OCR_VERSION:
        raise ValueError("Tesseract version differs from the pinned version")


def ocr_words(image: Path, binary: Path, eng_data: Path | None = None) -> list[Word]:
    # The fixed coordinates are meaningful only on the audited V1 display.
    with image.open("rb") as source:
        header = source.read(24)
    if len(header) != 24 or header[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or (int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")) != (1920, 1080):
        raise ValueError("visual policy requires a 1920x1080 AUV PNG")
    environment = dict(os.environ)
    if eng_data is not None:
        environment["TESSDATA_PREFIX"] = str(eng_data.parent)
    result = subprocess.run([str(binary), str(image), "stdout", "-l", "eng", "tsv"],
                            capture_output=True, text=True, timeout=10, env=environment, check=False)
    if result.returncode or not result.stdout.startswith("level\tpage_num\t"):
        raise ValueError(f"Tesseract TSV failed: {result.stderr[-300:]}")
    words = []
    for line in result.stdout.splitlines()[1:]:
        fields = line.split("\t", 11)
        if len(fields) != 12 or fields[0] != "5":
            continue
        try:
            words.append(Word(fields[11], int(fields[6]), int(fields[7]), float(fields[10])))
        except ValueError as error:
            raise ValueError("Tesseract TSV contains an invalid word row") from error
    return words


def policy() -> dict:
    path = Path(__file__).with_name("action_templates") / POLICY_NAME
    if capture.sha256(path) != POLICY_SHA256:
        raise ValueError("Chrome visual policy differs from pinned SHA256")
    value = json.loads(path.read_text())
    if not isinstance(value, dict) or set(value) != {"version", "task_id", "name", "steps"} or value["version"] != 1 or value["task_id"] != capture.CHROME_TASK:
        raise ValueError("Chrome visual policy schema or task differs")
    steps = value["steps"]
    if not isinstance(steps, list) or len(steps) != len(GATES):
        raise ValueError("Chrome visual policy has wrong step count")
    for step, gate in zip(steps, GATES):
        if not isinstance(step, dict) or set(step) != {"action", "gate"} or not isinstance(step["action"], dict) or step["gate"] != gate:
            raise ValueError("Chrome visual policy has an invalid typed step or gate")
    return value


def _batch(path: Path) -> tuple[Path, Path, Path, Path]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict) or set(value) != {"batch_id", "episode", "action_binary", "tesseract_binary", "eng_traineddata"}:
        raise ValueError("Chrome controller batch requires exactly five fixed fields")
    if not isinstance(value["batch_id"], str) or not value["batch_id"]:
        raise ValueError("batch_id is required")
    paths = []
    for field in ("episode", "action_binary", "tesseract_binary", "eng_traineddata"):
        raw = value[field]
        if not isinstance(raw, str) or not Path(raw).is_absolute() or not Path(raw).is_file():
            raise ValueError(f"{field} must be an existing absolute file")
        paths.append(Path(raw).resolve(strict=True))
    config_path, binary, tesseract, eng_data = paths
    config = capture.load_config(config_path)
    if config["batch_id"] != value["batch_id"] or config.get("task_id") != capture.CHROME_TASK:
        raise ValueError("batch and explicit pinned Chrome task must match episode config")
    if capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("interactive action binary differs from pinned host build")
    verify_ocr_pins(tesseract, eng_data)
    policy()
    return config_path, binary, tesseract, eng_data


def manifest(batch_path: Path) -> dict:
    config_path, binary, tesseract, eng_data = _batch(batch_path)
    built = capture.manifest(config_path)
    config = capture.load_config(config_path)
    config_sha = capture.sha256(config_path)
    identity = built["episodes"][0]["identity"]
    identity.update({"topology": TOPOLOGY, "runner_identity": "k8s_task_controller.py scripted visual policy",
                     "action_entry_source": ACTION_ENTRY_SOURCE, "action_binary_sha256": ACTION_SHA256,
                     "controller_policy_sha256": POLICY_SHA256, "ocr_binary_sha256": OCR_SHA256,
                     "ocr_eng_sha256": ENG_SHA256, "episode_config_sha256": config_sha})
    for name in capture.PHASES:
        built["episodes"][0]["phases"][name] = {
            "argv": [sys.executable, str(Path(__file__).resolve()), "phase", name,
                     "--config", str(config_path), "--config-sha256", config_sha,
                     "--action-binary", str(binary), "--tesseract-binary", str(tesseract),
                     "--eng-traineddata", str(eng_data)],
            "timeout_seconds": capture.TIMEOUTS[name],
        }
    if built["episodes"][0]["identity"]["task_id"] != config["task_id"]:
        raise ValueError("capture manifest task identity changed")
    return built


class InteractivePipe:
    """Bounded JSONL reader for the fixed AUV child; no shell or GUI relay."""

    def __init__(self, process: subprocess.Popen):
        self.process = process
        self.pending = bytearray()
        self.total = 0
        self.sequence = 0

    def read(self) -> dict:
        deadline = time.monotonic() + RESPONSE_TIMEOUT_SECONDS
        while b"\n" not in self.pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("interactive AUV response deadline reached")
            ready, _, _ = select.select([self.process.stdout], [], [], remaining)
            if not ready:
                raise TimeoutError("interactive AUV response deadline reached")
            chunk = os.read(self.process.stdout.fileno(), 4096)
            if not chunk:
                raise EOFError("interactive AUV closed stdout before response")
            self.pending.extend(chunk)
            self.total += len(chunk)
            if len(self.pending) > MAX_RESPONSE_LINE_BYTES or self.total > 1024 * 1024:
                raise ValueError("interactive AUV response byte limit reached")
        line, _, rest = self.pending.partition(b"\n")
        self.pending = bytearray(rest)
        response = json.loads(line)
        if not isinstance(response, dict):
            raise ValueError("interactive AUV response is not an object")
        return response

    def request(self, operation: str, action: dict | None = None) -> dict:
        self.sequence += 1
        request = {"seq": self.sequence, "op": operation}
        if action is not None:
            request["action"] = action
        encoded = json.dumps(request, separators=(",", ":")).encode() + b"\n"
        self.process.stdin.write(encoded)
        self.process.stdin.flush()
        response = self.read()
        if operation == "finish":
            if set(response) != {"run_ids", "final_artifact"}:
                raise ValueError("interactive finish response is not the terminal sidecar")
        elif response.get("seq") != self.sequence or response.get("op") != operation:
            raise ValueError("interactive AUV response sequence or operation differs")
        return response


def _artifact(directory: Path, response: dict) -> Path:
    value = response.get("artifact")
    if not isinstance(value, dict) or set(value) != {"path", "sha256"} or not isinstance(value["path"], str) or not re.fullmatch(r"checkpoint-[0-9]{4}\.png", value["path"]) or not re.fullmatch("[0-9a-f]{64}", str(value["sha256"])):
        raise ValueError("interactive capture response has invalid artifact")
    image = (directory / value["path"]).resolve(strict=True)
    if not image.is_relative_to(directory.resolve()) or capture.sha256(image) != value["sha256"]:
        raise ValueError("interactive capture is outside episode or SHA differs")
    index = json.loads((directory / "checkpoints.json").read_text())
    if not isinstance(index, list) or not index or index[-1] != value:
        raise ValueError("interactive checkpoint index differs from response")
    return image


def _save_decisions(directory: Path, value: dict) -> None:
    capture.write_json(directory / "controller_decisions.json", value)


def action(config: dict, directory: Path, binary: Path, tesseract: Path, eng_data: Path) -> None:
    audited = policy()
    if capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("interactive action binary differs from pinned host build")
    verify_ocr_pins(tesseract, eng_data)
    episode = capture.Episode(config, directory)
    episode.assert_identity()
    paired = json.loads((directory / "paired-device.json").read_text())
    device_id = paired.get("device_id") if isinstance(paired, dict) else None
    profiles = directory / "paired-profiles.json"
    if not isinstance(device_id, str) or not device_id or not profiles.is_file():
        raise ValueError("install did not record the observed paired Device ID and profiles")
    context = {"version": 1, "context": {"kind": "paired", "device_id": device_id,
                                         "config_profile": config["episode_id"], "profiles_file": str(profiles)}}
    context_path = directory / "controller-context.json"
    if context_path.exists() or (directory / "controller_decisions.json").exists():
        raise FileExistsError("refusing to reuse controller evidence")
    capture.write_json(context_path, context)
    decisions = {"schema_version": 1, "policy_sha256": POLICY_SHA256, "task_id": capture.CHROME_TASK,
                 "run_id": None, "checks": [], "status": "starting"}
    _save_decisions(directory, decisions)
    with episode.forward(auv=True):
        process = subprocess.Popen([str(binary), "--interactive", "--context", str(context_path)],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None,
                                   cwd=directory, env=os.environ.copy())
        pipe = InteractivePipe(process)
        try:
            ready = pipe.read()
            run_id = ready.get("run_id")
            if ready.get("op") != "ready" or ready.get("version") != 1 or not isinstance(run_id, str) or not run_id:
                raise ValueError("interactive AUV did not report a valid ready Run")
            sidecar_path = directory / "action_evidence.json"
            if json.loads(sidecar_path.read_text()) != {"run_ids": [run_id], "final_artifact": None}:
                raise ValueError("ready Run ID differs from atomic sidecar")
            decisions["run_id"] = run_id
            decisions["status"] = "running"
            _save_decisions(directory, decisions)
            for number, step in enumerate(audited["steps"], 1):
                response = pipe.request("action", step["action"])
                delivery = response.get("delivery")
                if not isinstance(delivery, list) or not delivery or not all(
                    isinstance(item, dict) and any(attempt.get("succeeded") is True for attempt in item.get("attempts", []))
                    for item in delivery):
                    raise ValueError(f"step {number} has no successful typed AUV delivery")
                gate = step["gate"]
                if gate is None:
                    continue
                passed = False
                for attempt in range(1, MAX_CAPTURE_ATTEMPTS + 1):
                    if attempt > 1:
                        time.sleep(CAPTURE_RETRY_SECONDS)
                    frame = pipe.request("capture")
                    image = _artifact(directory, frame)
                    words = ocr_words(image, tesseract, eng_data)
                    matched = matches_gate(gate, words)
                    decisions["checks"].append({"step": number, "gate": gate, "attempt": attempt,
                                                "seq": pipe.sequence, "artifact": frame["artifact"],
                                                "matched": matched,
                                                "ocr_words": [word._asdict() for word in words if word.confidence >= 60
                                                              and _normalized(word.text) in ("add", "folder", "new", "favorites", "bookmarks", "bar", "save")]})
                    _save_decisions(directory, decisions)
                    if matched:
                        passed = True
                        break
                if not passed:
                    raise ValueError(f"step {number} did not reach visual gate {gate}")
            terminal = pipe.request("finish")
            if terminal != json.loads(sidecar_path.read_text()) or terminal.get("run_ids") != [run_id]:
                raise ValueError("interactive final stdout differs from atomic Run sidecar")
            artifact = terminal.get("final_artifact")
            if not isinstance(artifact, dict) or set(artifact) != {"path", "sha256"} or artifact["path"] != "final-screenshot.png" or not re.fullmatch("[0-9a-f]{64}", str(artifact["sha256"])) or capture.sha256((directory / artifact["path"]).resolve(strict=True)) != artifact["sha256"]:
                raise ValueError("interactive final artifact SHA differs")
            process.wait(timeout=5)
            if process.returncode != 0:
                raise RuntimeError(f"interactive action binary exited {process.returncode}")
            decisions["status"] = "finished"
            _save_decisions(directory, decisions)
            episode.assert_identity()
            print(json.dumps(terminal, sort_keys=True), flush=True)
        except BaseException as error:
            decisions["status"] = "failed"
            decisions["failure"] = str(error)
            _save_decisions(directory, decisions)
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)
            raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("manifest")
    build.add_argument("--batch", type=Path, required=True)
    run = sub.add_parser("phase")
    run.add_argument("phase", choices=capture.PHASES)
    run.add_argument("--config", type=Path, required=True)
    run.add_argument("--config-sha256", required=True)
    run.add_argument("--action-binary", type=Path, required=True)
    run.add_argument("--tesseract-binary", type=Path, required=True)
    run.add_argument("--eng-traineddata", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "manifest":
        print(json.dumps(manifest(args.batch), indent=2, sort_keys=True))
        return
    if capture.sha256(args.config) != args.config_sha256:
        raise ValueError("episode config changed after controller manifest generation")
    if args.phase != "reset" and capture.sha256(args.action_binary) != ACTION_SHA256:
        raise ValueError("action binary changed after controller manifest generation")
    config = capture.load_config(args.config)
    if config.get("task_id") != capture.CHROME_TASK:
        raise ValueError("controller only supports the pinned Chrome V1 task")
    directory = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"]).resolve(strict=True)
    if directory.name != config["episode_id"]:
        raise ValueError("runner episode directory and config ID differ")
    episode = capture.Episode(config, directory)
    if args.phase == "action":
        action(config, directory, args.action_binary, args.tesseract_binary, args.eng_traineddata)
    elif args.phase in ("setup", "evaluate"):
        episode.evaluator(args.phase)
    else:
        getattr(episode, args.phase)()


if __name__ == "__main__":
    main()
