from __future__ import annotations

import asyncio
import os
import re
from pathlib import Path
from typing import Optional, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field


class GlobInput(BaseModel):
    """Input for the GlobTool."""

    pattern: str = Field(description="Glob pattern to match files against.")
    path: str = Field(default="", description="Directory to search in. Defaults to workspace root.")


class GlobTool(BaseTool):
    """Search for files matching a glob pattern within the workspace."""

    name: str = "glob"
    description: str = (
        "Fast file pattern matching tool. Supports glob patterns like '**/*.py' or 'src/**/*.ts'. "
        "Returns matching file paths relative to the workspace root."
    )
    args_schema: Type[BaseModel] = GlobInput
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

    def _run(self, pattern: str, path: str = "") -> str:
        base = self._resolve_and_validate(path)
        if not base.exists():
            return f"Error: path '{path}' does not exist."
        if not base.is_dir():
            return f"Error: path '{path}' is not a directory."

        ws = Path(self.workspace).resolve()
        results: list[str] = []
        try:
            for i, match in enumerate(sorted(base.glob(pattern))):
                if i >= 1000:
                    break
                if match.is_file():
                    try:
                        rel = match.relative_to(ws)
                    except ValueError:
                        continue
                    results.append(str(rel))
        except OSError as exc:
            return f"Error during glob: {exc}"

        if not results:
            return "No files matched the pattern."
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
            return sorted(f for f in base.glob(glob_filter) if f.is_file())
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
