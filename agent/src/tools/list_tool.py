from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._paths import resolve_workspace_path
from .result_schema import make_tool_error, make_tool_success

DEFAULT_IGNORE = [".git", "node_modules", ".venv", "dist", "build"]
MAX_ENTRIES = 2000


class ListInput(BaseModel):
    """Input for the ListTool."""

    path: str = Field(default=".", description="Directory path to list, relative to /workspace.")
    depth: int = Field(
        default=2,
        ge=0,
        le=16,
        description="Maximum recursion depth. 0 means only the target directory itself.",
    )
    ignore: list[str] = Field(
        default_factory=lambda: list(DEFAULT_IGNORE),
        description="Directory or file names to ignore while traversing.",
    )


class ListTool(BaseTool):
    """List directory contents as a tree-like structure."""

    name: str = "list"
    description: str = (
        "List files and folders under a directory as a structured tree. "
        "Supports depth limiting and ignore-name filters."
    )
    args_schema: Type[BaseModel] = ListInput
    workspace: str = "/workspace"

    def _run(
        self,
        path: str = ".",
        depth: int = 2,
        ignore: list[str] | None = None,
    ) -> dict[str, Any]:
        try:
            root = resolve_workspace_path(path or ".", self.workspace)
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))

        if not root.exists():
            return make_tool_error(kind=self.name, error=f"path does not exist: {path}")
        if not root.is_dir():
            return make_tool_error(kind=self.name, error=f"path is not a directory: {path}")

        ignore_names = {name.strip() for name in (ignore or DEFAULT_IGNORE) if name.strip()}
        ws_root = Path(self.workspace).resolve()
        entries: list[dict[str, Any]] = []
        lines: list[str] = []
        truncated = False

        def _is_ignored(entry: Path) -> bool:
            return entry.name in ignore_names

        def _rel(p: Path) -> str:
            try:
                return str(p.relative_to(ws_root))
            except ValueError:
                return str(p)

        def _fmt_mtime(p: Path) -> str | None:
            try:
                return datetime.fromtimestamp(p.stat().st_mtime, tz=timezone.utc).isoformat()
            except OSError:
                return None

        def _append_entry(entry: Path, *, current_depth: int) -> bool:
            nonlocal truncated
            if len(entries) >= MAX_ENTRIES:
                truncated = True
                return False

            is_dir = entry.is_dir()
            size: int | None = None
            if not is_dir:
                try:
                    size = entry.stat().st_size
                except OSError:
                    size = None

            rel_path = _rel(entry)
            entries.append({
                "path": rel_path,
                "name": entry.name,
                "type": "directory" if is_dir else "file",
                "size": size,
                "mtime": _fmt_mtime(entry),
                "depth": current_depth,
            })
            prefix = "  " * current_depth
            suffix = "/" if is_dir else ""
            lines.append(f"{prefix}{entry.name}{suffix}")
            return True

        # include root summary line
        root_label = _rel(root)
        lines.append(f"{root_label}/")

        queue: list[tuple[Path, int]] = [(root, 0)]
        while queue:
            current, current_depth = queue.pop(0)
            if current_depth >= depth:
                continue

            try:
                children = sorted(
                    [p for p in current.iterdir() if not _is_ignored(p)],
                    key=lambda p: (not p.is_dir(), p.name.lower()),
                )
            except OSError as exc:
                return make_tool_error(kind=self.name, error=f"failed to list '{_rel(current)}': {exc}")

            for child in children:
                if not _append_entry(child, current_depth=current_depth + 1):
                    break
                if child.is_dir():
                    queue.append((child, current_depth + 1))
            if truncated:
                break

        text = "\n".join(lines)
        if truncated:
            text += f"\n... truncated at {MAX_ENTRIES} entries"

        return make_tool_success(
            kind=self.name,
            text=text,
            data={
                "path": _rel(root),
                "depth": depth,
                "ignore": sorted(ignore_names),
                "entries": entries,
            },
            meta={
                "entry_count": len(entries),
                "truncated": truncated,
            },
        )

    async def _arun(
        self,
        path: str = ".",
        depth: int = 2,
        ignore: list[str] | None = None,
    ) -> dict[str, Any]:
        return self._run(path, depth, ignore)
