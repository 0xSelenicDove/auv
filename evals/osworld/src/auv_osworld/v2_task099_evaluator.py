"""Pinned OSWorld-V2.1 Task099 setup/evaluation against an external guest.

This bridge does not start a VM or deliver GUI input. The agent must use AUV
between ``prepare`` and ``evaluate``. Only the task's pinned file setup and
read-side evaluator are permitted through the guest control endpoint.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import logging
import math
import os
import subprocess
import types
from datetime import datetime
from pathlib import Path
from typing import Any, Optional, Union
from urllib.parse import urlsplit

import requests

from .integrity import verified_bytes

UPSTREAM_REV = "3d778a3c9a34a079316f70df023b166700445792"
TASK_SHA256 = "58c460fdfecf518f64714fdc21933b60818d8cf28ec02f9fa15a10e56ef02e32"
GETTER_SHA256 = "4d827d235170ee05a629a483aa972fbc341e994258977816706e36f6c37843ce"
ASSET_SHA256 = "6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1"
ASSET_BYTES = 1_851_281
ASSET_RELATIVE_PATH = "task_099/my_image.png"
IMAGE_VM_PATH = "/home/user/Desktop/my_image.png"
POSITION_VM_PATH = "/home/user/Desktop/position.txt"


def load_task(upstream: Path, task_source: Path, asset: Path):
    """Reject source/asset drift before constructing any guest transport."""
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if revision != UPSTREAM_REV:
        raise ValueError(f"OSWorld-V2.1 revision {revision} is not pinned {UPSTREAM_REV}")
    if subprocess.check_output(["git", "-C", str(upstream), "status", "--porcelain"], text=True).strip():
        raise ValueError("OSWorld-V2.1 checkout has uncommitted changes")
    source = verified_bytes(task_source, TASK_SHA256)
    getter_source = upstream / "desktop_env" / "evaluators" / "getters" / "file.py"
    verified_bytes(getter_source, GETTER_SHA256)
    image = verified_bytes(asset, ASSET_SHA256)
    if len(image) != ASSET_BYTES:
        raise ValueError("Task099 asset size differs from pinned release")

    # Compile the upstream getter body and task module unchanged. A restricted
    # import boundary supplies only the selected file getter and local asset.
    getter_tree = ast.parse(getter_source.read_text(), filename=str(getter_source))
    getters = [node for node in getter_tree.body if isinstance(node, ast.FunctionDef) and node.name == "get_vm_file"]
    if len(getters) != 1:
        raise ValueError("pinned get_vm_file definition is missing or ambiguous")
    getter_ns = {
        "__builtins__": __builtins__,
        "os": os,
        "datetime": datetime,
        "Any": Any,
        "Dict": dict,
        "List": list,
        "Optional": Optional,
        "Set": set,
        "Union": Union,
        "logger": logging.getLogger("desktopenv.getter.file"),
    }
    exec(
        compile(ast.fix_missing_locations(ast.Module(body=getters, type_ignores=[])), str(getter_source), "exec"),
        getter_ns,
    )

    builtin_import = __import__

    def audited_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == "desktop_env.task_base" and tuple(fromlist) == ("BaseTask",):
            return types.SimpleNamespace(BaseTask=object)
        if name == "desktop_env.file_source" and tuple(fromlist) == ("asset",):

            def pinned_asset(relative):
                if relative != ASSET_RELATIVE_PATH:
                    raise ValueError("Task099 requested an unreviewed asset")
                return str(asset)

            return types.SimpleNamespace(asset=pinned_asset)
        if name == "desktop_env.evaluators.getters" and tuple(fromlist) == ("get_vm_file",):
            return types.SimpleNamespace(get_vm_file=getter_ns["get_vm_file"])
        if name.startswith("desktop_env"):
            raise ImportError(f"unreviewed OSWorld module import: {name}")
        return builtin_import(name, globals, locals, fromlist, level)

    builtins = dict(vars(__import__("builtins")), __import__=audited_import)
    task_ns = {"__builtins__": builtins, "__name__": "pinned_task_099"}
    exec(compile(source, str(task_source), "exec"), task_ns)
    task = task_ns["Task099"]()
    if task.id != "099" or task_ns["IMAGE_VM_PATH"] != IMAGE_VM_PATH or task_ns["POSITION_VM_PATH"] != POSITION_VM_PATH:
        raise ValueError("pinned Task099 contract differs from the reviewed paths")
    return task, image


class FileTransport:
    """Only the two Task099 file endpoints; never ``/execute`` or GUI input."""

    def __init__(self, endpoint: str):
        parsed = urlsplit(endpoint)
        if (
            parsed.scheme != "http"
            or parsed.hostname not in ("127.0.0.1", "localhost")
            or not parsed.port
            or parsed.path not in ("", "/")
            or parsed.query
            or parsed.fragment
            or parsed.username
            or parsed.password
        ):
            raise ValueError("endpoint must be a loopback HTTP origin with an explicit port")
        self.endpoint = endpoint.rstrip("/")
        self.transport_error: Exception | None = None
        self.last_position_bytes: bytes | None = None

    def get_file(self, path: str) -> bytes | None:
        if path not in (IMAGE_VM_PATH, POSITION_VM_PATH):
            raise ValueError("Task099 requested an unreviewed guest file")
        try:
            response = requests.post(self.endpoint + "/file", data={"file_path": path}, timeout=30)
            if response.status_code == 404:
                return None
            response.raise_for_status()
            if path == POSITION_VM_PATH:
                self.last_position_bytes = response.content
            return response.content
        except Exception as error:
            self.transport_error = RuntimeError("guest file transport failed")
            raise self.transport_error from error

    def download(self, files: list[dict[str, str]], image: bytes, asset: Path):
        if files != [{"url": str(asset), "path": IMAGE_VM_PATH}]:
            raise ValueError("Task099 setup requested an unreviewed download")
        try:
            response = requests.post(
                self.endpoint + "/setup/upload",
                data={"file_path": IMAGE_VM_PATH},
                files={"file_data": ("my_image.png", image, "image/png")},
                timeout=120,
            )
            if response.status_code != 200 or response.text != f"File Uploaded: {ASSET_BYTES} bytes":
                raise RuntimeError("Task099 upload was not confirmed by guest")
        except requests.RequestException as error:
            raise RuntimeError("Task099 upload transport failed") from error
        observed = self.get_file(IMAGE_VM_PATH)
        if observed is None or hashlib.sha256(observed).hexdigest() != ASSET_SHA256:
            raise RuntimeError("Task099 guest image postcondition failed")


def prepare(task, image: bytes, asset: Path, transport: FileTransport) -> None:
    class Setup:
        def download(self, files):
            transport.download(files, image, asset)

    task.setup(Setup(), use_proxy=False)


def evaluate(task, transport: FileTransport, cache_dir: Path) -> dict:
    env = types.SimpleNamespace(controller=transport, cache_dir=str(cache_dir))
    result = task.evaluate(env)
    # ROOT CAUSE: upstream get_vm_file and Task099.evaluate both catch broad
    # exceptions. Without this out-of-band check, a broken file connection
    # would be reported as an ordinary missing answer and scored zero.
    if transport.transport_error is not None:
        raise RuntimeError("Task099 evaluator transport failed; no score") from transport.transport_error
    if transport.last_position_bytes is not None:
        cached = cache_dir / "position.txt"
        if not cached.is_file() or cached.read_bytes() != transport.last_position_bytes:
            raise RuntimeError("Task099 evaluator could not persist the fetched answer; no score")
    score = result.get("score") if isinstance(result, dict) else None
    if not isinstance(score, (float, int)) or isinstance(score, bool) or not math.isfinite(score):
        raise TypeError("Task099 evaluator returned a non-finite score")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["prepare", "evaluate"])
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--task-source", type=Path, required=True)
    parser.add_argument("--asset", type=Path, required=True)
    parser.add_argument("--episode-dir", type=Path, required=True)
    parser.add_argument("--endpoint", default="http://127.0.0.1:5000")
    args = parser.parse_args()
    task, image = load_task(
        args.upstream.resolve(strict=True), args.task_source.resolve(strict=True), args.asset.resolve(strict=True)
    )
    transport = FileTransport(args.endpoint)
    marker = args.episode_dir / "prepared.json"
    identity = {
        "boundary": "V2.1 Task099 file-only; GUI actions must use AUV",
        "upstream_revision": UPSTREAM_REV,
        "task_sha256": TASK_SHA256,
        "asset_sha256": ASSET_SHA256,
        "endpoint": transport.endpoint,
    }
    # TODO: The marker cannot prove that a reconnected port-forward still
    # reaches the same Pod. A later batch scheduler must pin the Pod UID.
    if args.phase == "prepare":
        if marker.exists():
            raise FileExistsError(f"episode already prepared: {marker}")
        prepare(task, image, args.asset.resolve(), transport)
        args.episode_dir.mkdir(parents=True, exist_ok=True)
        marker.write_text(json.dumps(identity, sort_keys=True) + "\n")
        print(json.dumps({**identity, "phase": "prepare", "setup": "image_sha256_verified"}, sort_keys=True))
    else:
        if not marker.exists() or json.loads(marker.read_text()) != identity:
            raise ValueError("evaluate requires a matching prepare marker and guest endpoint")
        result = evaluate(task, transport, args.episode_dir / "cache")
        print(json.dumps({**identity, "phase": "evaluate", "result": result}, sort_keys=True))


if __name__ == "__main__":
    main()
