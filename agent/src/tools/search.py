from __future__ import annotations

import asyncio
import os
import re
from pathlib import Path
from typing import Optional, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._paths import resolve_workspace_path


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
    path: str = Field(default="", description="Directory to search in. Defaults to workspace root.")


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

    def _resolve_and_validate(self, path: str) -> Path:
        """Resolve a path and ensure it is within the workspace."""
        return resolve_workspace_path(path or ".", self.workspace)

    def _run(self, pattern: str, path: str = "") -> str:
        base = self._resolve_and_validate(path)
        if not base.exists():
            return f"Error: path '{path}' does not exist."
        if not base.is_dir():
            return f"Error: path '{path}' is not a directory."

        ws = Path(self.workspace).resolve()
        results: list[str] = []
        seen: set[str] = set()
        try:
            for expanded in _expand_braces(pattern):
                for match in base.glob(expanded):
                    if len(results) >= 1000:
                        break
                    if match.is_file():
                        try:
                            rel = str(match.relative_to(ws))
                        except ValueError:
                            continue
                        if rel not in seen:
                            seen.add(rel)
                            results.append(rel)
                if len(results) >= 1000:
                    break
        except OSError as exc:
            return f"Error during glob: {exc}"

        if not results:
            return "No files matched the pattern."
        results.sort()
        return "\n".join(results)

    async def _arun(self, pattern: str, path: str = "") -> str:
        return await asyncio.to_thread(self._run, pattern, path)


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
        ws = Path(self.workspace).resolve()
        if path:
            resolved = (ws / path).resolve()
        else:
            resolved = ws
        if not str(resolved).startswith(str(ws)):
            raise ValueError(f"Path '{path}' is outside the workspace.")
        return resolved

    def _collect_files(self, base: Path, glob_filter: str) -> list[Path]:
        """Collect files to search, optionally filtered by a glob pattern."""
        if base.is_file():
            return [base]
        if glob_filter:
            seen: set[Path] = set()
            files: list[Path] = []
            for expanded in _expand_braces(glob_filter):
                for f in base.glob(expanded):
                    if f.is_file() and f not in seen:
                        seen.add(f)
                        files.append(f)
            return sorted(files)
        return sorted(f for f in base.rglob("*") if f.is_file())

    def _is_binary(self, filepath: Path) -> bool:
        """Heuristic check for binary files."""
        try:
            with open(filepath, "rb") as fh:
                chunk = fh.read(8192)
            return b"\x00" in chunk
        except OSError:
            return True

    def _run(
        self,
        pattern: str,
        path: str = "",
        glob_filter: str = "",
        context: int = 0,
    ) -> str:
        base = self._resolve_and_validate(path)
        if not base.exists():
            return f"Error: path '{path}' does not exist."

        try:
            regex = re.compile(pattern)
        except re.error as exc:
            return f"Error: invalid regex pattern: {exc}"

        ws = Path(self.workspace).resolve()
        files = self._collect_files(base, glob_filter)
        output_parts: list[str] = []
        total_len = 0
        max_output = 50000

        for filepath in files:
            if self._is_binary(filepath):
                continue
            try:
                with open(filepath, "r", encoding="utf-8", errors="replace") as fh:
                    lines = fh.readlines()
            except OSError:
                continue

            try:
                rel = str(filepath.relative_to(ws))
            except ValueError:
                continue

            for lineno, line in enumerate(lines, start=1):
                if regex.search(line):
                    if context > 0:
                        start = max(0, lineno - 1 - context)
                        end = min(len(lines), lineno + context)
                        for ctx_idx in range(start, end):
                            ctx_lineno = ctx_idx + 1
                            entry = f"{rel}:{ctx_lineno}:{lines[ctx_idx].rstrip()}"
                            output_parts.append(entry)
                            total_len += len(entry) + 1
                        output_parts.append("--")
                        total_len += 3
                    else:
                        entry = f"{rel}:{lineno}:{line.rstrip()}"
                        output_parts.append(entry)
                        total_len += len(entry) + 1

                    if total_len >= max_output:
                        output_parts.append("... output truncated (50000 char limit)")
                        return "\n".join(output_parts)

        if not output_parts:
            return "No matches found."
        return "\n".join(output_parts)

    async def _arun(
        self,
        pattern: str,
        path: str = "",
        glob_filter: str = "",
        context: int = 0,
    ) -> str:
        return await asyncio.to_thread(self._run, pattern, path, glob_filter, context)
