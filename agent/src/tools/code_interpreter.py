from __future__ import annotations

import asyncio
import os
import subprocess
import sys
import tempfile
from typing import Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._media import ALL_MEDIA_EXTENSIONS, classify_media, format_sandbox_ref

# code_interpreter also supports SVG output
_SCAN_EXTENSIONS = ALL_MEDIA_EXTENSIONS | frozenset({".svg"})


def _scan_media_files(workspace: str) -> set[str]:
    """Return a set of relative paths for media files under workspace."""
    media = set()
    for root, _dirs, files in os.walk(workspace):
        for f in files:
            ext = os.path.splitext(f)[1].lower()
            if ext in _SCAN_EXTENSIONS:
                full = os.path.join(root, f)
                rel = os.path.relpath(full, workspace)
                media.add(rel)
    return media


def _format_media_refs(new_files: set[str]) -> str:
    """Format sandbox:// references for newly created media files."""
    if not new_files:
        return ""
    lines = []
    for rel in sorted(new_files):
        ext = os.path.splitext(rel)[1].lower()
        media_type = classify_media(ext)
        if media_type:
            lines.append(format_sandbox_ref(rel, media_type))
        else:
            # SVG or other non-classified media — treat as image
            name = os.path.basename(rel)
            lines.append(f"![{name}](sandbox:///{rel})")
    return "\n\n" + "\n\n".join(lines)


class CodeInterpreterInput(BaseModel):
    """Input schema for the CodeInterpreterTool."""

    code: str = Field(..., description="The source code to execute.")
    language: str = Field(
        default="python",
        description="The programming language to use.",
        json_schema_extra={"enum": ["python", "javascript"]},
    )


class CodeInterpreterTool(BaseTool):
    """Execute Python or JavaScript code and return the output."""

    name: str = "code_interpreter"
    description: str = "Execute Python or JavaScript code and return the output."
    args_schema: Type[BaseModel] = CodeInterpreterInput
    workspace: str = "/workspace"

    def _run(self, code: str, language: str = "python") -> str:
        """Execute code synchronously and return combined stdout/stderr."""
        ext = ".py" if language == "python" else ".js"
        cmd_prefix = [sys.executable] if language == "python" else ["node"]
        tmp_path: str | None = None

        # Scan media files before execution
        before = _scan_media_files(self.workspace)

        try:
            with tempfile.NamedTemporaryFile(
                mode="w",
                suffix=ext,
                dir=self.workspace,
                delete=False,
            ) as tmp:
                tmp.write(code)
                tmp_path = tmp.name

            result = subprocess.run(
                [*cmd_prefix, tmp_path],
                capture_output=True,
                text=True,
                timeout=30,
                cwd=self.workspace,
            )

            output = result.stdout + result.stderr
            output = output[:50000]

            # Scan for new media files after execution
            after = _scan_media_files(self.workspace)
            new_files = after - before
            output += _format_media_refs(new_files)

            return output

        except subprocess.TimeoutExpired:
            return "Error: Code execution timed out after 30 seconds."
        except Exception as exc:
            return f"Error executing code: {exc}"
        finally:
            if tmp_path and os.path.exists(tmp_path):
                os.remove(tmp_path)

    async def _arun(self, code: str, language: str = "python") -> str:
        """Execute code asynchronously via asyncio.to_thread."""
        return await asyncio.to_thread(self._run, code, language)
