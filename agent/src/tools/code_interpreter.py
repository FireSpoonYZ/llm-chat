from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._media import ALL_MEDIA_EXTENSIONS, classify_media, format_sandbox_ref
from .result_schema import make_tool_error, make_tool_success

# code_interpreter also supports SVG output
_SCAN_EXTENSIONS = ALL_MEDIA_EXTENSIONS | frozenset({".svg"})
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


def _scan_media_files(workspace: str) -> set[str]:
    """Return a set of relative paths for media files under workspace."""
    media = set()
    for root, _dirs, files in os.walk(workspace):
        _dirs[:] = [d for d in _dirs if d not in _SKIP_DIRS]
        for f in files:
            ext = os.path.splitext(f)[1].lower()
            if ext in _SCAN_EXTENSIONS:
                full = os.path.join(root, f)
                rel = os.path.relpath(full, workspace)
                media.add(rel)
    return media


def _format_media(new_files: set[str]) -> tuple[str, list[dict[str, Any]]]:
    """Return markdown refs and structured media list for new files."""
    if not new_files:
        return "", []

    lines: list[str] = []
    media: list[dict[str, Any]] = []
    for rel in sorted(new_files):
        ext = os.path.splitext(rel)[1].lower()
        media_type = classify_media(ext)
        if media_type:
            lines.append(format_sandbox_ref(rel, media_type))
            media.append(
                {
                    "type": media_type,
                    "name": os.path.basename(rel),
                    "url": f"sandbox:///{rel}",
                }
            )
        else:
            # SVG or other non-classified media — treat as image
            name = os.path.basename(rel)
            lines.append(f"![{name}](sandbox:///{rel})")
            media.append(
                {
                    "type": "image",
                    "name": name,
                    "url": f"sandbox:///{rel}",
                }
            )

    return "\n\n" + "\n\n".join(lines), media


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
    known_media_files: set[str] = Field(default_factory=set)
    media_index_initialized: bool = Field(default=False)

    def _run(self, code: str, language: str = "python") -> dict[str, Any]:
        """Execute code synchronously and return structured output."""
        ext = ".py" if language == "python" else ".js"
        cmd_prefix = [sys.executable] if language == "python" else ["node"]
        tmp_path: str | None = None

        if not self.media_index_initialized:
            self.known_media_files = _scan_media_files(self.workspace)
            self.media_index_initialized = True

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

            output = (result.stdout + result.stderr)[:50000]

            # Scan for new media files after execution
            after = _scan_media_files(self.workspace)
            new_files = after - self.known_media_files
            self.known_media_files = after
            media_text, media_refs = _format_media(new_files)
            text = (output + media_text).strip() or "(no output)"

            success = result.returncode == 0
            error = None if success else f"code exited with status {result.returncode}"

            return make_tool_success(
                kind=self.name,
                text=text,
                data={
                    "language": language,
                    "exit_code": result.returncode,
                    "media": media_refs,
                },
                meta={"truncated": len(result.stdout + result.stderr) > 50000},
            ) if success else make_tool_error(
                kind=self.name,
                error=error or "code execution failed",
                text=text,
                data={
                    "language": language,
                    "exit_code": result.returncode,
                    "media": media_refs,
                },
                meta={"truncated": len(result.stdout + result.stderr) > 50000},
            )

        except subprocess.TimeoutExpired:
            return make_tool_error(
                kind=self.name,
                error="Code execution timed out after 30 seconds",
            )
        except Exception as exc:
            return make_tool_error(kind=self.name, error=f"executing code failed: {exc}")
        finally:
            if tmp_path and os.path.exists(tmp_path):
                try:
                    os.remove(tmp_path)
                except OSError:
                    pass

    async def _arun(self, code: str, language: str = "python") -> dict[str, Any]:
        """Execute code asynchronously."""
        return self._run(code, language)
