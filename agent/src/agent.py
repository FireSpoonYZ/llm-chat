"""LangChain agent setup with streaming support."""

from __future__ import annotations

import asyncio
import json
import logging
import os
import uuid
from unittest.mock import Mock
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
from .provider_contracts import get_provider_contract
from .providers import create_chat_model
from .tools.result_schema import (
    extract_text_from_legacy_list,
    make_tool_error,
    make_tool_result,
    make_tool_success,
)


def _load_max_iterations() -> int:
    """Load max iterations from environment.

    0 or negative means unlimited iterations.
    """
    raw = (os.getenv("MAX_ITERATIONS", "0") or "0").strip()
    try:
        return int(raw)
    except ValueError:
        logging.getLogger("claude-chat-agent").warning(
            "Invalid MAX_ITERATIONS=%r, defaulting to 0 (unlimited)",
            raw,
        )
        return 0


MAX_ITERATIONS = _load_max_iterations()
DEFAULT_THINKING_BUDGET = 128000
MIN_THINKING_BUDGET = 1024
MAX_THINKING_BUDGET = 1_000_000


def _resolve_thinking_budget(thinking_budget: int | None) -> int:
    """Normalize budget to a safe integer range.

    The backend should validate this, but we keep a defensive guard because
    the agent may also be invoked directly in tests or local scripts.
    """
    if thinking_budget is None:
        return DEFAULT_THINKING_BUDGET

    try:
        budget = int(thinking_budget)
    except (TypeError, ValueError):
        logger.warning(
            "Invalid thinking_budget=%r; falling back to default=%d",
            thinking_budget,
            DEFAULT_THINKING_BUDGET,
        )
        return DEFAULT_THINKING_BUDGET

    if budget < MIN_THINKING_BUDGET:
        logger.warning(
            "thinking_budget=%d is below min=%d; clamping",
            budget,
            MIN_THINKING_BUDGET,
        )
        return MIN_THINKING_BUDGET
    if budget > MAX_THINKING_BUDGET:
        logger.warning(
            "thinking_budget=%d exceeds max=%d; clamping",
            budget,
            MAX_THINKING_BUDGET,
        )
        return MAX_THINKING_BUDGET
    return budget


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
        self.subagent_provider: str = (
            init_data.get("subagent_provider", "") or self.provider
        )
        self.subagent_model: str = (
            init_data.get("subagent_model", "") or self.model
        )
        self.subagent_api_key: str = (
            init_data.get("subagent_api_key", "") or self.api_key
        )
        self.subagent_endpoint_url: str | None = (
            init_data.get("subagent_endpoint_url") or self.endpoint_url
        )
        self.deep_thinking: bool = bool(init_data.get("deep_thinking", False))
        self.thinking_budget: int | None = init_data.get("thinking_budget")
        self.subagent_thinking_budget: int | None = init_data.get("subagent_thinking_budget")
        if self.subagent_thinking_budget is None:
            self.subagent_thinking_budget = self.thinking_budget
        self.system_prompt: str = init_data.get("system_prompt") or _default_system_prompt()
        self.tools_enabled: bool = init_data.get("tools_enabled", True)
        self.mcp_servers: list[dict[str, Any]] = init_data.get("mcp_servers", [])
        self.history: list[dict[str, Any]] = init_data.get("history", [])
        self.history_parts: list[dict[str, Any]] = init_data.get("history_parts", [])
        # Image generation model config (separate from chat model)
        self.image_provider: str = init_data.get("image_provider", "") or ""
        self.image_model: str = init_data.get("image_model", "") or ""
        self.image_api_key: str = init_data.get("image_api_key", "") or ""
        self.image_endpoint_url: str | None = init_data.get("image_endpoint_url")


