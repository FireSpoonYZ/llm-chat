"""LangChain agent setup with streaming support."""

from __future__ import annotations

import asyncio
import json
import logging
import os
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
from .prompts.presets import get_preset
from .providers import create_chat_model

MAX_ITERATIONS = 25


def _default_system_prompt() -> str:
    preset = get_preset("default")
    if preset is None:
        raise RuntimeError("Built-in 'default' preset is missing")
    return preset.content


class AgentConfig:
    """Configuration received from the backend init message."""

    def __init__(self, init_data: dict[str, Any]) -> None:
        self.conversation_id: str = init_data["conversation_id"]
        self.provider: str = init_data.get("provider", "openai")
        self.model: str = init_data.get("model", "gpt-4o")
        self.api_key: str = init_data.get("api_key", "")
        self.endpoint_url: str | None = init_data.get("endpoint_url")
        self.system_prompt: str = init_data.get("system_prompt") or _default_system_prompt()
        self.tools_enabled: bool = init_data.get("tools_enabled", True)
        self.mcp_servers: list[dict[str, Any]] = init_data.get("mcp_servers", [])
        self.history: list[dict[str, str]] = init_data.get("history", [])
        # Image generation model config (separate from chat model)
        self.image_provider: str = init_data.get("image_provider", "") or ""
        self.image_model: str = init_data.get("image_model", "") or ""
        self.image_api_key: str = init_data.get("image_api_key", "") or ""
        self.image_endpoint_url: str | None = init_data.get("image_endpoint_url")


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


def _build_multimodal_content(
    text: str, attachments: list[dict[str, Any]]
) -> str | list[dict[str, Any]]:
    """Build multimodal content from text and file attachments.

    If attachments contain image files, returns a list of content blocks
    (text + image_url). Otherwise returns the plain text string.
    """
    if not attachments:
        return text

    IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".gif", ".webp"}
    blocks: list[dict[str, Any]] = []
    if text:
        blocks.append({"type": "text", "text": text})

    for att in attachments:
        path = att.get("path", "")
        ext = os.path.splitext(path)[1].lower()
        if ext not in IMAGE_EXTENSIONS:
            continue
        data = att.get("data")
        if not data:
            continue
        mime = f"image/{ext.lstrip('.')}"
        if ext == ".jpg":
            mime = "image/jpeg"
        blocks.append({
            "type": "image_url",
            "image_url": {"url": f"data:{mime};base64,{data}"},
        })

    if len(blocks) <= 1 and not any(b["type"] == "image_url" for b in blocks):
        return text
    return blocks


class StreamEvent:
    """An event emitted during agent streaming."""

    def __init__(self, event_type: str, data: dict[str, Any]) -> None:
        self.type = event_type
        self.data = data

    def to_json(self) -> str:
        return json.dumps({"type": self.type, **self.data})


