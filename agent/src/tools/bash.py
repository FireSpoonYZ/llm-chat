"""Bash command execution tool for LangChain agents running in Docker."""

from __future__ import annotations

import asyncio
import os
import signal
import subprocess
import time
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from .result_schema import make_tool_result

MAX_OUTPUT_CHARS = 50000
MAX_STDIO_FIELD_CHARS = 25000


class BashInput(BaseModel):
    """Input schema for the BashTool."""

    command: str = Field(description="The shell command to execute.")
    timeout: int = Field(
        default=120,
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

    def _terminate_process_group(self, pid: int | None) -> None:
        """Terminate a spawned command and its children."""
        if pid is None:
            return
        try:
            os.killpg(pid, signal.SIGKILL)
        except (AttributeError, ProcessLookupError):
            # Windows or already exited process.
            pass
        except Exception:
            pass

    def _truncate(self, text: str, limit: int) -> tuple[str, bool]:
        if len(text) <= limit:
            return text, False
        return text[:limit] + "\n... [output truncated]", True

    def _build_result(
        self,
        *,
        stdout: str,
        stderr: str,
        exit_code: int | None,
        duration_ms: int,
        timed_out: bool = False,
        error: bool = False,
    ) -> dict[str, Any]:
        combined = stdout + stderr
        text, text_truncated = self._truncate(combined, MAX_OUTPUT_CHARS)
        if not text:
            text = "(no output)"

        stdout_trimmed, stdout_truncated = self._truncate(stdout, MAX_STDIO_FIELD_CHARS)
        stderr_trimmed, stderr_truncated = self._truncate(stderr, MAX_STDIO_FIELD_CHARS)
        truncated = text_truncated or stdout_truncated or stderr_truncated
        success = not error and not timed_out and (exit_code == 0)

        error_text: str | None = None
        if timed_out:
            error_text = "command timed out"
        elif error:
            error_text = stderr_trimmed.strip() or "command execution failed"
        elif isinstance(exit_code, int) and exit_code != 0:
            error_text = f"command exited with code {exit_code}"

        return make_tool_result(
            kind=self.name,
            text=text,
            success=success,
            error=error_text,
            data={
                "stdout": stdout_trimmed,
                "stderr": stderr_trimmed,
                "exit_code": exit_code,
            },
            meta={
                "timed_out": timed_out,
                "truncated": truncated,
                "duration_ms": duration_ms,
            },
        )

    def _run(self, command: str, timeout: int = 120) -> dict[str, Any]:
        """Execute *command* synchronously and return structured output."""
        start = time.monotonic()
        try:
            process = subprocess.Popen(
                command,
                shell=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                cwd=self.workspace,
                start_new_session=True,
            )
            try:
                stdout, stderr = process.communicate(timeout=timeout)
                exit_code = process.returncode
            except subprocess.TimeoutExpired:
                self._terminate_process_group(process.pid)
                stdout, stderr = process.communicate()
                duration_ms = int((time.monotonic() - start) * 1000)
                if not stdout and not stderr:
                    stderr = f"Error: command timed out after {timeout} seconds."
                return self._build_result(
                    stdout=stdout or "",
                    stderr=stderr or "",
                    exit_code=None,
                    duration_ms=duration_ms,
                    timed_out=True,
                    error=True,
                )
            duration_ms = int((time.monotonic() - start) * 1000)
            return self._build_result(
                stdout=stdout,
                stderr=stderr,
                exit_code=exit_code,
                duration_ms=duration_ms,
            )
        except Exception as exc:
            duration_ms = int((time.monotonic() - start) * 1000)
            return self._build_result(
                stdout="",
                stderr=f"Error executing command: {exc}",
                exit_code=None,
                duration_ms=duration_ms,
                error=True,
            )

    async def _arun(self, command: str, timeout: int = 120) -> dict[str, Any]:
        """Execute *command* asynchronously."""
        start = time.monotonic()
        try:
            process = await asyncio.create_subprocess_shell(
                command,
                cwd=self.workspace,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                start_new_session=True,
            )
            try:
                out_b, err_b = await asyncio.wait_for(process.communicate(), timeout=timeout)
            except asyncio.TimeoutError:
                self._terminate_process_group(process.pid)
                out_b, err_b = await process.communicate()
                duration_ms = int((time.monotonic() - start) * 1000)
                stdout = out_b.decode("utf-8", errors="replace")
                stderr = err_b.decode("utf-8", errors="replace")
                if not stdout and not stderr:
                    stderr = f"Error: command timed out after {timeout} seconds."
                return self._build_result(
                    stdout=stdout,
                    stderr=stderr,
                    exit_code=None,
                    duration_ms=duration_ms,
                    timed_out=True,
                    error=True,
                )

            duration_ms = int((time.monotonic() - start) * 1000)
            return self._build_result(
                stdout=out_b.decode("utf-8", errors="replace"),
                stderr=err_b.decode("utf-8", errors="replace"),
                exit_code=process.returncode,
                duration_ms=duration_ms,
            )
        except Exception as exc:
            duration_ms = int((time.monotonic() - start) * 1000)
            return self._build_result(
                stdout="",
                stderr=f"Error executing command: {exc}",
                exit_code=None,
                duration_ms=duration_ms,
                error=True,
            )
