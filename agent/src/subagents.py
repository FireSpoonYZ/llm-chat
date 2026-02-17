"""Subagent runtime for task-based delegation."""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from typing import Any, Sequence

from langchain_core.tools import BaseTool

from .agent import AgentConfig, ChatAgent, StreamEvent
from .prompts.assembler import assemble_system_prompt
from .tools.capabilities import tool_is_read_only
from .tools.result_schema import make_tool_error, make_tool_success

EXPLORE_SUBAGENT_PROMPT = """\
You are a file search specialist for this codebase. You excel at thoroughly
navigating and exploring repositories.

CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS
This is a read-only exploration task.

You are strictly prohibited from:
- Creating new files.
- Modifying existing files.
- Deleting files.
- Moving or copying files.
- Creating temporary files anywhere (including /tmp).
- Running commands or tools that change system state.

Your role is exclusively to search and analyze existing code.
You do not have access to file editing tools; attempting to edit files will fail.

Your strengths:
- Rapidly finding files using glob patterns.
- Searching code and text with regex patterns.
- Reading and analyzing file contents.

Guidelines:
- Use glob for broad file pattern matching.
- Use grep for searching file contents with regex.
- Use read when you know the specific file path to inspect.
- Adapt your search approach based on the thoroughness requested by the caller.
- Return file paths as absolute paths in your final response.
- Communicate your final report directly as a regular message.
- For clear communication, avoid emojis.
- You are a fast agent: return useful output as quickly as possible.

To achieve this:
- Use tools efficiently and choose the shortest path to relevant evidence.
- Prefer multiple parallel tool calls when lookups are independent.
- Report findings clearly, and call out uncertainty when something cannot be confirmed.
"""