def build_message_history(history: list[dict[str, Any]]) -> list[BaseMessage]:
    """Convert raw history dicts to LangChain message objects.

    For assistant messages with ``tool_calls`` (the ``all_content_blocks``
    array persisted in the DB), the blocks are reconstructed into the proper
    AIMessage / ToolMessage sequence that LLM APIs expect.
    """
    messages: list[BaseMessage] = []
    for entry in history:
        role = entry.get("role", "")
        content = entry.get("content", "")
        tool_calls = entry.get("tool_calls")
        if role == "user":
            messages.append(HumanMessage(content=content))
        elif role == "assistant":
            if tool_calls:
                if isinstance(tool_calls, list):
                    reconstructed = _reconstruct_assistant_messages(tool_calls)
                    if reconstructed:
                        messages.extend(reconstructed)
                    else:
                        messages.append(AIMessage(content=content))
                else:
                    messages.append(AIMessage(content=content))
            else:
                messages.append(AIMessage(content=content))
        elif role == "system":
            messages.append(SystemMessage(content=content))
    return messages


def build_message_history_from_parts(history_parts: list[dict[str, Any]]) -> list[BaseMessage]:
    """Convert structured history_parts entries to LangChain messages."""
    messages: list[BaseMessage] = []
    for entry in history_parts:
        role = str(entry.get("role", ""))
        parts = entry.get("parts")
        if not isinstance(parts, list):
            continue

        if role == "user":
            user_text = " ".join(
                str(p.get("text", "")).strip()
                for p in parts
                if isinstance(p, dict) and p.get("type") == "text"
            ).strip()
            messages.append(HumanMessage(content=user_text))
            continue

        if role != "assistant":
            continue

        indexed_parts = [
            (idx, p)
            for idx, p in enumerate(parts)
            if isinstance(p, dict)
        ]
        seq_values = [
            p.get("seq")
            for _, p in indexed_parts
            if isinstance(p.get("seq"), int)
        ]
        max_seq = max(seq_values, default=-1)

        def _sort_key(item: tuple[int, dict[str, Any]]) -> int:
            idx, part = item
            seq = part.get("seq")
            if isinstance(seq, int):
                return seq
            return max_seq + idx + 1

        ordered_parts = [p for _, p in sorted(indexed_parts, key=_sort_key)]

        pending_texts: list[str] = []
        pending_tool_calls: list[dict[str, Any]] = []
        pending_tool_results: list[ToolMessage] = []

        def _payload_obj(payload: Any) -> dict[str, Any]:
            if isinstance(payload, dict):
                return payload
            if isinstance(payload, str):
                try:
                    parsed = json.loads(payload)
                except json.JSONDecodeError:
                    return {}
                return parsed if isinstance(parsed, dict) else {}
            return {}

        def _tool_result_text(part: dict[str, Any]) -> str:
            result_text = str(part.get("text", ""))
            if result_text:
                return result_text
            payload = part.get("json_payload")
            if isinstance(payload, str):
                return payload
            if isinstance(payload, dict):
                return str(payload.get("text", ""))
            if isinstance(payload, list):
                return " ".join(
                    str(b.get("text", ""))
                    for b in payload
                    if isinstance(b, dict) and b.get("type") == "text"
                )
            return ""

        def _flush_tool_round() -> None:
            nonlocal pending_texts, pending_tool_calls, pending_tool_results
            if not pending_tool_calls:
                return
            messages.append(AIMessage(
                content=" ".join(pending_texts).strip(),
                tool_calls=pending_tool_calls,
            ))
            messages.extend(pending_tool_results)
            pending_texts = []
            pending_tool_calls = []
            pending_tool_results = []

        def _flush_text_only() -> None:
            nonlocal pending_texts
            content = " ".join(pending_texts).strip()
            if content:
                messages.append(AIMessage(content=content))
            pending_texts = []

        for part in ordered_parts:
            ptype = part.get("type")
            if ptype == "text":
                if pending_tool_calls:
                    _flush_tool_round()
                text = str(part.get("text", "")).strip()
                if text:
                    pending_texts.append(text)
                continue

            if ptype == "tool_call":
                payload_obj = _payload_obj(part.get("json_payload"))
                tc_id = str(part.get("tool_call_id") or payload_obj.get("id") or "")
                pending_tool_calls.append({
                    "id": tc_id,
                    "name": str(payload_obj.get("name", "")),
                    "args": payload_obj.get("input", {}),
                })
                continue

            if ptype == "tool_result":
                tc_id = str(part.get("tool_call_id") or "")
                result_text = _tool_result_text(part)
                tool_msg = ToolMessage(content=result_text, tool_call_id=tc_id)
                if pending_tool_calls:
                    pending_tool_results.append(tool_msg)
                else:
                    _flush_text_only()
                    messages.append(tool_msg)

        if pending_tool_calls:
            _flush_tool_round()
        else:
            _flush_text_only()

    return messages


