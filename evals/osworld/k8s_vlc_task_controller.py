"""Fixed VLC V1 visual controller; GUI input and observations use paired AUV only.

This is a task-specific scripted policy, not an OSWorld GUI relay or a general
agent. The pinned evaluator remains the only source of benchmark score.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import k8s_phase_adapter as capture
import k8s_task_controller as shared


VLC_TASK = "5ac2891a-eacd-4954-b339-98abba077adb"
ACTION_SHA256 = "ee84942d59239f274b2b35d6a56707e621e29fff3680552d29ebaa2082f30bb0"
# NOTICE: This is the last commit touching the action entry, not a proof that
# the locally measured binary above was reproducibly built from that commit.
ACTION_ENTRY_SOURCE = "f7c63a754755238f47f72e4c02ba5a545654cd04"
FFMPEG_SHA256 = "4d390e8dc10fbe2a5fbf015dbdcbe2eebbd6e5ffc3ed1240ce2ede3566d3e207"
POLICY_NAME = "vlc-visual-controller-v1.json"
POLICY_SHA256 = "94c64900027c6ac4488178d578a824eb24e864dbce913662e76a1bfc80cc09cb"
TOPOLOGY = "paired-remote-scripted-visual-vlc-v1"


def policy() -> dict:
    path = Path(__file__).with_name("action_templates") / POLICY_NAME
    if capture.sha256(path) != POLICY_SHA256:
        raise ValueError("VLC visual policy differs from pinned SHA256")
    value = json.loads(path.read_text())
    names = ("open_preferences", "show_all", "close_preferences", "focus_search", "search",
             "select_playlist", "save")
    if (not isinstance(value, dict) or set(value) != {"version", "task_id", "name", "actions",
                                                     "max_preference_opens", "max_ambiguous_recaptures",
                                                     "max_captures", "max_interactive_seconds"}
        or value["version"] != 1 or value["task_id"] != VLC_TASK
        or not isinstance(value["actions"], dict) or tuple(value["actions"]) != names
        or any(not isinstance(value["actions"][name], dict) for name in names)
        or (value["max_preference_opens"], value["max_ambiguous_recaptures"],
            value["max_captures"], value["max_interactive_seconds"]) != (3, 3, 32, 570)):
        raise ValueError("VLC visual policy schema or task differs")
    return value


def verify_ffmpeg(binary: Path) -> None:
    # NOTICE: This exact executable is pinned because its crop output is used
    # for a checkbox state decision; changing raster conversion reopens the gate.
    if not binary.is_absolute() or not binary.is_file() or capture.sha256(binary) != FFMPEG_SHA256:
        raise ValueError("ffmpeg executable differs from the pinned build")


def _batch(path: Path) -> tuple[Path, Path, Path, Path, Path]:
    value = json.loads(path.read_text())
    if (not isinstance(value, dict)
        or set(value) != {"batch_id", "episode", "action_binary", "tesseract_binary",
                          "eng_traineddata", "ffmpeg_binary"}
        or not isinstance(value["batch_id"], str) or not value["batch_id"]):
        raise ValueError("VLC controller batch requires six fixed fields")
    paths = []
    for field in ("episode", "action_binary", "tesseract_binary", "eng_traineddata", "ffmpeg_binary"):
        raw = value[field]
        if not isinstance(raw, str) or not Path(raw).is_absolute() or not Path(raw).is_file():
            raise ValueError(f"{field} must be an existing absolute file")
        paths.append(Path(raw).resolve(strict=True))
    config_path, binary, tesseract, eng_data, ffmpeg = paths
    config = capture.load_config(config_path)
    if config["batch_id"] != value["batch_id"] or config.get("task_id") != VLC_TASK:
        raise ValueError("batch and explicit pinned VLC task must match episode config")
    if capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("interactive action binary differs from pinned host build")
    shared.verify_ocr_pins(tesseract, eng_data)
    verify_ffmpeg(ffmpeg)
    policy()
    return config_path, binary, tesseract, eng_data, ffmpeg


def manifest(batch_path: Path) -> dict:
    config_path, binary, tesseract, eng_data, ffmpeg = _batch(batch_path)
    built = capture.manifest(config_path)
    if built["episodes"][0]["identity"]["task_id"] != VLC_TASK:
        raise ValueError("capture manifest task identity changed")
    config_sha = capture.sha256(config_path)
    identity = built["episodes"][0]["identity"]
    identity.update({"topology": TOPOLOGY, "runner_identity": "k8s_vlc_task_controller.py scripted visual policy",
                     "action_entry_source": ACTION_ENTRY_SOURCE, "action_binary_sha256": ACTION_SHA256,
                     "controller_policy_sha256": POLICY_SHA256, "ocr_binary_sha256": shared.OCR_SHA256,
                     "ocr_eng_sha256": shared.ENG_SHA256, "ffmpeg_binary_sha256": FFMPEG_SHA256,
                     "episode_config_sha256": config_sha})
    for phase in capture.PHASES:
        built["episodes"][0]["phases"][phase] = {
            "argv": [sys.executable, str(Path(__file__).resolve()), "phase", phase,
                     "--config", str(config_path), "--config-sha256", config_sha,
                     "--action-binary", str(binary), "--tesseract-binary", str(tesseract),
                     "--eng-traineddata", str(eng_data), "--ffmpeg-binary", str(ffmpeg)],
            "timeout_seconds": capture.TIMEOUTS[phase],
        }
    return built


def classify(words: list[shared.Word]) -> tuple[str, dict[str, bool]]:
    """Require title AND body; a partially redrawn Advanced window is not green."""
    word = shared._word
    advanced = word(words, "advanced", (850, 190, 950, 220)) and word(words, "preferences", (930, 190, 1050, 220))
    simple = word(words, "simple", (850, 190, 950, 220)) and word(words, "preferences", (930, 190, 1050, 220))
    old = word(words, "interface", (550, 310, 660, 345)) and word(words, "settings", (640, 310, 760, 345))
    new = word(words, "advanced", (880, 230, 1000, 270)) and word(words, "settings", (980, 230, 1090, 270))
    target = (word(words, "playlist", (880, 230, 980, 270))
              and word(words, "play", (910, 380, 950, 410))
              and word(words, "and", (940, 380, 975, 410))
              and word(words, "exit", (965, 380, 1010, 410)))
    main = (word(words, "vlc", (870, 330, 920, 360))
            and word(words, "media", (900, 330, 965, 360))
            and word(words, "player", (960, 330, 1030, 360)))
    signals = {"advanced_title": advanced, "simple_title": simple, "old_pane": old,
               "new_pane": new, "target_pane": target, "main_title": main}
    if advanced and old and not (new or target):
        return "RED_STALE", signals
    if advanced and new and not old:
        return "GREEN_ADVANCED", signals
    if advanced and target and not old:
        return "GREEN_TARGET", signals
    if simple and old and not advanced:
        return "SIMPLE", signals
    if main and not (advanced or simple or old or new or target):
        return "MAIN", signals
    return "AMBIGUOUS", signals


def _crop(image: Path, ffmpeg: Path, filter_graph: str, codec: str) -> bytes:
    result = subprocess.run([str(ffmpeg), "-loglevel", "error", "-i", str(image), "-vf", filter_graph,
                             "-f", "image2pipe" if codec == "png" else "rawvideo",
                             "-vcodec", "png" if codec == "png" else "rawvideo",
                             *([] if codec == "png" else ["-pix_fmt", "rgb24"]), "pipe:1"],
                            capture_output=True, timeout=10, check=False)
    if result.returncode:
        raise ValueError("pinned ffmpeg crop failed")
    return result.stdout


def query_visible(image: Path, ffmpeg: Path, tesseract: Path, eng_data: Path) -> bool:
    # Full-frame TSV missed the tiny search field in archived checkpoint-0004.
    crop = _crop(image, ffmpeg, "crop=320:35:560:235,scale=1280:140", "png")
    result = subprocess.run([str(tesseract), "stdin", "stdout", "-l", "eng", "--psm", "7"],
                            input=crop, capture_output=True, timeout=10, check=False,
                            env={**os.environ, "TESSDATA_PREFIX": str(eng_data.parent)})
    if result.returncode:
        raise ValueError("pinned OCR search crop failed")
    return "play and exit" in result.stdout.decode(errors="replace").lower()


def checkbox_counts(image: Path, ffmpeg: Path) -> dict[str, int]:
    # NOTICE: These 11x11 interior ROIs are from the 1920x1080 1006e AUV frames.
    # The checked and unchecked same-frame controls reject crop/threshold drift.
    counts = {}
    for name, y in (("target", 390), ("checked_control", 498), ("unchecked_control", 308)):
        pixels = _crop(image, ffmpeg, f"crop=11:11:901:{y}", "rgb")
        if len(pixels) != 11 * 11 * 3:
            raise ValueError("checkbox crop has unexpected pixel count")
        counts[name] = sum(max(pixels[i:i + 3]) < 100 for i in range(0, len(pixels), 3))
    return counts


def checkbox_state(counts: dict[str, int]) -> str:
    if counts.get("checked_control", 0) < 15 or counts.get("unchecked_control") != 0:
        raise ValueError("same-frame checkbox controls do not match audited pixels")
    if counts.get("target") == 0:
        return "unchecked"
    if counts.get("target", 0) >= 15:
        return "checked"
    raise ValueError("target checkbox pixels are ambiguous")


def action(config: dict, directory: Path, binary: Path, tesseract: Path, eng_data: Path, ffmpeg: Path) -> None:
    audited = policy()
    if capture.sha256(binary) != ACTION_SHA256:
        raise ValueError("interactive action binary differs from pinned host build")
    shared.verify_ocr_pins(tesseract, eng_data)
    verify_ffmpeg(ffmpeg)
    episode = capture.Episode(config, directory)
    episode.assert_identity()
    paired = json.loads((directory / "paired-device.json").read_text())
    profiles = directory / "paired-profiles.json"
    if not isinstance(paired, dict) or not isinstance(paired.get("device_id"), str) or not paired["device_id"] or not profiles.is_file():
        raise ValueError("install did not record paired Device ID and profiles")
    context = {"version": 1, "context": {"kind": "paired", "device_id": paired["device_id"],
                                         "config_profile": config["episode_id"], "profiles_file": str(profiles)}}
    context_path = directory / "controller-context.json"
    decisions_path = directory / "controller_decisions.json"
    if context_path.exists() or decisions_path.exists():
        raise FileExistsError("refusing to reuse controller evidence")
    capture.write_json(context_path, context)
    decisions = {"schema_version": 1, "policy_sha256": POLICY_SHA256, "task_id": VLC_TASK,
                 "run_id": None, "checks": [], "status": "starting", "target_states": []}
    capture.write_json(decisions_path, decisions)
    actions = audited["actions"]
    with episode.forward(auv=True):
        process = subprocess.Popen([str(binary), "--interactive", "--context", str(context_path)],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None,
                                   cwd=directory, env=os.environ.copy())
        pipe = shared.InteractivePipe(process)
        start = time.monotonic()

        def check_budget() -> None:
            if time.monotonic() - start > audited["max_interactive_seconds"]:
                raise TimeoutError("VLC interactive policy time bound reached")

        def send(name: str) -> None:
            check_budget()
            response = pipe.request("action", actions[name])
            delivery = response.get("delivery")
            if not isinstance(delivery, list) or not delivery or not all(
                isinstance(item, dict) and item.get("attempts")
                and item["attempts"][-1].get("succeeded") is True for item in delivery
            ):
                raise ValueError(f"{name} has no successful typed AUV delivery")

        def observe(stage: str) -> tuple[str, Path, list[shared.Word]]:
            check_budget()
            if len(decisions["checks"]) >= audited["max_captures"]:
                raise ValueError("VLC capture bound reached")
            frame = pipe.request("capture")
            image = shared._artifact(directory, frame)
            words = shared.ocr_words(image, tesseract, eng_data)
            verdict, signals = classify(words)
            decisions["checks"].append({"stage": stage, "seq": pipe.sequence,
                                        "artifact": frame["artifact"], "verdict": verdict, "signals": signals})
            capture.write_json(decisions_path, decisions)
            return verdict, image, words

        try:
            ready = pipe.read()
            run_id = ready.get("run_id")
            sidecar = directory / "action_evidence.json"
            if (ready.get("op") != "ready" or ready.get("version") != 1
                or not isinstance(run_id, str) or not run_id
                or json.loads(sidecar.read_text()) != {"run_ids": [run_id], "final_artifact": None}):
                raise ValueError("interactive AUV did not report a matching ready Run")
            decisions.update({"run_id": run_id, "status": "running"})
            capture.write_json(decisions_path, decisions)
            if observe("initial")[0] != "MAIN":
                raise ValueError("initial VLC main page is not verified")
            for opening in range(1, audited["max_preference_opens"] + 1):
                send("open_preferences")
                time.sleep(1.5)
                if observe(f"simple-{opening}")[0] != "SIMPLE":
                    raise ValueError("Simple Preferences is not verified")
                send("show_all")
                time.sleep(1.5)
                verdict = observe(f"advanced-{opening}-1")[0]
                for extra in range(1, audited["max_ambiguous_recaptures"] + 1):
                    if verdict != "AMBIGUOUS":
                        break
                    time.sleep(0.6)
                    verdict = observe(f"advanced-{opening}-{extra + 1}")[0]
                if verdict == "GREEN_ADVANCED":
                    break
                if verdict not in ("RED_STALE", "AMBIGUOUS"):
                    raise ValueError(f"unexpected Preferences state: {verdict}")
                signals = decisions["checks"][-1]["signals"]
                if not signals["advanced_title"] or signals["main_title"] or signals["simple_title"]:
                    raise ValueError("refusing to close an unverified Advanced Preferences window")
                send("close_preferences")
                time.sleep(1.0)
                if observe(f"returned-main-{opening}")[0] != "MAIN":
                    raise ValueError("Preferences did not return to VLC main")
            else:
                raise ValueError("no full Advanced right pane in three preference opens")
            send("focus_search")
            send("search")
            time.sleep(1.0)
            verdict, image, words = observe("search")
            if (verdict != "GREEN_ADVANCED" or not query_visible(image, ffmpeg, tesseract, eng_data)
                or not shared._word(words, "playlist", (605, 295, 685, 330))):
                raise ValueError("search query, Playlist result, or Advanced pane is not verified")
            send("select_playlist")
            time.sleep(1.0)
            verdict, image, _ = observe("initial-off")
            if verdict != "GREEN_TARGET":
                raise ValueError("initial-off Playlist target pane is not verified")
            counts = checkbox_counts(image, ffmpeg)
            state = checkbox_state(counts)
            decisions["target_states"].append({"stage": "initial-off", "state": state, "pixels": counts,
                                               "artifact": decisions["checks"][-1]["artifact"]})
            capture.write_json(decisions_path, decisions)
            if state != "unchecked":
                raise ValueError(f"initial-off expected unchecked, observed {state}")
            # NOTICE: The pinned setup writes play-and-exit=1 after launching
            # VLC, while the open dialog can still display unchecked. This
            # policy validates the observed UI state and saves it unchanged;
            # a semantic toggle needs a separately approved benchmark slice.
            send("save")
            time.sleep(1.0)
            if observe("after-save")[0] != "MAIN":
                raise ValueError("Save did not return to VLC main")
            terminal = pipe.request("finish")
            artifact = terminal.get("final_artifact")
            if (terminal != json.loads(sidecar.read_text()) or terminal.get("run_ids") != [run_id]
                or not isinstance(artifact, dict) or set(artifact) != {"path", "sha256"}
                or artifact["path"] != "final-screenshot.png"
                or not re.fullmatch("[0-9a-f]{64}", str(artifact["sha256"]))
                or capture.sha256((directory / artifact["path"]).resolve(strict=True)) != artifact["sha256"]):
                raise ValueError("interactive final Run sidecar or screenshot differs")
            process.wait(timeout=5)
            if process.returncode != 0:
                raise RuntimeError(f"interactive action binary exited {process.returncode}")
            decisions["status"] = "finished"
            capture.write_json(decisions_path, decisions)
            episode.assert_identity()
            print(json.dumps(terminal, sort_keys=True), flush=True)
        except BaseException as error:
            decisions.update({"status": "failed", "failure": str(error)})
            capture.write_json(decisions_path, decisions)
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
    run.add_argument("--ffmpeg-binary", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "manifest":
        print(json.dumps(manifest(args.batch), indent=2, sort_keys=True))
        return
    if capture.sha256(args.config) != args.config_sha256:
        raise ValueError("episode config changed after controller manifest generation")
    if args.phase != "reset" and capture.sha256(args.action_binary) != ACTION_SHA256:
        raise ValueError("action binary changed after controller manifest generation")
    config = capture.load_config(args.config)
    if config.get("task_id") != VLC_TASK:
        raise ValueError("controller only supports the pinned VLC V1 task")
    directory = Path(os.environ["AUV_OSWORLD_EPISODE_DIR"]).resolve(strict=True)
    if directory.name != config["episode_id"]:
        raise ValueError("runner episode directory and config ID differ")
    episode = capture.Episode(config, directory)
    if args.phase == "action":
        action(config, directory, args.action_binary, args.tesseract_binary,
               args.eng_traineddata, args.ffmpeg_binary)
    elif args.phase in ("setup", "evaluate"):
        episode.evaluator(args.phase)
    else:
        getattr(episode, args.phase)()


if __name__ == "__main__":
    main()
