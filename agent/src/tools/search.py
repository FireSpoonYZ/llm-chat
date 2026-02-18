from __future__ import annotations

import os
import re
from collections import deque
from pathlib import Path
from typing import Any, Iterator, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._paths import resolve_workspace_path
from .result_schema import make_tool_error, make_tool_success

_SKIP_DIRS = {
    ".git",
    ".hg",
    ".svn",
    ".idea",
    ".vscode",
    "__pycache__",
    "node_modules",
    ".venv",
    "venv",
    "dist",
    "build",
    "target",
}


def _expand_braces(pattern: str) -> list[str]:
    """Expand bash-style brace patterns (e.g. ``*.{py,txt}``) into separate globs.

    Python's ``pathlib.glob`` does not support brace expansion, so this helper
    recursively expands ``{a,b,c}`` groups into individual patterns.
    """
    match = re.search(r"\{([^{}]+)\}", pattern)
    if not match:
        return [pattern]
    prefix = pattern[: match.start()]
    suffix = pattern[match.end() :]
    results: list[str] = []
    for alt in match.group(1).split(","):
        results.extend(_expand_braces(prefix + alt.strip() + suffix))
    return results


class GlobInput(BaseModel):
    """Input for the GlobTool."""

    pattern: str = Field(description="Glob pattern to match files against.")
    path: str | None = Field(
        default=None,
        description="Directory to search in. Omit or leave empty to use workspace root.",
    )


class GlobTool(BaseTool):
    """Search for files matching a glob pattern within the workspace."""

    name: str = "glob"
    description: str = (
        "Fast file pattern matching tool. Supports glob patterns like '**/*.py' or 'src/**/*.ts'. "
        "Brace expansion is supported (e.g. '**/*.{py,txt}'). "
        "Returns matching file paths relative to the workspace root."
    )
    args_schema: Type[BaseModel] = GlobInput
    workspace: str = "/workspace"

    def _normalize_path(self, path: str | None) -> str:
        """Normalize optional path input for consistent default behavior."""
        if path is None:
            return "."
        if path == "" or path.isspace():
            return "."
        return path

    def _resolve_and_validate(self, path: str | None) -> Path:
        """Resolve a path and ensure it is within the workspace."""
        return resolve_workspace_path(self._normalize_path(path), self.workspace)

    def _run(self, pattern: str, path: str | None = None) -> dict[str, Any]:
        normalized_path = self._normalize_path(path)
        try:
            base = self._resolve_and_validate(path)
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))

        if not base.exists():
            return make_tool_error(kind=self.name, error=f"path '{normalized_path}' does not exist")
        if not base.is_dir():
            return make_tool_error(kind=self.name, error=f"path '{normalized_path}' is not a directory")

        ws = Path(self.workspace).resolve()
        results: list[str] = []
        seen: set[str] = set()
        truncated = False
        try:
            for expanded in _expand_braces(pattern):
                for match in base.glob(expanded):
                    if len(results) >= 1000:
                        truncated = True
                        break
                    if not match.is_file():
                        continue
                    try:
                        rel = str(match.relative_to(ws))
                    except ValueError:
                        continue
                    if rel not in seen:
                        seen.add(rel)
                        results.append(rel)
                if truncated:
                    break
        except OSError as exc:
            return make_tool_error(kind=self.name, error=f"glob failed: {exc}")

        results.sort()
        if not results:
            return make_tool_success(
                kind=self.name,
                text="No files matched the pattern.",
                data={"paths": [], "pattern": pattern, "path": normalized_path},
                meta={"match_count": 0, "truncated": False},
            )
        return make_tool_success(
            kind=self.name,
            text="\n".join(results),
            data={"paths": results, "pattern": pattern, "path": normalized_path},
            meta={"match_count": len(results), "truncated": truncated},
        )

    async def _arun(self, pattern: str, path: str | None = None) -> dict[str, Any]:
        return self._run(pattern, path)


class GrepInput(BaseModel):
    """Input for the GrepTool."""

    pattern: str = Field(description="Regular expression pattern to search for.")
    path: str = Field(default="", description="File or directory to search in. Defaults to workspace root.")
    glob_filter: str = Field(default="", description="Glob pattern to filter which files are searched.")
    context: int = Field(default=0, description="Number of context lines to show before and after each match.")