def _reconstruct_assistant_messages(
    blocks: list[dict[str, Any]],
) -> list[BaseMessage]:
    """Rebuild AIMessage/ToolMessage sequence from all_content_blocks.

    Algorithm: walk blocks, accumulate text and tool_calls.  When a text
    block appears after pending tool_calls, flush the previous group first.
    """
    result: list[BaseMessage] = []
    pending_text = ""
    pending_tool_calls: list[dict[str, Any]] = []
    pending_tool_results: list[tuple[str, str]] = []  # (id, result)

    def _normalize_history_tool_result(raw_result: Any) -> str:
        """Convert persisted tool result payloads to ToolMessage text."""
        if isinstance(raw_result, dict):
            return str(raw_result.get("text", ""))
        if isinstance(raw_result, list):
            return " ".join(
                b.get("text", "")
                for b in raw_result
                if isinstance(b, dict) and b.get("type") == "text"
            )
        return str(raw_result)

    def _flush() -> None:
        nonlocal pending_text, pending_tool_calls, pending_tool_results
        if pending_tool_calls:
            result.append(AIMessage(
                content=pending_text,
                tool_calls=[
                    {"id": tc.get("id", ""), "name": tc.get("name", ""), "args": tc.get("input", {})}
                    for tc in pending_tool_calls
                ],
            ))
            for tc_id, tc_result in pending_tool_results:
                result.append(ToolMessage(content=tc_result, tool_call_id=tc_id))
            pending_text = ""
            pending_tool_calls = []
            pending_tool_results = []

    for block in blocks:
        if not isinstance(block, dict):
            continue
        btype = block.get("type", "")
        if btype == "thinking":
            continue
        elif btype == "text":
            text = block.get("content", "")
            if pending_tool_calls:
                _flush()
            pending_text = text
        elif btype == "tool_call":
            pending_tool_calls.append(block)
            pending_tool_results.append(
                (block.get("id", ""), _normalize_history_tool_result(block.get("result", "")))
            )

    # Flush remaining
    if pending_tool_calls:
        _flush()
    # If there's trailing text after the last flush (or no tool calls at all)
    if pending_text:
        result.append(AIMessage(content=pending_text))

    return result


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

_REPLACEMENT_CHAR = "\ufffd"
TASK_EVENT_QUEUE_MAXSIZE = 256


def _normalize_tool_result(tool_name: str, result: Any) -> dict[str, Any]:
    """Normalize tool output into the standard tool result envelope."""
    if isinstance(result, dict):
        if (
            isinstance(result.get("kind"), str)
            and isinstance(result.get("text"), str)
            and isinstance(result.get("success"), bool)
        ):
            normalized = dict(result)
            normalized.setdefault("error", None)
            normalized.setdefault("data", {})
            normalized.setdefault("meta", {})
            return normalized

        # Legacy bash payload support
        if result.get("kind") == "bash":
            data = result.get("data") if isinstance(result.get("data"), dict) else {}
            meta = result.get("meta") if isinstance(result.get("meta"), dict) else {}
            exit_code = data.get("exit_code", result.get("exit_code"))
            timed_out = bool(meta.get("timed_out", result.get("timed_out")))
            has_error = bool(result.get("error")) or timed_out
            if isinstance(exit_code, int) and exit_code != 0:
                has_error = True
            error = None if not has_error else "command execution failed"
            if timed_out:
                error = "command timed out"
            return make_tool_result(
                kind=tool_name,
                text=str(result.get("text", "")),
                success=not has_error,
                error=error,
                data={
                    "stdout": data.get("stdout", result.get("stdout", "")),
                    "stderr": data.get("stderr", result.get("stderr", "")),
                    "exit_code": exit_code,
                },
                meta={
                    "timed_out": timed_out,
                    "truncated": bool(meta.get("truncated", result.get("truncated"))),
                    "duration_ms": meta.get("duration_ms", result.get("duration_ms")),
                },
            )

        if isinstance(result.get("text"), str):
            success = bool(result.get("success", True))
            error = result.get("error")
            return make_tool_result(
                kind=tool_name,
                text=result["text"],
                success=success,
                error=str(error) if error else None,
                data=result.get("data") if isinstance(result.get("data"), dict) else {},
                meta=result.get("meta") if isinstance(result.get("meta"), dict) else {},
            )

    if isinstance(result, list):
        return make_tool_success(
            kind=tool_name,
            text=extract_text_from_legacy_list(result),
            llm_content=result,
        )

    return make_tool_success(kind=tool_name, text=str(result))


