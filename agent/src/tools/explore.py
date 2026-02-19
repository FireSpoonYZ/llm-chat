from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any, Protocol, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field, PrivateAttr

from .result_schema import make_tool_error


class ExploreRunner(Protocol):
    async def run_subagent(
        self,
        *,
        subagent_type: str,
        result_kind: str | None = None,
        description: str,
        prompt: str,
        event_sink: Callable[[Any], Awaitable[None]] | None = None,
    ) -> dict[str, Any]:
        ...


class ExploreInput(BaseModel):
    description: str = Field(
        description="A short 3-5 word summary of what should be explored."
    )
    prompt: str = Field(
        description=(
            "Detailed exploration instructions and context, including scope, "
            "constraints, and expected output."
        )
    )


class ExploreTool(BaseTool):
    name: str = "explore"
    supports_runtime_events: bool = True
    supports_subagent_trace: bool = True
    description: str = (
        "Delegate broad or deep codebase exploration to a specialized "
        "read-only subagent and return its report. Prefer direct read/glob/grep "
        "for simple, targeted lookups."
    )
    args_schema: Type[BaseModel] = ExploreInput
    runner: Any = None
    _event_sink: Callable[[Any], Awaitable[None]] | None = PrivateAttr(default=None)

    def set_event_sink(self, sink: Callable[[Any], Awaitable[None]] | None) -> None:
        """Attach a transient callback for subagent trace streaming."""
        self._event_sink = sink

    def _run(self, description: str, prompt: str) -> dict[str, Any]:
        return make_tool_error(
            kind=self.name,
            error="explore tool is async-only",
            text="Error: explore tool requires async execution.",
        )

    async def _arun(
        self,
        description: str,
        prompt: str,
    ) -> dict[str, Any]:
        if self.runner is None:
            return make_tool_error(kind=self.name, error="explore runner is not configured")

        run_subagent = getattr(self.runner, "run_subagent", None)
        if callable(run_subagent):
            return await run_subagent(
                subagent_type="explore",
                result_kind="explore",
                description=description,
                prompt=prompt,
                event_sink=self._event_sink,
            )

        # Fallback for older runner stubs that still expose run_task.
        run_task = getattr(self.runner, "run_task", None)
        if callable(run_task):
            return await run_task(
                subagent_type="explore",
                description=description,
                prompt=prompt,
                event_sink=self._event_sink,
            )

        return make_tool_error(
            kind=self.name,
            error="explore runner does not implement run_subagent",
        )
