"""LangChain agent setup with streaming support."""

from __future__ import annotations

import asyncio
import json
import uuid
from typing import Any, AsyncIterator, Sequence

from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.messages import (
    AIMessage,
    AIMessageChunk,
    BaseMessage,
    HumanMessage,
    SystemMessage,
    ToolMessage,
)
from langchain_core.tools import BaseTool

from .prompts.assembler import assemble_system_prompt
from .providers import create_chat_model


DEFAULT_SYSTEM_PROMPT = (
    "You are a helpful AI assistant. You have access to tools that let you "
    "interact with the user's workspace, run code, search the web, and more. "
    "Use tools when they would help accomplish the user's request."
)


class AgentConfig:
    """Configuration received from the backend init message."""

    def __init__(self, init_data: dict[str, Any]) -> None:
        self.conversation_id: str = init_data["conversation_id"]
        self.provider: str = init_data.get("provider", "openai")
        self.model: str = init_data.get("model", "gpt-4o")
        self.api_key: str = init_data.get("api_key", "")
        self.endpoint_url: str | None = init_data.get("endpoint_url")
        self.system_prompt: str = init_data.get("system_prompt") or DEFAULT_SYSTEM_PROMPT
        self.tools_enabled: bool = init_data.get("tools_enabled", True)
        self.mcp_servers: list[dict[str, Any]] = init_data.get("mcp_servers", [])
        self.history: list[dict[str, str]] = init_data.get("history", [])


def build_message_history(history: list[dict[str, str]]) -> list[BaseMessage]:
    """Convert raw history dicts to LangChain message objects."""
    messages: list[BaseMessage] = []
    for entry in history:
        role = entry.get("role", "")
        content = entry.get("content", "")
        if role == "user":
            messages.append(HumanMessage(content=content))
        elif role == "assistant":
            messages.append(AIMessage(content=content))
        elif role == "system":
            messages.append(SystemMessage(content=content))
    return messages


class StreamEvent:
    """An event emitted during agent streaming."""

    def __init__(self, event_type: str, data: dict[str, Any]) -> None:
        self.type = event_type
        self.data = data

    def to_json(self) -> str:
        return json.dumps({"type": self.type, **self.data})


class ChatAgent:
    """Manages a LangChain chat model with optional tool calling."""

    def __init__(self, config: AgentConfig, tools: Sequence[BaseTool] = ()) -> None:
        self.config = config
        self.tools = list(tools)
        self.llm: BaseChatModel = create_chat_model(
            provider=config.provider,
            model=config.model,
            api_key=config.api_key,
            endpoint_url=config.endpoint_url,
            streaming=True,
        )
        if self.tools:
            self.llm = self.llm.bind_tools(self.tools)
        self.messages: list[BaseMessage] = [
            SystemMessage(content=config.system_prompt),
            *build_message_history(config.history),
        ]
        self._cancelled = False

    def cancel(self) -> None:
        """Signal cancellation of the current generation."""
        self._cancelled = True

    async def handle_message(self, content: str) -> AsyncIterator[StreamEvent]:
        """Process a user message and yield streaming events.

        Yields StreamEvent objects for: assistant_delta, tool_call,
        tool_result, complete, error.
        """
        self._cancelled = False
        self.messages.append(HumanMessage(content=content))

        try:
            async for event in self._agent_loop():
                yield event
        except asyncio.CancelledError:
            yield StreamEvent("error", {"code": "cancelled", "message": "Generation cancelled"})
        except Exception as exc:
            yield StreamEvent("error", {"code": "agent_error", "message": str(exc)})

    async def _agent_loop(self) -> AsyncIterator[StreamEvent]:
        """Run the agent loop: call LLM, handle tool calls, repeat."""
        max_iterations = 20

        for _ in range(max_iterations):
            if self._cancelled:
                return

            full_content = ""
            tool_calls: list[dict[str, Any]] = []

            async for chunk in self.llm.astream(self.messages):
                if self._cancelled:
                    return

                if not isinstance(chunk, AIMessageChunk):
                    continue

                # Stream text content
                if chunk.content:
                    if isinstance(chunk.content, str):
                        delta = chunk.content
                    elif isinstance(chunk.content, list):
                        delta = "".join(
                            block.get("text", "") if isinstance(block, dict) else str(block)
                            for block in chunk.content
                        )
                    else:
                        delta = ""
                    if delta:
                        full_content += delta
                        yield StreamEvent("assistant_delta", {"delta": delta})

                # Accumulate tool calls
                if chunk.tool_call_chunks:
                    for tc_chunk in chunk.tool_call_chunks:
                        _accumulate_tool_call(tool_calls, tc_chunk)

            # If no tool calls, we're done
            if not tool_calls:
                self.messages.append(AIMessage(content=full_content))
                yield StreamEvent("complete", {
                    "content": full_content,
                    "token_usage": {"prompt": 0, "completion": 0},
                })
                return

            # Build AI message with tool calls
            ai_msg = AIMessage(
                content=full_content,
                tool_calls=[
                    {
                        "id": tc.get("id", str(uuid.uuid4())),
                        "name": tc["name"],
                        "args": tc.get("args", {}),
                    }
                    for tc in tool_calls
                ],
            )
            self.messages.append(ai_msg)

            # Execute tool calls
            for tc in tool_calls:
                if self._cancelled:
                    return

                tc_id = tc.get("id", str(uuid.uuid4()))
                tc_name = tc["name"]
                tc_args = tc.get("args", {})

                yield StreamEvent("tool_call", {
                    "tool_call_id": tc_id,
                    "tool_name": tc_name,
                    "tool_input": tc_args,
                })

                result, is_error = await self._execute_tool(tc_name, tc_args)

                yield StreamEvent("tool_result", {
                    "tool_call_id": tc_id,
                    "result": result,
                    "is_error": is_error,
                })

                self.messages.append(
                    ToolMessage(content=result, tool_call_id=tc_id)
                )

        # Max iterations reached
        yield StreamEvent("error", {
            "code": "max_iterations",
            "message": "Agent reached maximum iteration limit",
        })

    async def _execute_tool(self, name: str, args: dict[str, Any]) -> tuple[str, bool]:
        """Execute a tool by name and return (result, is_error)."""
        tool = next((t for t in self.tools if t.name == name), None)
        if tool is None:
            return f"Unknown tool: {name}", True

        try:
            result = await tool.ainvoke(args)
            return str(result), False
        except Exception as exc:
            return f"Tool error: {exc}", True


def _accumulate_tool_call(
    tool_calls: list[dict[str, Any]],
    chunk: Any,
) -> None:
    """Accumulate streaming tool call chunks into complete tool calls."""
    idx = chunk.index if hasattr(chunk, "index") and chunk.index is not None else 0

    # Extend list if needed
    while len(tool_calls) <= idx:
        tool_calls.append({"id": "", "name": "", "args_str": ""})

    tc = tool_calls[idx]

    if hasattr(chunk, "id") and chunk.id:
        tc["id"] = chunk.id
    if hasattr(chunk, "name") and chunk.name:
        tc["name"] = chunk.name
    if hasattr(chunk, "args") and chunk.args:
        tc["args_str"] += chunk.args

    # Try to parse accumulated args
    if tc["args_str"]:
        try:
            tc["args"] = json.loads(tc["args_str"])
        except json.JSONDecodeError:
            pass