def _normalize_tool_result_for_display(tool_name: str, result: Any) -> dict[str, Any]:
    """Normalize tool output to frontend-safe structured result."""
    normalized = _normalize_tool_result(tool_name, result)
    return {k: v for k, v in normalized.items() if k != "llm_content"}


def _tool_message_content(result: Any) -> str | list:
    """Convert tool output into LangChain ToolMessage content."""
    if isinstance(result, dict):
        llm_content = result.get("llm_content")
        if isinstance(llm_content, list):
            return llm_content
        if isinstance(llm_content, str):
            return llm_content
        return str(result.get("text", ""))
    if isinstance(result, list):
        return result
    return str(result)


def sanitize_delta(text: str) -> str:
    """Strip U+FFFD replacement characters from streaming deltas."""
    if _REPLACEMENT_CHAR not in text:
        return text
    logger.warning("Stripped %d U+FFFD from delta: %r",
                   text.count(_REPLACEMENT_CHAR), text[:200])
    return text.replace(_REPLACEMENT_CHAR, "")


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
        self.provider_contract = get_provider_contract(config.provider)
        restored_history = (
            build_message_history_from_parts(config.history_parts)
            if config.history_parts
            else build_message_history(config.history)
        )
        self.messages: list[BaseMessage] = [
            SystemMessage(content=config.system_prompt),
            *restored_history,
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

    async def handle_message(self, content: str | list, deep_thinking: bool = False, thinking_budget: int | None = None) -> AsyncIterator[StreamEvent]:
        """Process a user message and yield streaming events.

        Args:
            content: Plain text string or multimodal content list
                     (text + image_url blocks).
            thinking_budget: Optional custom budget for thinking tokens.
                             None means use provider defaults.

        Yields StreamEvent objects for: assistant_delta, thinking_delta,
        tool_call, tool_result, complete, error.
        """
        self._cancelled = False
        self.messages.append(HumanMessage(content=content))

        try:
            async for event in self._agent_loop(deep_thinking, thinking_budget):
                yield event
        except asyncio.CancelledError:
            yield StreamEvent("error", {"code": "cancelled", "message": "Generation cancelled"})
        except Exception as exc:
            yield StreamEvent("error", {"code": "agent_error", "message": str(exc)})

    def _get_budgeted_llm(self, thinking_budget: int | None = None) -> BaseChatModel:
        """Return an LLM with provider-specific output token caps bound."""
        budget = _resolve_thinking_budget(thinking_budget)
        return self._bind_llm(**self.provider_contract.build_budget_kwargs(budget))

    def _bind_llm(self, **kwargs: Any) -> BaseChatModel:
        """Bind kwargs to LLM, with test-safe fallback for malformed mocks."""
        if not kwargs:
            return self.llm
        bind_fn = getattr(self.llm, "bind", None)
        if not callable(bind_fn):
            return self.llm
        bound = bind_fn(**kwargs)
        if asyncio.iscoroutine(bound):
            bound.close()
            return self.llm
        if isinstance(bound, Mock):
            return self.llm
        if not hasattr(bound, "astream"):
            return self.llm
        return bound

    def _get_thinking_llm(self, thinking_budget: int | None = None) -> BaseChatModel:
        """Return an LLM with both budget caps and deep-thinking parameters bound."""
        budget = _resolve_thinking_budget(thinking_budget)
        return self._bind_llm(**self.provider_contract.build_thinking_kwargs(budget))

    def _get_turn_llm(
        self,
        deep_thinking: bool,
        thinking_budget: int | None = None,
    ) -> BaseChatModel:
        if deep_thinking:
            return self._get_thinking_llm(thinking_budget)
        return self._get_budgeted_llm(thinking_budget)

    async def _agent_loop(self, deep_thinking: bool = False, thinking_budget: int | None = None) -> AsyncIterator[StreamEvent]:
        """Run the agent loop: call LLM, handle tool calls, repeat."""
        total_content = ""  # Accumulates across all iterations
        all_content_blocks: list[dict[str, Any]] = []  # Interleaved thinking/text/tool_call blocks
        effective_budget = thinking_budget if thinking_budget is not None else self.config.thinking_budget
        llm = self._get_turn_llm(deep_thinking, effective_budget)
        if deep_thinking:
            logger.info("Deep thinking enabled (provider=%s, thinking_budget=%s), bound kwargs: %s",
                        self.config.provider, effective_budget, getattr(llm, 'kwargs', {}))

        iteration = 0
        while MAX_ITERATIONS <= 0 or iteration < MAX_ITERATIONS:
            iteration += 1
            logger.info("Agent iteration %d", iteration)
            if self._cancelled:
                return

            iteration_content = ""  # Per-iteration content for LangChain message history
            tool_calls: list[dict[str, Any]] = []
            accumulated_chunk: AIMessageChunk | None = None

            thinking_total = 0
            chunk_count = 0

            async for chunk in llm.astream(self.messages):
                if self._cancelled:
                    return

                if not isinstance(chunk, AIMessageChunk):
                    continue

                accumulated_chunk = chunk if accumulated_chunk is None else accumulated_chunk + chunk

                chunk_count += 1
                # Log first few chunks for debugging
                if deep_thinking and chunk_count <= 3:
                    logger.info("Chunk #%d content type=%s, content=%s",
                                chunk_count, type(chunk.content).__name__,
                                repr(chunk.content)[:300])

                # Stream text content
                if chunk.content:
                    if isinstance(chunk.content, str):
                        delta = sanitize_delta(chunk.content)
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
                                for thinking_raw in self.provider_contract.extract_thinking_deltas(block):
                                    thinking_text = sanitize_delta(thinking_raw)
                                    if thinking_text:
                                        thinking_total += len(thinking_text)
                                        yield StreamEvent("thinking_delta", {"delta": thinking_text})
                                        if all_content_blocks and all_content_blocks[-1].get("type") == "thinking":
                                            all_content_blocks[-1]["content"] += thinking_text
                                        else:
                                            all_content_blocks.append({"type": "thinking", "content": thinking_text})
                                delta = sanitize_delta(self.provider_contract.extract_text_delta(block))
                                if delta:
                                    iteration_content += delta
                                    total_content += delta
                                    yield StreamEvent("assistant_delta", {"delta": delta})
                                    if all_content_blocks and all_content_blocks[-1].get("type") == "text":
                                        all_content_blocks[-1]["content"] += delta
                                    else:
                                        all_content_blocks.append({"type": "text", "content": delta})
                            else:
                                delta = sanitize_delta(str(block))
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
                # Use accumulated content to preserve thinking blocks with signatures
                if accumulated_chunk is not None and isinstance(accumulated_chunk.content, list):
                    final_content = [
                        block for block in accumulated_chunk.content
                        if not (isinstance(block, dict) and block.get("type") == "tool_use")
                    ]
                else:
                    final_content = iteration_content
                final_content = self.provider_contract.normalize_history_content(final_content)
                self.messages.append(AIMessage(content=final_content))
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
            # Use accumulated content to preserve thinking blocks with signatures
            if accumulated_chunk is not None and isinstance(accumulated_chunk.content, list):
                ai_content = [
                    block for block in accumulated_chunk.content
                    if not (isinstance(block, dict) and block.get("type") == "tool_use")
                ]
                if not ai_content:
                    ai_content = iteration_content
            else:
                ai_content = iteration_content
            ai_content = self.provider_contract.normalize_history_content(ai_content)
            ai_msg = AIMessage(
                content=ai_content,
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

                if tc_name == "task":
                    task_tool = next((t for t in self.tools if t.name == tc_name), None)
                    set_event_sink = getattr(task_tool, "set_event_sink", None)
                    if task_tool is not None and callable(set_event_sink):
                        task_event_queue: asyncio.Queue[StreamEvent] = asyncio.Queue(
                            maxsize=TASK_EVENT_QUEUE_MAXSIZE
                        )

                        async def _on_task_event(task_event: StreamEvent) -> None:
                            await task_event_queue.put(task_event)

                        set_event_sink(_on_task_event)
                        tool_task: asyncio.Task[Any] | None = None
                        pending_event_task: asyncio.Task[StreamEvent] | None = None
                        try:
                            tool_task = asyncio.create_task(task_tool.ainvoke(tc_args))
                            while tool_task is not None:
                                if self._cancelled and not tool_task.done():
                                    tool_task.cancel()

                                if pending_event_task is None:
                                    pending_event_task = asyncio.create_task(task_event_queue.get())

                                done, _ = await asyncio.wait(
                                    {tool_task, pending_event_task},
                                    return_when=asyncio.FIRST_COMPLETED,
                                )

                                if pending_event_task in done:
                                    task_event = pending_event_task.result()
                                    pending_event_task = None
                                    yield StreamEvent("task_trace_delta", {
                                        "tool_call_id": tc_id,
                                        "event_type": task_event.type,
                                        "payload": task_event.data,
                                    })

                                if tool_task.done():
                                    # Drain any buffered events emitted before task completion.
                                    while not task_event_queue.empty():
                                        task_event = await task_event_queue.get()
                                        yield StreamEvent("task_trace_delta", {
                                            "tool_call_id": tc_id,
                                            "event_type": task_event.type,
                                            "payload": task_event.data,
                                        })
                                    break

                            raw_result = await tool_task
                            result = _normalize_tool_result(tc_name, raw_result)
                            is_error = not bool(result.get("success", True))
                        except asyncio.CancelledError:
                            if tool_task is not None and not tool_task.done():
                                tool_task.cancel()
                            raise
                        except Exception as exc:
                            result = make_tool_error(
                                kind=tc_name,
                                error=f"Tool error: {exc}",
                            )
                            is_error = True
                        finally:
                            if pending_event_task is not None and not pending_event_task.done():
                                pending_event_task.cancel()
                            set_event_sink(None)
                    else:
                        result, is_error = await self._execute_tool(tc_name, tc_args)
                else:
                    result, is_error = await self._execute_tool(tc_name, tc_args)
                display_result = _normalize_tool_result_for_display(tc_name, result)

                yield StreamEvent("tool_result", {
                    "tool_call_id": tc_id,
                    "result": display_result,
                    "is_error": is_error,
                })

                self.messages.append(
                    ToolMessage(content=_tool_message_content(result), tool_call_id=tc_id)
                )

                all_content_blocks.append({
                    "type": "tool_call",
                    "id": tc_id,
                    "name": tc_name,
                    "input": tc_args,
                    "result": display_result,
                    "isError": is_error,
                })

        if MAX_ITERATIONS > 0:
            # Exhausted MAX_ITERATIONS without a final response
            yield StreamEvent("error", {
                "code": "max_iterations",
                "message": f"Agent exceeded maximum of {MAX_ITERATIONS} iterations",
            })

    async def _execute_tool(
        self, name: str, args: dict[str, Any]
    ) -> tuple[dict[str, Any], bool]:
        """Execute a tool by name and return (result, is_error).

        Result is always a normalized structured envelope.
        """
        tool = next((t for t in self.tools if t.name == name), None)
        if tool is None:
            result = make_tool_error(kind=name, error=f"Unknown tool: {name}")
            return result, True

        try:
            result = await tool.ainvoke(args)
            normalized = _normalize_tool_result(name, result)
            return normalized, not bool(normalized.get("success", True))
        except Exception as exc:
            error_result = make_tool_error(kind=name, error=f"Tool error: {exc}")
            return error_result, True


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
