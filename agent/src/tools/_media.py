"""Shared media type classification and sandbox URL helpers."""

from __future__ import annotations

import os
from pathlib import Path

MEDIA_TYPES: dict[str, frozenset[str]] = {
    "image": frozenset({".png", ".jpg", ".jpeg", ".gif", ".webp"}),
    "video": frozenset({".mp4", ".webm", ".mov"}),
    "audio": frozenset({".mp3", ".wav", ".ogg", ".m4a"}),
}

ALL_MEDIA_EXTENSIONS: frozenset[str] = frozenset().union(*MEDIA_TYPES.values())


def classify_media(ext: str) -> str | None:
    """Return 'image', 'video', 'audio', or None for the given extension."""
    ext = ext.lower()
    for media_type, extensions in MEDIA_TYPES.items():
        if ext in extensions:
            return media_type
    return None


def sandbox_url(resolved: Path, workspace: str) -> str:
    """Build a sandbox:/// URL from a resolved path and workspace root."""
    rel = os.path.relpath(str(resolved), workspace)
    return f"sandbox:///{rel}"


def format_sandbox_ref(rel_path: str, media_type: str) -> str:
    """Format a markdown reference for a sandbox media file."""
    name = os.path.basename(rel_path)
    url = f"sandbox:///{rel_path}"
    if media_type == "image":
        return f"![{name}]({url})"
    elif media_type == "video":
        return f"[Video: {name}]({url})"
    else:
        return f"[Audio: {name}]({url})"
