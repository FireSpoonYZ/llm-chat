from __future__ import annotations

import asyncio
import os
import subprocess
import sys
import tempfile
from typing import Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field


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
            return output[:50000]

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