logger = logging.getLogger("claude-chat-agent")


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

    def truncate_history(self, keep_turns: int) -> None:
        """Truncate message history to keep only the first *keep_turns* user exchanges.

        The SystemMessage at index 0 is always preserved.  Messages are cut at
        the (keep_turns + 1)-th ``HumanMessage`` boundary so that all
        intermediate ``ToolMessage`` / ``AIMessage`` entries belonging to kept
        turns are retained.
        """
        human_count = 0
        truncate_idx = len(self.messages)
        for i, msg in enumerate(self.messages):
            if isinstance(msg, HumanMessage):
                human_count += 1
                if human_count > keep_turns:
                    truncate_idx = i
                    break
        self.messages = self.messages[:truncate_idx]

    async def handle_message(self, content: str | list, deep_thinking: bool = False) -> AsyncIterator[StreamEvent]:
        """Process a user message and yield streaming events.

        Args:
            content: Plain text string or multimodal content list
                     (text + image_url blocks).

        Yields StreamEvent objects for: assistant_delta, thinking_delta,
        tool_call, tool_result, complete, error.
        """
        self._cancelled = False
        self.messages.append(HumanMessage(content=content))

        try:
            async for event in self._agent_loop(deep_thinking):
                yield event
        except asyncio.CancelledError:
            yield StreamEvent("error", {"code": "cancelled", "message": "Generation cancelled"})
        except Exception as exc:
            yield StreamEvent("error", {"code": "agent_error", "message": str(exc)})

    def _get_thinking_llm(self) -> BaseChatModel:
        """Return an LLM with provider-specific thinking/reasoning params."""
        provider = self.config.provider.lower()
        if provider == "anthropic":
            return self.llm.bind(
                max_tokens=64000,
                thinking={"type": "enabled", "budget_tokens": 50000},
            )
        elif provider == "openai":
            return self.llm.bind(
                reasoning={"effort": "high", "summary": "auto"},
            )
        elif provider == "google":
            return self.llm.bind(
                thinking_budget=32768,
            )
        else:
            # Mistral and others: no thinking support, return as-is
            return self.llm

    async def _agent_loop(self, deep_thinking: bool = False) -> AsyncIterator[StreamEvent]:
        """Run the agent loop: call LLM, handle tool calls, repeat."""
        total_content = ""  # Accumulates across all iterations
        all_content_blocks: list[dict[str, Any]] = []  # Interleaved thinking/text/tool_call blocks
        llm = self._get_thinking_llm() if deep_thinking else self.llm
        if deep_thinking:
            logger.info("Deep thinking enabled (provider=%s), bound kwargs: %s",
                        self.config.provider, getattr(llm, 'kwargs', {}))

        iteration = 0
        while iteration < MAX_ITERATIONS:
            iteration += 1
            logger.info("Agent iteration %d", iteration)
            if self._cancelled:
                return

            iteration_content = ""  # Per-iteration content for LangChain message history
            tool_calls: list[dict[str, Any]] = []

            thinking_total = 0
            chunk_count = 0

            async for chunk in llm.astream(self.messages):
                if self._cancelled:
                    return

                if not isinstance(chunk, AIMessageChunk):
                    continue

                chunk_count += 1
                # Log first few chunks for debugging
                if deep_thinking and chunk_count <= 3:
                    logger.info("Chunk #%d content type=%s, content=%s",
                                chunk_count, type(chunk.content).__name__,
                                repr(chunk.content)[:300])

                # Stream text content
                if chunk.content:
                    if isinstance(chunk.content, str):
                        delta = chunk.content
                        if delta:
                            iteration_content += delta
                            total_content += delta
                            yield StreamEvent("assistant_delta", {"delta": delta})
                            if all_content_blocks and all_content_blocks[-1].get("type") == "text":
                                all_content_blocks[-1]["content"] += delta
                            else:
                                all_content_blocks.append({"type": "text", "content": delta})
                    elif isinstance(chunk.content, list):
                        for block in chunk.content:
                            if isinstance(block, dict):
                                if block.get("type") == "thinking":
                                    thinking_text = block.get("thinking", "")
                                    if thinking_text:
                                        thinking_total += len(thinking_text)
                                        yield StreamEvent("thinking_delta", {"delta": thinking_text})
                                        if all_content_blocks and all_content_blocks[-1].get("type") == "thinking":
                                            all_content_blocks[-1]["content"] += thinking_text
                                        else:
                                            all_content_blocks.append({"type": "thinking", "content": thinking_text})
                                elif block.get("type") == "reasoning":
                                    # OpenAI format — summary is a list of dicts
                                    summaries = block.get("summary") or []
                                    if isinstance(summaries, list):
                                        for s in summaries:
                                            if isinstance(s, dict):
                                                text = s.get("text", "")
                                                if text:
                                                    thinking_total += len(text)
                                                    yield StreamEvent("thinking_delta", {"delta": text})
                                                    if all_content_blocks and all_content_blocks[-1].get("type") == "thinking":
                                                        all_content_blocks[-1]["content"] += text
                                                    else:
                                                        all_content_blocks.append({"type": "thinking", "content": text})
                                    # Also check normalized reasoning field
                                    reasoning_text = block.get("reasoning", "")
                                    if reasoning_text:
                                        thinking_total += len(reasoning_text)
                                        yield StreamEvent("thinking_delta", {"delta": reasoning_text})
                                        if all_content_blocks and all_content_blocks[-1].get("type") == "thinking":
                                            all_content_blocks[-1]["content"] += reasoning_text
                                        else:
                                            all_content_blocks.append({"type": "thinking", "content": reasoning_text})
                                elif block.get("type") == "text":
                                    delta = block.get("text", "")
                                    if delta:
                                        iteration_content += delta
                                        total_content += delta
                                        yield StreamEvent("assistant_delta", {"delta": delta})
                                        if all_content_blocks and all_content_blocks[-1].get("type") == "text":
                                            all_content_blocks[-1]["content"] += delta
                                        else:
                                            all_content_blocks.append({"type": "text", "content": delta})
                            else:
                                delta = str(block)
                                if delta:
                                    iteration_content += delta
                                    total_content += delta
                                    yield StreamEvent("assistant_delta", {"delta": delta})
                                    if all_content_blocks and all_content_blocks[-1].get("type") == "text":
                                        all_content_blocks[-1]["content"] += delta
                                    else:
                                        all_content_blocks.append({"type": "text", "content": delta})

                # Accumulate tool calls
                if chunk.tool_call_chunks:
                    for tc_chunk in chunk.tool_call_chunks:
                        _accumulate_tool_call(tool_calls, tc_chunk)

            # Filter out ghost tool call entries (empty name from index gaps)
            if deep_thinking:
                logger.info("Thinking total chars: %d", thinking_total)
            tool_calls = [tc for tc in tool_calls if tc.get("name")]

            # If no tool calls, we're done
            if not tool_calls:
                self.messages.append(AIMessage(content=iteration_content))
                has_rich_blocks = any(
                    b.get("type") in ("tool_call", "thinking")
                    for b in all_content_blocks
                )
                yield StreamEvent("complete", {
                    "content": total_content,
                    "tool_calls": all_content_blocks if has_rich_blocks else None,
                })
                return

            # Build AI message with tool calls
            ai_msg = AIMessage(
                content=iteration_content,
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

                # For multimodal results, extract text-only display for frontend
                if isinstance(result, list):
                    display_result = " ".join(
                        b.get("text", "") for b in result if b.get("type") == "text"
                    )
                else:
                    display_result = result

                yield StreamEvent("tool_result", {
                    "tool_call_id": tc_id,
                    "result": display_result,
                    "is_error": is_error,
                })

                self.messages.append(
                    ToolMessage(content=result, tool_call_id=tc_id)
                )

                all_content_blocks.append({
                    "type": "tool_call",
                    "id": tc_id,
                    "name": tc_name,
                    "input": tc_args,
                    "result": display_result,
                    "isError": is_error,
                })

        # Exhausted MAX_ITERATIONS without a final response
        yield StreamEvent("error", {
            "code": "max_iterations",
            "message": f"Agent exceeded maximum of {MAX_ITERATIONS} iterations",
        })

    async def _execute_tool(self, name: str, args: dict[str, Any]) -> tuple[str | list, bool]:
        """Execute a tool by name and return (result, is_error).

        Result may be a string or a list of content blocks (multimodal).
        """
        tool = next((t for t in self.tools if t.name == name), None)
        if tool is None:
            return f"Unknown tool: {name}", True

        try:
            result = await tool.ainvoke(args)
            if isinstance(result, list):
                return result, False
            return str(result), False
        except Exception as exc:
            return f"Tool error: {exc}", True


def _accumulate_tool_call(
    tool_calls: list[dict[str, Any]],
    chunk: Any,
) -> None:
    """Accumulate streaming tool call chunks into complete tool calls."""

    def _get(obj: Any, key: str, default: Any = None) -> Any:
        """Get attribute or dict key."""
        if isinstance(obj, dict):
            return obj.get(key, default)
        return getattr(obj, key, default)

    idx = _get(chunk, "index")
    if idx is None:
        idx = 0

    # Extend list if needed
    while len(tool_calls) <= idx:
        tool_calls.append({"id": "", "name": "", "args_str": ""})

    tc = tool_calls[idx]

    chunk_id = _get(chunk, "id")
    if chunk_id:
        tc["id"] = chunk_id
    chunk_name = _get(chunk, "name")
    if chunk_name:
        tc["name"] = chunk_name
    chunk_args = _get(chunk, "args")
    if chunk_args:
        tc["args_str"] += chunk_args

    # Try to parse accumulated args
    if tc["args_str"]:
        try:
            tc["args"] = json.loads(tc["args_str"])
        except json.JSONDecodeError:
            pass
