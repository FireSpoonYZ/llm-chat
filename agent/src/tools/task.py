from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any, Protocol, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field, PrivateAttr

from .result_schema import make_tool_error


class TaskRunner(Protocol):
    async def run_task(
        self,
        *,
        subagent_type: str,
        description: str,
        prompt: str,
        event_sink: Callable[[Any], Awaitable[None]] | None = None,
    ) -> dict[str, Any]:
        ...


class TaskInput(BaseModel):
    subagent_type: str = Field(
        description=(
            "Subagent type to run. Currently supported: 'explore' "
            "(read-only codebase exploration)."
        )
    )
    description: str = Field(
        description="A short 3-5 word summary of what the subagent should do."
    )
    prompt: str = Field(
        description=(
            "Detailed task instructions and context for the subagent, "
            "including scope, constraints, and expected output."
        )
    )


class TaskTool(BaseTool):
    name: str = "task"
    description: str = (
        "Delegate broad or deep codebase exploration to a specialized "
        "read-only subagent and return its report. Prefer direct read/glob/grep "
        "for simple, targeted lookups."
    )
    args_schema: Type[BaseModel] = TaskInput
    runner: Any = None
    _event_sink: Callable[[Any], Awaitable[None]] | None = PrivateAttr(default=None)

    def set_event_sink(self, sink: Callable[[Any], Awaitable[None]] | None) -> None:
        """Attach a transient callback for subagent trace streaming."""
        self._event_sink = sink

    def _run(self, subagent_type: str, description: str, prompt: str) -> dict[str, Any]:
        return make_tool_error(
            kind=self.name,
            error="task tool is async-only",
            text="Error: task tool requires async execution.",
        )

    async def _arun(
        self,
        subagent_type: str,
        description: str,
        prompt: str,
    ) -> dict[str, Any]:
        if self.runner is None:
            return make_tool_error(kind=self.name, error="task runner is not configured")
        return await self.runner.run_task(
            subagent_type=subagent_type,
            description=description,
            prompt=prompt,
            event_sink=self._event_sink,
        )
