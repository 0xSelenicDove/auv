"""Pinned OSWorld-V2.1 Task044 file/launch setup and read-side evaluation.

This bridge never starts a VM or delivers GUI input. Between ``prepare`` and
``evaluate``, the agent must observe and edit the Shotcut desktop using AUV.
"""

from __future__ import annotations

import argparse
import ast
from datetime import datetime
import hashlib
from importlib import metadata
import json
import logging
import math
import os
from pathlib import Path
import subprocess
import types
from typing import Any, Dict, List, Optional, Set, Union
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit
from urllib.request import Request, urlopen


UPSTREAM_REV = "3d778a3c9a34a079316f70df023b166700445792"
TASK_SHA256 = "3ff702eb197aff0e4987537c7a98c40f50100f8c416371c9f0d424c6a3a860ad"
GETTER_SHA256 = "4d827d235170ee05a629a483aa972fbc341e994258977816706e36f6c37843ce"
ASSET_SHA256 = "987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108"
ASSET_BYTES = 5_789_382
ASSET_RELATIVE_PATH = "task_044/promo_video.mp4"
SOURCE_VM_PATH = "/home/user/Desktop/promo_video.mp4"
EXPORT_VM_PATH = "/home/user/Desktop/promo_video_v1.mp4"
PROJECT_VM_PATH = "/home/user/Desktop/promo_video.mlt"
GET_PATHS = frozenset((SOURCE_VM_PATH, EXPORT_VM_PATH, PROJECT_VM_PATH))
OPENCV_DIST_VERSION = "4.8.1.78"
OPENCV_MODULE_VERSION = "4.8.1"


def _verify_sha(path: Path, expected: str) -> bytes:
    raw = path.read_bytes()
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected:
        raise ValueError(f"{path.name} SHA256 {actual} differs from pinned {expected}")
    return raw


def _pinned_cv2():
    """Reject a differently packaged decoder before running upstream scoring."""
    try:
        wheel = metadata.version("opencv-python")
        try:
            headless = metadata.version("opencv-python-headless")
        except metadata.PackageNotFoundError:
            headless = None
        import cv2
    except (metadata.PackageNotFoundError, ImportError) as error:
        raise RuntimeError("Task044 requires opencv-python==4.8.1.78") from error
    if wheel != OPENCV_DIST_VERSION or headless is not None or cv2.__version__ != OPENCV_MODULE_VERSION:
        raise RuntimeError(
            f"Task044 OpenCV mismatch: opencv-python={wheel}, headless={headless}, cv2={cv2.__version__}; "
            f"required opencv-python=={OPENCV_DIST_VERSION} only"
        )
    return cv2


def load_task(upstream: Path, task_source: Path, asset: Path):
    """Reject source, asset, and evaluator dependency drift before guest IO."""
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if revision != UPSTREAM_REV:
        raise ValueError(f"OSWorld-V2.1 revision {revision} is not pinned {UPSTREAM_REV}")
    if subprocess.check_output(["git", "-C", str(upstream), "status", "--porcelain"], text=True).strip():
        raise ValueError("OSWorld-V2.1 checkout has uncommitted changes")
    source = _verify_sha(task_source, TASK_SHA256)
    getter_source = upstream / "desktop_env" / "evaluators" / "getters" / "file.py"
    _verify_sha(getter_source, GETTER_SHA256)
    video = _verify_sha(asset, ASSET_SHA256)
    if len(video) != ASSET_BYTES:
        raise ValueError("Task044 asset size differs from pinned release")
    cv2 = _pinned_cv2()

    # Compile the unchanged official getter body without importing unrelated
    # OSWorld getters (some of which can reach browser or command endpoints).
    getter_tree = ast.parse(getter_source.read_text(), filename=str(getter_source))
    getters = [node for node in getter_tree.body if isinstance(node, ast.FunctionDef) and node.name == "get_vm_file"]
    if len(getters) != 1:
        raise ValueError("pinned get_vm_file definition is missing or ambiguous")
    getter_ns = {
        "__builtins__": __builtins__, "os": os, "datetime": datetime,
        "Any": Any, "Dict": Dict, "List": List, "Optional": Optional, "Set": Set, "Union": Union,
        "logger": logging.getLogger("desktopenv.getter.file"),
    }
    exec(compile(ast.fix_missing_locations(ast.Module(body=getters, type_ignores=[])),
                 str(getter_source), "exec"), getter_ns)

    builtin_import = __import__

    def audited_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == "desktop_env.task_base" and tuple(fromlist) == ("BaseTask",):
            return types.SimpleNamespace(BaseTask=object)
        if name == "desktop_env.file_source" and tuple(fromlist) == ("asset",):
            def pinned_asset(relative):
                if relative != ASSET_RELATIVE_PATH:
                    raise ValueError("Task044 requested an unreviewed asset")
                return str(asset)
            return types.SimpleNamespace(asset=pinned_asset)
        if name == "desktop_env.evaluators" and tuple(fromlist) == ("getters",):
            return types.SimpleNamespace(getters=types.SimpleNamespace(get_vm_file=getter_ns["get_vm_file"]))
        if name == "cv2":
            return cv2
        if name.startswith("desktop_env"):
            raise ImportError(f"unreviewed OSWorld module import: {name}")
        return builtin_import(name, globals, locals, fromlist, level)

    builtins = dict(vars(__import__("builtins")), __import__=audited_import)
    task_ns = {"__builtins__": builtins, "__name__": "pinned_task_044"}
    exec(compile(source, str(task_source), "exec"), task_ns)
    task = task_ns["Task044"]()
    if task.id != "044":
        raise ValueError("pinned Task044 ID differs from reviewed task")
    return task, video


