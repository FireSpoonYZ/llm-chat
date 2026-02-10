"""Bash command execution tool for LangChain agents running in Docker."""

from __future__ import annotations

import asyncio
import subprocess
from typing import Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

MAX_OUTPUT_CHARS = 50000


class BashInput(BaseModel):
    """Input schema for the BashTool."""

    command: str = Field(description="The shell command to execute.")
    timeout: int = Field(
        default=30,
        description="Maximum number of seconds the command is allowed to run.",
    )


class BashTool(BaseTool):
    """Execute shell commands inside the agent's Docker container."""

    name: str = "bash"
    description: str = (
        "Execute a shell command in the workspace directory. "
        "Use this to run programs, install packages, inspect the "
        "filesystem, or perform any operation available from the "
        "command line. The command runs inside a Docker container "
        "with /workspace as the working directory."
    )
    args_schema: Type[BaseModel] = BashInput
    workspace: str = "/workspace"

    def _run(self, command: str, timeout: int = 30) -> str:
        """Execute *command* synchronously and return combined output."""
        try:
            result = subprocess.run(
                command,
                shell=True,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.workspace,
            )
            output = result.stdout + result.stderr
            if len(output) > MAX_OUTPUT_CHARS:
                output = output[:MAX_OUTPUT_CHARS] + "\n... [output truncated]"
            return output if output else "(no output)"
        except subprocess.TimeoutExpired:
            return f"Error: command timed out after {timeout} seconds."
        except Exception as exc:
            return f"Error executing command: {exc}"

    async def _arun(self, command: str, timeout: int = 30) -> str:
        """Execute *command* asynchronously."""
        return await asyncio.to_thread(self._run, command, timeout)
