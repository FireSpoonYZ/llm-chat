"""Shared workspace path resolution and validation."""

from __future__ import annotations

from pathlib import Path


def resolve_workspace_path(file_path: str, workspace: str) -> Path:
    """Return an absolute *Path* guaranteed to live under *workspace*.

    Uses ``Path.is_relative_to()`` (Python 3.9+) for correct containment
    checking — unlike ``str.startswith()``, this cannot be bypassed by
    sibling directories with a shared prefix (e.g. ``/workspace2``).

    Raises ``ValueError`` if the resolved path escapes the workspace.
    """
    ws = Path(workspace).resolve()
    p = Path(file_path)
    if not p.is_absolute():
        p = ws / p
    resolved = p.resolve()
    if not resolved.is_relative_to(ws):
        raise ValueError(
            f"Access denied: {file_path!r} resolves outside the workspace."
        )
    return resolved