class SubagentRunner:
    """Runs specialized subagents from the task tool."""

    def __init__(
        self,
        *,
        parent_config: AgentConfig,
        base_tools: Sequence[BaseTool],
        mcp_servers: list[dict[str, Any]] | None = None,
    ) -> None:
        self.parent_config = parent_config
        self.base_tools = list(base_tools)
        self.mcp_servers = list(mcp_servers or [])
        self._depth = 0
        self._depth_lock = asyncio.Lock()

    async def run_task(
        self,
        *,
        subagent_type: str,
        description: str,
        prompt: str,
        event_sink: Callable[[StreamEvent], Awaitable[None]] | None = None,
    ) -> dict[str, Any]:
        subagent_type = (subagent_type or "").strip().lower()
        if subagent_type != "explore":
            return make_tool_error(
                kind="task",
                error=f"unsupported subagent_type: {subagent_type}",
                text="Error: only subagent_type='explore' is supported.",
            )

        if not self.parent_config.subagent_provider or not self.parent_config.subagent_model:
            return make_tool_error(
                kind="task",
                error="subagent model is not configured for this conversation",
                text="Error: subagent provider/model is not configured for this conversation.",
            )

        async with self._depth_lock:
            if self._depth > 0:
                return make_tool_error(
                    kind="task",
                    error="nested subagent invocation is disabled",
                    text="Error: subagents cannot invoke subagents.",
                )
            self._depth += 1

        try:
            return await self._run_explore_subagent(
                description=description,
                prompt=prompt,
                event_sink=event_sink,
            )
        finally:
            async with self._depth_lock:
                self._depth = max(0, self._depth - 1)

    async def _run_explore_subagent(
        self,
        *,
        description: str,
        prompt: str,
        event_sink: Callable[[StreamEvent], Awaitable[None]] | None = None,
    ) -> dict[str, Any]:
        read_only_tools = [
            tool for tool in self.base_tools
            if tool.name != "task" and tool_is_read_only(tool)
        ]
        if not read_only_tools:
            return make_tool_error(
                kind="task",
                error="no read-only tools are available for explore subagent",
                text="Error: no read-only tools available for explore subagent.",
            )

        system_prompt = assemble_system_prompt(
            [t.name for t in read_only_tools],
            mcp_servers=self.mcp_servers or None,
            base_prompt=EXPLORE_SUBAGENT_PROMPT,
        )
        subagent_config = AgentConfig({
            "conversation_id": f"{self.parent_config.conversation_id}:explore",
            "provider": self.parent_config.subagent_provider,
            "model": self.parent_config.subagent_model,
            "api_key": self.parent_config.subagent_api_key,
            "endpoint_url": self.parent_config.subagent_endpoint_url,
            "system_prompt": system_prompt,
            "tools_enabled": True,
            "history": [],
            "mcp_servers": self.mcp_servers,
        })

        # Parent model should pass gathered context to subagent as part of prompt.
        task_prompt = (
            f"Task summary:\n{description}\n\n"
            f"Detailed prompt and context:\n{prompt}"
        )

        subagent = ChatAgent(subagent_config, tools=read_only_tools)
        trace, final_content, error_msg = await self._collect_trace(
            subagent,
            task_prompt,
            deep_thinking=self.parent_config.deep_thinking,
            thinking_budget=self.parent_config.subagent_thinking_budget,
            event_sink=event_sink,
        )
        if error_msg:
            return make_tool_error(
                kind="task",
                error=error_msg,
                text=f"Error: {error_msg}",
                data={
                    "subagent_type": "explore",
                    "description": description,
                    "trace": trace,
                },
            )

        text = final_content.strip() if final_content else "(no output)"
        return make_tool_success(
            kind="task",
            text=text,
            data={
                "subagent_type": "explore",
                "description": description,
                "summary": text,
                "trace": trace,
            },
            meta={
                "trace_blocks": len(trace),
                "read_only_tools": [t.name for t in read_only_tools],
            },
        )

    async def _collect_trace(
        self,
        subagent: ChatAgent,
        prompt: str,
        *,
        deep_thinking: bool,
        thinking_budget: int | None,
        event_sink: Callable[[StreamEvent], Awaitable[None]] | None = None,
    ) -> tuple[list[dict[str, Any]], str, str | None]:
        trace: list[dict[str, Any]] = []
        final_content = ""
        error_msg: str | None = None

        async for event in subagent.handle_message(
            prompt,
            deep_thinking=deep_thinking,
            thinking_budget=thinking_budget,
        ):
            self._append_trace_event(trace, event)
            if event_sink is not None:
                await event_sink(event)
            if event.type == "complete":
                final_content = str(event.data.get("content") or "")
            elif event.type == "error":
                error_msg = str(event.data.get("message") or "subagent execution failed")

        return trace, final_content, error_msg

    def _append_trace_event(self, trace: list[dict[str, Any]], event: StreamEvent) -> None:
        if event.type == "assistant_delta":
            delta = str(event.data.get("delta") or "")
            if not delta:
                return
            if trace and trace[-1].get("type") == "text":
                trace[-1]["content"] = str(trace[-1].get("content", "")) + delta
            else:
                trace.append({"type": "text", "content": delta})
            return

        if event.type == "thinking_delta":
            delta = str(event.data.get("delta") or "")
            if not delta:
                return
            if trace and trace[-1].get("type") == "thinking":
                trace[-1]["content"] = str(trace[-1].get("content", "")) + delta
            else:
                trace.append({"type": "thinking", "content": delta})
            return

        if event.type == "tool_call":
            trace.append({
                "type": "tool_call",
                "id": event.data.get("tool_call_id"),
                "name": event.data.get("tool_name"),
                "input": event.data.get("tool_input"),
                "result": None,
                "isError": False,
            })
            return

        if event.type == "tool_result":
            tc_id = event.data.get("tool_call_id")
            for item in trace:
                if item.get("type") == "tool_call" and item.get("id") == tc_id:
                    item["result"] = event.data.get("result")
                    item["isError"] = bool(event.data.get("is_error"))
                    return