class FileTransport:
    """Only Task044's fixed setup and file endpoints; never GUI or execute."""

    def __init__(self, endpoint: str):
        parsed = urlsplit(endpoint)
        if (parsed.scheme != "http" or parsed.hostname not in ("127.0.0.1", "localhost")
                or not parsed.port or parsed.path not in ("", "/") or parsed.query or parsed.fragment
                or parsed.username or parsed.password):
            raise ValueError("endpoint must be a loopback HTTP origin with an explicit port")
        self.endpoint = endpoint.rstrip("/")
        self.transport_error: Exception | None = None
        self.fetched: dict[str, bytes] = {}
        self.missing: set[str] = set()

    def get_file(self, path: str) -> bytes | None:
        if path not in GET_PATHS:
            raise ValueError("Task044 requested an unreviewed guest file")
        request = Request(self.endpoint + "/file", data=urlencode({"file_path": path}).encode(),
                          headers={"Content-Type": "application/x-www-form-urlencoded"}, method="POST")
        try:
            with urlopen(request, timeout=30) as response:
                if response.status != 200:
                    raise RuntimeError(f"guest file endpoint returned {response.status}")
                content = response.read()
                self.fetched[path] = content
                return content
        except HTTPError as error:
            if error.code == 404:
                self.missing.add(path)
                error.close()
                return None
            self.transport_error = RuntimeError(f"guest file endpoint returned HTTP {error.code}")
            error.close()
            raise self.transport_error from error
        except Exception as error:
            self.transport_error = RuntimeError("guest file transport failed")
            raise self.transport_error from error

    def download(self, files: list[dict[str, str]], video: bytes, asset: Path) -> None:
        if files != [{"url": str(asset), "path": SOURCE_VM_PATH}]:
            raise ValueError("Task044 setup requested an unreviewed download")
        boundary = "auv-osworld-task044-pinned-upload"
        body = (
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file_path\"\r\n\r\n{SOURCE_VM_PATH}\r\n"
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file_data\"; filename=\"promo_video.mp4\"\r\n"
            "Content-Type: video/mp4\r\n\r\n"
        ).encode() + video + f"\r\n--{boundary}--\r\n".encode()
        request = Request(self.endpoint + "/setup/upload", data=body,
                          headers={"Content-Type": f"multipart/form-data; boundary={boundary}"}, method="POST")
        try:
            with urlopen(request, timeout=120) as response:
                result = response.read().decode()
                if response.status != 200 or result != f"File Uploaded: {ASSET_BYTES} bytes":
                    raise RuntimeError("Task044 upload was not confirmed by guest")
        except (HTTPError, URLError, TimeoutError, OSError) as error:
            raise RuntimeError("Task044 upload transport failed") from error
        observed = self.get_file(SOURCE_VM_PATH)
        if observed is None or hashlib.sha256(observed).hexdigest() != ASSET_SHA256:
            raise RuntimeError("Task044 guest video postcondition failed")

    def launch(self, command: list[str], shell: bool = False) -> None:
        if command != ["shotcut"] or shell is not False:
            raise ValueError("Task044 setup requested an unreviewed launch")
        request = Request(self.endpoint + "/setup/launch",
                          data=json.dumps({"command": command, "shell": False}).encode(),
                          headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urlopen(request, timeout=30) as response:
                receipt = response.read()
                if response.status != 200 or receipt != b"shotcut launched successfully":
                    raise RuntimeError("Task044 Shotcut launch was not confirmed by guest")
        except (HTTPError, URLError, TimeoutError, OSError) as error:
            raise RuntimeError("Task044 Shotcut launch transport failed") from error


def prepare(task, video: bytes, asset: Path, transport: FileTransport) -> None:
    class Setup:
        def download(self, files):
            transport.download(files, video, asset)

        def launch(self, command, shell=False):
            transport.launch(command, shell)

    task.setup(Setup(), use_proxy=False)


def evaluate(task, transport: FileTransport, cache_dir: Path) -> float:
    env = types.SimpleNamespace(controller=transport, cache_dir=str(cache_dir))
    result = task.evaluate(env)
    # Upstream get_vm_file catches broad exceptions. A transport/cache failure
    # must never masquerade as a legitimate missing output or partial score.
    if transport.transport_error is not None:
        raise RuntimeError("Task044 evaluator transport failed; no score") from transport.transport_error
    if SOURCE_VM_PATH in transport.missing:
        raise RuntimeError("Task044 source video is unavailable; no score")
    for guest_path, content in transport.fetched.items():
        cache_name = {
            EXPORT_VM_PATH: "promo_video_v1.mp4",
            PROJECT_VM_PATH: "promo_video.mlt",
            SOURCE_VM_PATH: "promo_video_source.mp4",
        }[guest_path]
        cached = cache_dir / cache_name
        if not cached.is_file() or cached.read_bytes() != content:
            raise RuntimeError(f"Task044 evaluator could not persist {cache_name}; no score")
    if SOURCE_VM_PATH in transport.fetched and hashlib.sha256(transport.fetched[SOURCE_VM_PATH]).hexdigest() != ASSET_SHA256:
        raise RuntimeError("Task044 source video changed before evaluation; no score")
    if SOURCE_VM_PATH in transport.fetched:
        # The source is a verified fixed asset, not an agent-produced video.
        # If the pinned decoder cannot read it, upstream would silently omit
        # only the resolution point and report a misleading partial score.
        source_resolution = task.evaluate.__globals__["_get_resolution"](str(cache_dir / "promo_video_source.mp4"))
        if source_resolution != (834, 1112):
            raise RuntimeError("Task044 evaluator cannot decode pinned source video; no score")
    if isinstance(result, bool) or not isinstance(result, (float, int)) or not math.isfinite(result) or not 0 <= result <= 1:
        raise TypeError("Task044 evaluator returned an invalid score")
    return float(result)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["prepare", "evaluate"])
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--task-source", type=Path, required=True)
    parser.add_argument("--asset", type=Path, required=True)
    parser.add_argument("--episode-dir", type=Path, required=True)
    parser.add_argument("--endpoint", default="http://127.0.0.1:5000")
    args = parser.parse_args()
    task, video = load_task(args.upstream.resolve(strict=True), args.task_source.resolve(strict=True),
                            args.asset.resolve(strict=True))
    transport = FileTransport(args.endpoint)
    marker = args.episode_dir / "prepared.json"
    identity = {"boundary": "V2.1 Task044 file/launch setup; GUI actions must use AUV",
                "upstream_revision": UPSTREAM_REV, "task_sha256": TASK_SHA256,
                "asset_sha256": ASSET_SHA256, "opencv_python": OPENCV_DIST_VERSION,
                "endpoint": transport.endpoint}
    # TODO(osworld-v2-cohort): The task-local marker does not pin the Pod UID;
    # the six-phase scheduler must do that before the cohort can use this bridge.
    if args.phase == "prepare":
        if marker.exists():
            raise FileExistsError(f"episode already prepared: {marker}")
        prepare(task, video, args.asset.resolve(), transport)
        args.episode_dir.mkdir(parents=True, exist_ok=True)
        marker.write_text(json.dumps(identity, sort_keys=True) + "\n")
        print(json.dumps({**identity, "phase": "prepare", "setup": "video_sha256_and_launch_verified"}, sort_keys=True))
    else:
        if not marker.exists() or json.loads(marker.read_text()) != identity:
            raise ValueError("evaluate requires a matching prepare marker and guest endpoint")
        score = evaluate(task, transport, args.episode_dir / "cache")
        # The unchanged upstream method returns a float. Keep that raw result
        # visible alongside batch_runner's required top-level score projection.
        print(json.dumps({**identity, "phase": "evaluate", "result": score, "score": score}, sort_keys=True))


if __name__ == "__main__":
    main()