class GrepTool(BaseTool):
    """Search file contents using regular expressions within the workspace."""

    name: str = "grep"
    description: str = (
        "Search for a regular expression pattern in file contents. "
        "Returns matching lines in the format filepath:lineno:line_content."
    )
    args_schema: Type[BaseModel] = GrepInput
    workspace: str = "/workspace"

    def _resolve_and_validate(self, path: str) -> Path:
        """Resolve a path and ensure it is within the workspace."""
        return resolve_workspace_path(path or ".", self.workspace)

    def _iter_files(self, base: Path, glob_filter: str) -> Iterator[Path]:
        """Yield files to search, optionally filtered by a glob pattern."""
        if base.is_file():
            yield base
            return

        if glob_filter:
            seen: set[Path] = set()
            for expanded in _expand_braces(glob_filter):
                for f in base.glob(expanded):
                    if f.is_file() and f not in seen:
                        seen.add(f)
                        yield f
            return

        for root, dirs, files in os.walk(base):
            dirs[:] = sorted(d for d in dirs if d not in _SKIP_DIRS)
            for name in sorted(files):
                yield Path(root) / name

    def _is_binary(self, filepath: Path) -> bool:
        """Heuristic check for binary files."""
        try:
            with open(filepath, "rb") as fh:
                chunk = fh.read(8192)
            return b"\x00" in chunk
        except OSError:
            return True

    def _success_result(
        self,
        *,
        text: str,
        pattern: str,
        path: str,
        glob_filter: str,
        context: int,
        match_count: int,
        truncated: bool,
    ) -> dict[str, Any]:
        return make_tool_success(
            kind=self.name,
            text=text,
            data={
                "pattern": pattern,
                "path": path or ".",
                "glob_filter": glob_filter,
                "context": context,
            },
            meta={"match_count": match_count, "truncated": truncated},
        )

    def _run(
        self,
        pattern: str,
        path: str = "",
        glob_filter: str = "",
        context: int = 0,
    ) -> dict[str, Any]:
        try:
            base = self._resolve_and_validate(path)
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))
        if not base.exists():
            return make_tool_error(kind=self.name, error=f"path '{path}' does not exist")

        try:
            regex = re.compile(pattern)
        except re.error as exc:
            return make_tool_error(kind=self.name, error=f"invalid regex pattern: {exc}")

        ws = Path(self.workspace).resolve()
        output_parts: list[str] = []
        total_len = 0
        max_output = 50000
        match_count = 0
        truncated = False

        for filepath in self._iter_files(base, glob_filter):
            if self._is_binary(filepath):
                continue

            try:
                rel = str(filepath.relative_to(ws))
            except ValueError:
                continue

            try:
                if context <= 0:
                    with open(filepath, "r", encoding="utf-8", errors="replace") as fh:
                        for lineno, line in enumerate(fh, start=1):
                            if not regex.search(line):
                                continue
                            entry = f"{rel}:{lineno}:{line.rstrip()}"
                            output_parts.append(entry)
                            match_count += 1
                            total_len += len(entry) + 1
                            if total_len >= max_output:
                                output_parts.append("... output truncated (50000 char limit)")
                                truncated = True
                                text = "\n".join(output_parts)
                                return self._success_result(
                                    text=text,
                                    pattern=pattern,
                                    path=path,
                                    glob_filter=glob_filter,
                                    context=context,
                                    match_count=match_count,
                                    truncated=truncated,
                                )
                    continue

                prev_lines: deque[tuple[int, str]] = deque(maxlen=context)
                trailing_remaining = 0
                last_emitted_lineno = 0
                with open(filepath, "r", encoding="utf-8", errors="replace") as fh:
                    for lineno, line in enumerate(fh, start=1):
                        stripped = line.rstrip()
                        matched = bool(regex.search(line))

                        if matched:
                            for prev_lineno, prev_text in prev_lines:
                                if prev_lineno <= last_emitted_lineno:
                                    continue
                                entry = f"{rel}:{prev_lineno}:{prev_text}"
                                output_parts.append(entry)
                                match_count += 1
                                total_len += len(entry) + 1
                                last_emitted_lineno = prev_lineno
                                if total_len >= max_output:
                                    output_parts.append("... output truncated (50000 char limit)")
                                    truncated = True
                                    text = "\n".join(output_parts)
                                    return self._success_result(
                                        text=text,
                                        pattern=pattern,
                                        path=path,
                                        glob_filter=glob_filter,
                                        context=context,
                                        match_count=match_count,
                                        truncated=truncated,
                                    )

                            if lineno > last_emitted_lineno:
                                entry = f"{rel}:{lineno}:{stripped}"
                                output_parts.append(entry)
                                match_count += 1
                                total_len += len(entry) + 1
                                last_emitted_lineno = lineno
                                if total_len >= max_output:
                                    output_parts.append("... output truncated (50000 char limit)")
                                    truncated = True
                                    text = "\n".join(output_parts)
                                    return self._success_result(
                                        text=text,
                                        pattern=pattern,
                                        path=path,
                                        glob_filter=glob_filter,
                                        context=context,
                                        match_count=match_count,
                                        truncated=truncated,
                                    )
                            trailing_remaining = context
                        elif trailing_remaining > 0:
                            if lineno > last_emitted_lineno:
                                entry = f"{rel}:{lineno}:{stripped}"
                                output_parts.append(entry)
                                match_count += 1
                                total_len += len(entry) + 1
                                last_emitted_lineno = lineno
                                if total_len >= max_output:
                                    output_parts.append("... output truncated (50000 char limit)")
                                    truncated = True
                                    text = "\n".join(output_parts)
                                    return self._success_result(
                                        text=text,
                                        pattern=pattern,
                                        path=path,
                                        glob_filter=glob_filter,
                                        context=context,
                                        match_count=match_count,
                                        truncated=truncated,
                                    )
                            trailing_remaining -= 1
                            if trailing_remaining == 0:
                                output_parts.append("--")
                                total_len += 3
                                if total_len >= max_output:
                                    output_parts.append("... output truncated (50000 char limit)")
                                    truncated = True
                                    text = "\n".join(output_parts)
                                    return self._success_result(
                                        text=text,
                                        pattern=pattern,
                                        path=path,
                                        glob_filter=glob_filter,
                                        context=context,
                                        match_count=match_count,
                                        truncated=truncated,
                                    )

                        prev_lines.append((lineno, stripped))
            except OSError:
                continue

        if not output_parts:
            return self._success_result(
                text="No matches found.",
                pattern=pattern,
                path=path,
                glob_filter=glob_filter,
                context=context,
                match_count=0,
                truncated=False,
            )

        return self._success_result(
            text="\n".join(output_parts),
            pattern=pattern,
            path=path,
            glob_filter=glob_filter,
            context=context,
            match_count=match_count,
            truncated=truncated,
        )

    async def _arun(
        self,
        pattern: str,
        path: str = "",
        glob_filter: str = "",
        context: int = 0,
    ) -> dict[str, Any]:
        return self._run(pattern, path, glob_filter, context)
