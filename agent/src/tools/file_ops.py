"""File operation tools for LangChain agents running in Docker."""

from __future__ import annotations

import base64
import os
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._media import MEDIA_TYPES, classify_media, sandbox_url
from ._paths import resolve_workspace_path as _resolve_path
from .result_schema import make_tool_error, make_tool_success

IMAGE_EXTENSIONS = MEDIA_TYPES["image"]
MAX_IMAGE_SIZE = 10 * 1024 * 1024  # 10 MB
MAX_MEDIA_SIZE = 100 * 1024 * 1024  # 100 MB for video/audio


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
    ) -> dict[str, Any]:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            ext = os.path.splitext(resolved)[1].lower()

            # Image files: return multimodal content
            if ext in IMAGE_EXTENSIONS:
                size = resolved.stat().st_size
                if size == 0:
                    return make_tool_error(
                        kind=self.name,
                        error=f"image file is empty: {file_path}",
                    )
                if size > MAX_IMAGE_SIZE:
                    return make_tool_error(
                        kind=self.name,
                        error=f"image file too large ({size} bytes, max {MAX_IMAGE_SIZE}): {file_path}",
                    )
                data = resolved.read_bytes()
                b64 = base64.b64encode(data).decode("ascii")
                mime = f"image/{ext.lstrip('.')}"
                if ext in (".jpg", ".jpeg"):
                    mime = "image/jpeg"
                s_url = sandbox_url(resolved, self.workspace)
                text = f"Image file: {file_path}\n![{file_path}]({s_url})"
                llm_content: list[dict[str, Any]] = [
                    {"type": "text", "text": f"Image file: {file_path}\n![{file_path}]({s_url})"},
                    {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{b64}"}},
                ]
                return make_tool_success(
                    kind=self.name,
                    text=text,
                    data={
                        "file_path": file_path,
                        "media": [
                            {
                                "type": "image",
                                "name": os.path.basename(file_path),
                                "url": s_url,
                                "mime": mime,
                                "size": size,
                            }
                        ],
                    },
                    meta={"bytes": size},
                    llm_content=llm_content,
                )

            # Video/audio files: return text with sandbox URL
            media_type = classify_media(ext)
            if media_type in ("video", "audio"):
                size = resolved.stat().st_size
                if size == 0:
                    return make_tool_error(
                        kind=self.name,
                        error=f"{media_type} file is empty: {file_path}",
                    )
                if size > MAX_MEDIA_SIZE:
                    return make_tool_error(
                        kind=self.name,
                        error=f"{media_type} file too large ({size} bytes, max {MAX_MEDIA_SIZE}): {file_path}",
                    )
                s_url = sandbox_url(resolved, self.workspace)
                name = os.path.basename(file_path)
                label = media_type.capitalize()
                text = f"{label} file: {file_path} ({size} bytes)\n[{label}: {name}]({s_url})"
                return make_tool_success(
                    kind=self.name,
                    text=text,
                    data={
                        "file_path": file_path,
                        "media": [
                            {
                                "type": media_type,
                                "name": name,
                                "url": s_url,
                                "size": size,
                            }
                        ],
                    },
                    meta={"bytes": size},
                )

            safe_offset = max(offset, 0)
            safe_limit = max(limit, 0)
            numbered: list[str] = []
            if safe_limit > 0:
                with open(resolved, "r", encoding="utf-8", errors="replace") as fh:
                    selected_idx = 0
                    for line_no, line in enumerate(fh, start=1):
                        if line_no <= safe_offset:
                            continue
                        if selected_idx >= safe_limit:
                            break
                        numbered.append(f"{line_no:>6}\t{line}")
                        selected_idx += 1
            text = "".join(numbered) if numbered else "(empty file)"
            return make_tool_success(
                kind=self.name,
                text=text,
                data={
                    "file_path": file_path,
                    "offset": safe_offset,
                    "limit": safe_limit,
                    "lines_returned": len(numbered),
                },
            )
        except FileNotFoundError:
            return make_tool_error(kind=self.name, error=f"file not found: {file_path}")
        except IsADirectoryError:
            return make_tool_error(
                kind=self.name,
                error=f"path is a directory, not a file: {file_path}",
            )
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))
        except Exception as exc:
            return make_tool_error(kind=self.name, error=f"reading file failed: {exc}")

    async def _arun(
        self, file_path: str, offset: int = 0, limit: int = 2000
    ) -> dict[str, Any]:
        return self._run(file_path, offset, limit)


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

    def _run(self, file_path: str, content: str) -> dict[str, Any]:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            resolved.parent.mkdir(parents=True, exist_ok=True)
            with open(resolved, "w", encoding="utf-8") as fh:
                fh.write(content)
            return make_tool_success(
                kind=self.name,
                text=f"Successfully wrote {len(content)} characters to {file_path}.",
                data={"file_path": file_path, "chars_written": len(content)},
            )
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))
        except Exception as exc:
            return make_tool_error(kind=self.name, error=f"writing file failed: {exc}")

    async def _arun(self, file_path: str, content: str) -> dict[str, Any]:
        return self._run(file_path, content)


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
    ) -> dict[str, Any]:
        try:
            resolved = _resolve_path(file_path, self.workspace)
            with open(resolved, "r", encoding="utf-8", errors="replace") as fh:
                content = fh.read()

            count = content.count(old_string)
            if count == 0:
                return make_tool_error(kind=self.name, error="old_string not found in the file")
            if not replace_all and count > 1:
                return make_tool_error(
                    kind=self.name,
                    error=(
                        f"old_string appears {count} times. "
                        "Use replace_all=True or provide a more unique string."
                    ),
                )

            new_content = content.replace(old_string, new_string, -1 if replace_all else 1)
            with open(resolved, "w", encoding="utf-8") as fh:
                fh.write(new_content)

            replacements = count if replace_all else 1
            return make_tool_success(
                kind=self.name,
                text=(
                    f"Successfully replaced {replacements} occurrence(s) "
                    f"in {file_path}."
                ),
                data={
                    "file_path": file_path,
                    "replacements": replacements,
                    "replace_all": replace_all,
                },
            )
        except FileNotFoundError:
            return make_tool_error(kind=self.name, error=f"file not found: {file_path}")
        except ValueError as exc:
            return make_tool_error(kind=self.name, error=str(exc))
        except Exception as exc:
            return make_tool_error(kind=self.name, error=f"editing file failed: {exc}")

    async def _arun(
        self,
        file_path: str,
        old_string: str,
        new_string: str,
        replace_all: bool = False,
    ) -> dict[str, Any]:
        return self._run(file_path, old_string, new_string, replace_all)
