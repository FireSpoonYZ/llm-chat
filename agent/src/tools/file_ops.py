"""File operation tools for LangChain agents running in Docker."""

from __future__ import annotations

import asyncio
import base64
import os
from pathlib import Path
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._paths import resolve_workspace_path as _resolve_path

IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".gif", ".webp"}
MAX_IMAGE_SIZE = 10 * 1024 * 1024  # 10 MB


# ---------------------------------------------------------------------------
# Input schemas
# ---------------------------------------------------------------------------

class ReadInput(BaseModel):
    """Input schema for ReadTool."""

    file_path: str = Field(description="Path to the file to read.")
    offset: int = Field(
        default=0,
        description="Line number to start reading from (0-based).",
    )
    limit: int = Field(
        default=2000,
        description="Maximum number of lines to return.",
    )


class WriteInput(BaseModel):
    """Input schema for WriteTool."""

    file_path: str = Field(description="Path to the file to write.")
    content: str = Field(description="Content to write to the file.")


class EditInput(BaseModel):
    """Input schema for EditTool."""

    file_path: str = Field(description="Path to the file to edit.")
    old_string: str = Field(description="The exact text to find and replace.")
    new_string: str = Field(description="The replacement text.")
    replace_all: bool = Field(
        default=False,
        description="If True, replace every occurrence; otherwise require exactly one match.",
    )


# ---------------------------------------------------------------------------
# ReadTool
# ---------------------------------------------------------------------------

class ReadTool(BaseTool):
    """Read the contents of a file inside the workspace."""

    name: str = "read"
    description: str = (
        "Read a file from the workspace and return its contents with "
        "line numbers. Supports offset and limit for large files."
    )
    args_schema: Type[BaseModel] = ReadInput
    workspace: str = "/workspace"

    def _run(
        self, file_path: str, offset: int = 0, limit: int = 2000
    ) -> str | list[dict[str, Any]]:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            ext = os.path.splitext(resolved)[1].lower()

            # Image files: return multimodal content
            if ext in IMAGE_EXTENSIONS:
                size = resolved.stat().st_size
                if size == 0:
                    return f"Error: image file is empty: {file_path}"
                if size > MAX_IMAGE_SIZE:
                    return f"Error: image file too large ({size} bytes, max {MAX_IMAGE_SIZE}): {file_path}"
                data = resolved.read_bytes()
                b64 = base64.b64encode(data).decode("ascii")
                mime = f"image/{ext.lstrip('.')}"
                if ext == ".jpg":
                    mime = "image/jpeg"
                return [
                    {"type": "text", "text": f"Image file: {file_path}"},
                    {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
                ]

            with open(resolved, "r", encoding="utf-8", errors="replace") as fh:
                lines = fh.readlines()
            selected = lines[offset : offset + limit]
            numbered = [
                f"{i + offset + 1:>6}\t{line}"
                for i, line in enumerate(selected)
            ]
            return "".join(numbered) if numbered else "(empty file)"
        except FileNotFoundError:
            return f"Error: file not found: {file_path}"
        except IsADirectoryError:
            return f"Error: path is a directory, not a file: {file_path}"
        except ValueError as exc:
            return f"Error: {exc}"
        except Exception as exc:
            return f"Error reading file: {exc}"

    async def _arun(
        self, file_path: str, offset: int = 0, limit: int = 2000
    ) -> str | list[dict[str, Any]]:
        return await asyncio.to_thread(self._run, file_path, offset, limit)


# ---------------------------------------------------------------------------
# WriteTool
# ---------------------------------------------------------------------------

class WriteTool(BaseTool):
    """Write content to a file inside the workspace."""

    name: str = "write"
    description: str = (
        "Create or overwrite a file in the workspace with the given "
        "content. Parent directories are created automatically."
    )
    args_schema: Type[BaseModel] = WriteInput
    workspace: str = "/workspace"

    def _run(self, file_path: str, content: str) -> str:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            resolved.parent.mkdir(parents=True, exist_ok=True)
            with open(resolved, "w", encoding="utf-8") as fh:
                fh.write(content)
            return f"Successfully wrote {len(content)} characters to {file_path}."
        except ValueError as exc:
            return f"Error: {exc}"
        except Exception as exc:
            return f"Error writing file: {exc}"

    async def _arun(self, file_path: str, content: str) -> str:
        return await asyncio.to_thread(self._run, file_path, content)


# ---------------------------------------------------------------------------
# EditTool
# ---------------------------------------------------------------------------

class EditTool(BaseTool):
    """Find-and-replace text in an existing file inside the workspace."""

    name: str = "edit"
    description: str = (
        "Replace occurrences of a string in a file. By default the "
        "target string must appear exactly once; set replace_all=True "
        "to replace every occurrence."
    )
    args_schema: Type[BaseModel] = EditInput
    workspace: str = "/workspace"

    def _run(
        self,
        file_path: str,
        old_string: str,
        new_string: str,
        replace_all: bool = False,
    ) -> str:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            with open(resolved, "r", encoding="utf-8", errors="replace") as fh:
                content = fh.read()

            count = content.count(old_string)
            if count == 0:
                return "Error: old_string not found in the file."
            if not replace_all and count > 1:
                return (
                    f"Error: old_string appears {count} times. "
                    "Use replace_all=True or provide a more unique string."
                )

            new_content = content.replace(old_string, new_string, -1 if replace_all else 1)
            with open(resolved, "w", encoding="utf-8") as fh:
                fh.write(new_content)

            replacements = count if replace_all else 1
            return (
                f"Successfully replaced {replacements} occurrence(s) "
                f"in {file_path}."
            )
        except FileNotFoundError:
            return f"Error: file not found: {file_path}"
        except ValueError as exc:
            return f"Error: {exc}"
        except Exception as exc:
            return f"Error editing file: {exc}"

    async def _arun(
        self,
        file_path: str,
        old_string: str,
        new_string: str,
        replace_all: bool = False,
    ) -> str:
        return await asyncio.to_thread(
            self._run, file_path, old_string, new_string, replace_all
        )
