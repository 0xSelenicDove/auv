"""Content identity helpers used at external artifact trust boundaries."""

from __future__ import annotations

import hashlib
from pathlib import Path


def sha256(path: Path) -> str:
    """Return the SHA-256 digest of a file without loading it all into memory."""
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verified_bytes(path: Path, expected: str) -> bytes:
    """Read a small pinned source or asset and reject content drift."""
    raw = path.read_bytes()
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected:
        raise ValueError(f"{path.name} SHA256 {actual} differs from pinned {expected}")
    return raw
