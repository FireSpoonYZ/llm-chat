"""Tests for the agent module."""

from __future__ import annotations

import asyncio
import json
from types import SimpleNamespace
from typing import Any, AsyncIterator
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.agent import (
    AgentConfig,
    ChatAgent,
    StreamEvent,
    _accumulate_tool_call,
    _build_multimodal_content,
    build_message_history,
)
from src.prompts.presets import BUILTIN_PRESETS
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage


class TestAgentConfig:
    def test_minimal_config(self):
        config = AgentConfig({"conversation_id": "conv-1"})
        assert config.conversation_id == "conv-1"
        assert config.provider == "openai"
        assert config.model == "gpt-4o"
        assert config.api_key == ""
        assert config.endpoint_url is None
        assert config.tools_enabled is True
        assert config.mcp_servers == []
        assert config.history == []
        assert config.image_provider == ""
        assert config.image_model == ""
        assert config.image_api_key == ""
        assert config.image_endpoint_url is None

    def test_full_config(self):
        data = {
            "conversation_id": "conv-2",
            "provider": "anthropic",
            "model": "claude-sonnet-4-20250514",
            "api_key": "sk-ant-test",
            "endpoint_url": "https://custom.com",
            "system_prompt": "You are a pirate.",
            "tools_enabled": False,
            "mcp_servers": [{"name": "test-mcp"}],
            "history": [{"role": "user", "content": "hello"}],
            "image_provider": "google",
            "image_model": "gemini-3-pro-image-preview",
            "image_api_key": "goog-key",
            "image_endpoint_url": "https://img.custom.com",
        }
        config = AgentConfig(data)
        assert config.provider == "anthropic"
        assert config.model == "claude-sonnet-4-20250514"
        assert config.api_key == "sk-ant-test"
        assert config.endpoint_url == "https://custom.com"
        assert config.system_prompt == "You are a pirate."
        assert config.tools_enabled is False
        assert len(config.mcp_servers) == 1
        assert len(config.history) == 1
        assert config.image_provider == "google"
        assert config.image_model == "gemini-3-pro-image-preview"
        assert config.image_api_key == "goog-key"
        assert config.image_endpoint_url == "https://img.custom.com"

    def test_null_system_prompt_uses_default(self):
        config = AgentConfig({"conversation_id": "c", "system_prompt": None})
        assert config.system_prompt == BUILTIN_PRESETS["default"].content

    def test_empty_system_prompt_uses_default(self):
        config = AgentConfig({"conversation_id": "c", "system_prompt": ""})
        assert config.system_prompt == BUILTIN_PRESETS["default"].content

    def test_missing_conversation_id_raises(self):
        with pytest.raises(KeyError):
            AgentConfig({})

    def test_null_image_fields_coerced_to_empty(self):
        config = AgentConfig({
            "conversation_id": "c",
            "image_provider": None,
            "image_model": None,
            "image_api_key": None,
            "image_endpoint_url": None,
        })
        assert config.image_provider == ""
        assert config.image_model == ""
        assert config.image_api_key == ""
        assert config.image_endpoint_url is None


class TestBuildMessageHistory:
    def test_empty_history(self):
        assert build_message_history([]) == []

    def test_user_message(self):
        msgs = build_message_history([{"role": "user", "content": "hi"}])
        assert len(msgs) == 1
        assert isinstance(msgs[0], HumanMessage)
        assert msgs[0].content == "hi"

    def test_assistant_message(self):
        msgs = build_message_history([{"role": "assistant", "content": "hello"}])
        assert len(msgs) == 1
        assert isinstance(msgs[0], AIMessage)

    def test_system_message(self):
        msgs = build_message_history([{"role": "system", "content": "be nice"}])
        assert len(msgs) == 1
        assert isinstance(msgs[0], SystemMessage)

    def test_unknown_role_skipped(self):
        msgs = build_message_history([{"role": "tool", "content": "result"}])
        assert len(msgs) == 0

    def test_mixed_history(self):
        history = [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
            {"role": "user", "content": "how are you"},
        ]
        msgs = build_message_history(history)
        assert len(msgs) == 3
        assert isinstance(msgs[0], HumanMessage)
        assert isinstance(msgs[1], AIMessage)
        assert isinstance(msgs[2], HumanMessage)


class TestBuildMultimodalContent:
    def test_no_attachments_returns_string(self):
        result = _build_multimodal_content("hello", [])
        assert result == "hello"

    def test_non_image_attachments_returns_string(self):
        result = _build_multimodal_content("hello", [{"path": "doc.pdf", "data": "abc"}])
        assert result == "hello"

    def test_image_attachment_returns_list(self):
        result = _build_multimodal_content("describe this", [
            {"path": "photo.png", "data": "iVBORw0KGgo="},
        ])
        assert isinstance(result, list)
        assert len(result) == 2
        assert result[0] == {"type": "text", "text": "describe this"}
        assert result[1]["type"] == "image_url"
        assert "data:image/png;base64," in result[1]["image_url"]["url"]

    def test_jpg_mime_type(self):
        result = _build_multimodal_content("test", [
            {"path": "photo.jpg", "data": "abc123"},
        ])
        assert isinstance(result, list)
        assert "data:image/jpeg;base64," in result[1]["image_url"]["url"]

    def test_multiple_images(self):
        result = _build_multimodal_content("compare", [
            {"path": "a.png", "data": "data1"},
            {"path": "b.jpeg", "data": "data2"},
        ])
        assert isinstance(result, list)
        assert len(result) == 3  # text + 2 images

    def test_mixed_attachments_only_images(self):
        result = _build_multimodal_content("test", [
            {"path": "doc.pdf", "data": "pdf_data"},
            {"path": "img.png", "data": "img_data"},
        ])
        assert isinstance(result, list)
        assert len(result) == 2  # text + 1 image (pdf skipped)

    def test_missing_data_skipped(self):
        result = _build_multimodal_content("test", [
            {"path": "img.png"},  # no data
        ])
        assert result == "test"


class TestStreamEvent:
    def test_to_json(self):
        event = StreamEvent("assistant_delta", {"delta": "hello"})
        parsed = json.loads(event.to_json())
        assert parsed["type"] == "assistant_delta"
        assert parsed["delta"] == "hello"

    def test_complete_event(self):
        event = StreamEvent("complete", {
            "content": "full response",
            "token_usage": {"prompt": 10, "completion": 20},
        })
        parsed = json.loads(event.to_json())
        assert parsed["type"] == "complete"
        assert parsed["content"] == "full response"
        assert parsed["token_usage"]["prompt"] == 10

    def test_error_event(self):
        event = StreamEvent("error", {"code": "test", "message": "fail"})
        parsed = json.loads(event.to_json())
        assert parsed["type"] == "error"
        assert parsed["code"] == "test"


class TestAccumulateToolCall:
    def test_single_chunk(self):
        tool_calls: list[dict[str, Any]] = []
        chunk = SimpleNamespace(index=0, id="tc-1", name="bash", args='{"cmd": "ls"}')
        _accumulate_tool_call(tool_calls, chunk)
        assert len(tool_calls) == 1
        assert tool_calls[0]["id"] == "tc-1"
        assert tool_calls[0]["name"] == "bash"
        assert tool_calls[0]["args"] == {"cmd": "ls"}

    def test_streaming_args(self):
        tool_calls: list[dict[str, Any]] = []
        chunk1 = SimpleNamespace(index=0, id="tc-1", name="bash", args='{"cm')
        _accumulate_tool_call(tool_calls, chunk1)
        assert "args" not in tool_calls[0]  # Not yet parseable

        chunk2 = SimpleNamespace(index=0, id="", name="", args='d": "ls"}')
        _accumulate_tool_call(tool_calls, chunk2)
        assert tool_calls[0]["args"] == {"cmd": "ls"}

    def test_multiple_tool_calls(self):
        tool_calls: list[dict[str, Any]] = []
        chunk0 = SimpleNamespace(index=0, id="tc-0", name="bash", args='{"a": 1}')
        chunk1 = SimpleNamespace(index=1, id="tc-1", name="read", args='{"b": 2}')
        _accumulate_tool_call(tool_calls, chunk0)
        _accumulate_tool_call(tool_calls, chunk1)
        assert len(tool_calls) == 2
        assert tool_calls[0]["name"] == "bash"
        assert tool_calls[1]["name"] == "read"

    def test_none_index_defaults_to_zero(self):
        tool_calls: list[dict[str, Any]] = []
        chunk = SimpleNamespace(index=None, id="tc-1", name="bash", args='{}')
        _accumulate_tool_call(tool_calls, chunk)
        assert len(tool_calls) == 1


class TestChatAgent:
    def _make_config(self, **overrides) -> AgentConfig:
        data = {
            "conversation_id": "test-conv",
            "provider": "openai",
            "model": "gpt-4o",
            "api_key": "test-key",
            **overrides,
        }
        return AgentConfig(data)

    @patch("src.agent.create_chat_model")
    async def test_creates_llm_on_init(self, mock_create):
        mock_llm = MagicMock()
        mock_create.return_value = mock_llm
        config = self._make_config()
        agent = ChatAgent(config)
        mock_create.assert_called_once_with(
            provider="openai",
            model="gpt-4o",
            api_key="test-key",
            endpoint_url=None,
            streaming=True,
        )

    @patch("src.agent.create_chat_model")
    async def test_system_prompt_in_messages(self, mock_create):
        mock_create.return_value = MagicMock()
        config = self._make_config(system_prompt="Be helpful")
        agent = ChatAgent(config)
        assert len(agent.messages) == 1
        assert isinstance(agent.messages[0], SystemMessage)
        assert agent.messages[0].content == "Be helpful"

    @patch("src.agent.create_chat_model")
    async def test_history_loaded(self, mock_create):
        mock_create.return_value = MagicMock()
        config = self._make_config(
            history=[
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello"},
            ]
        )
        agent = ChatAgent(config)
        # system + 2 history messages
        assert len(agent.messages) == 3

    @patch("src.agent.create_chat_model")
    async def test_cancel_sets_flag(self, mock_create):
        mock_create.return_value = MagicMock()
        agent = ChatAgent(self._make_config())
        assert agent._cancelled is False
        agent.cancel()
        assert agent._cancelled is True

    @patch("src.agent.create_chat_model")
    async def test_handle_message_adds_user_message(self, mock_create):
        """After handle_message, the user message should be in history."""
        from langchain_core.messages import AIMessageChunk

        mock_llm = AsyncMock()
        # Simulate a simple text response (no tool calls)
        async def fake_astream(messages):
            yield AIMessageChunk(content="Hello!", tool_call_chunks=[])
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(self._make_config())
        events = []
        async for event in agent.handle_message("test"):
            events.append(event)

        # Should have delta + complete events
        types = [e.type for e in events]
        assert "assistant_delta" in types
        assert "complete" in types

        # Messages should include system + user + assistant
        assert len(agent.messages) == 3
        assert isinstance(agent.messages[1], HumanMessage)
        assert agent.messages[1].content == "test"

    @patch("src.agent.create_chat_model")
    async def test_handle_message_error_yields_error_event(self, mock_create):
        mock_llm = AsyncMock()
        async def fake_astream(messages):
            raise RuntimeError("LLM error")
            yield  # make it a generator
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(self._make_config())
        events = []
        async for event in agent.handle_message("test"):
            events.append(event)

        assert any(e.type == "error" for e in events)
        error_event = next(e for e in events if e.type == "error")
        assert "LLM error" in error_event.data["message"]

    @patch("src.agent.create_chat_model")
    async def test_multi_iteration_content_accumulation(self, mock_create):
        """Content from iteration 1 (before tool call) should be included in complete event."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                # Iteration 1: text + tool call
                yield AIMessageChunk(content="Sure, ", tool_call_chunks=[])
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(name="test_tool", args='{"cmd": "echo hi"}', id="tc-1", index=0),
                    ],
                )
            else:
                # Iteration 2: final text response after tool execution
                yield AIMessageChunk(content="Done!", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        mock_tool = MagicMock()
        mock_tool.name = "test_tool"
        mock_tool.ainvoke = AsyncMock(return_value="hello")

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        events = []
        async for event in agent.handle_message("run echo"):
            events.append(event)

        complete_event = next(e for e in events if e.type == "complete")
        # total_content should include text from BOTH iterations
        assert complete_event.data["content"] == "Sure, Done!"
        # tool_calls should be Format A content blocks (interleaved)
        blocks = complete_event.data["tool_calls"]
        assert blocks is not None
        tool_call_blocks = [b for b in blocks if b.get("type") == "tool_call"]
        assert len(tool_call_blocks) == 1
        assert tool_call_blocks[0]["name"] == "test_tool"
        # Should also have text blocks
        text_blocks = [b for b in blocks if b.get("type") == "text"]
        assert len(text_blocks) >= 1

    @patch("src.agent.create_chat_model")
    async def test_complete_event_no_tool_calls_when_none(self, mock_create):
        """When no tool calls happen, tool_calls in complete should be None."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(content="Hello!", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(self._make_config())
        events = []
        async for event in agent.handle_message("hi"):
            events.append(event)

        complete_event = next(e for e in events if e.type == "complete")
        assert complete_event.data["tool_calls"] is None

    @patch("src.agent.create_chat_model")
    async def test_execute_tool_unknown(self, mock_create):
        mock_create.return_value = MagicMock()
        agent = ChatAgent(self._make_config())
        result, is_error = await agent._execute_tool("nonexistent", {})
        assert is_error is True
        assert "Unknown tool" in result

    @patch("src.agent.create_chat_model")
    async def test_execute_tool_success(self, mock_create):
        mock_create.return_value = MagicMock()
        mock_tool = MagicMock()
        mock_tool.name = "test_tool"
        mock_tool.ainvoke = AsyncMock(return_value="tool output")

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("test_tool", {"arg": "val"})
        assert is_error is False
        assert result == "tool output"
        mock_tool.ainvoke.assert_called_once_with({"arg": "val"})

    @patch("src.agent.create_chat_model")
    async def test_execute_tool_error(self, mock_create):
        mock_create.return_value = MagicMock()
        mock_tool = MagicMock()
        mock_tool.name = "bad_tool"
        mock_tool.ainvoke = AsyncMock(side_effect=RuntimeError("tool broke"))

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("bad_tool", {})
        assert is_error is True
        assert "tool broke" in result

    @patch("src.agent.create_chat_model")
    async def test_truncate_history_keeps_system_and_turns(self, mock_create):
        mock_create.return_value = MagicMock()
        config = self._make_config(history=[
            {"role": "user", "content": "u1"},
            {"role": "assistant", "content": "a1"},
            {"role": "user", "content": "u2"},
            {"role": "assistant", "content": "a2"},
            {"role": "user", "content": "u3"},
            {"role": "assistant", "content": "a3"},
        ])
        agent = ChatAgent(config)
        assert len(agent.messages) == 7  # system + 6

        agent.truncate_history(2)
        # system + u1 + a1 + u2 + a2
        assert len(agent.messages) == 5
        assert isinstance(agent.messages[0], SystemMessage)
        assert agent.messages[1].content == "u1"
        assert agent.messages[4].content == "a2"

    @patch("src.agent.create_chat_model")
    async def test_truncate_history_zero_keeps_only_system(self, mock_create):
        mock_create.return_value = MagicMock()
        config = self._make_config(history=[
            {"role": "user", "content": "u1"},
            {"role": "assistant", "content": "a1"},
        ])
        agent = ChatAgent(config)
        agent.truncate_history(0)
        assert len(agent.messages) == 1
        assert isinstance(agent.messages[0], SystemMessage)

    @patch("src.agent.create_chat_model")
    async def test_truncate_history_preserves_tool_messages(self, mock_create):
        """ToolMessages between AI messages should be preserved for kept turns."""
        from langchain_core.messages import ToolMessage
        mock_create.return_value = MagicMock()
        config = self._make_config()
        agent = ChatAgent(config)
        # Manually build a history with tool messages
        agent.messages = [
            agent.messages[0],  # SystemMessage
            HumanMessage(content="u1"),
            AIMessage(content="", tool_calls=[{"id": "tc1", "name": "t", "args": {}}]),
            ToolMessage(content="result", tool_call_id="tc1"),
            AIMessage(content="a1"),
            HumanMessage(content="u2"),
            AIMessage(content="a2"),
        ]
        agent.truncate_history(1)
        # Should keep: System + u1 + AI(tool_call) + ToolMessage + AI(a1)
        assert len(agent.messages) == 5
        assert isinstance(agent.messages[3], ToolMessage)
        assert agent.messages[4].content == "a1"

    @patch("src.agent.create_chat_model")
    async def test_execute_tool_multimodal_result(self, mock_create):
        """When a tool returns a list (multimodal), _execute_tool should pass it through."""
        mock_create.return_value = MagicMock()
        multimodal_result = [
            {"type": "text", "text": "Image file: photo.png"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc123"}},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "read"
        mock_tool.ainvoke = AsyncMock(return_value=multimodal_result)

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("read", {"file_path": "photo.png"})
        assert is_error is False
        assert isinstance(result, list)
        assert result == multimodal_result

    @patch("src.agent.create_chat_model")
    async def test_tool_result_event_display_string(self, mock_create):
        """tool_result event sent to frontend should not contain base64 data."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(name="read", args='{"file_path": "img.png"}', id="tc-1", index=0),
                    ],
                )
            else:
                yield AIMessageChunk(content="I see a cat.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        multimodal_result = [
            {"type": "text", "text": "Image file: img.png"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "read"
        mock_tool.ainvoke = AsyncMock(return_value=multimodal_result)

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        events = []
        async for event in agent.handle_message("describe img.png"):
            events.append(event)

        tool_result_events = [e for e in events if e.type == "tool_result"]
        assert len(tool_result_events) == 1
        tr = tool_result_events[0]
        # Frontend display result should be the text portion only, no base64
        assert "base64" not in tr.data["result"]
        assert "img.png" in tr.data["result"]

        # The complete event's content blocks should also not contain base64
        complete_event = next(e for e in events if e.type == "complete")
        blocks = complete_event.data["tool_calls"]
        tool_block = next(b for b in blocks if b.get("type") == "tool_call")
        assert "base64" not in tool_block["result"]

    @patch("src.agent.create_chat_model")
    async def test_tool_message_contains_full_multimodal(self, mock_create):
        """ToolMessage appended to agent.messages should contain the full list content."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk, ToolMessage

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(name="read", args='{"file_path": "x.png"}', id="tc-1", index=0),
                    ],
                )
            else:
                yield AIMessageChunk(content="Got it.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        multimodal_result = [
            {"type": "text", "text": "Image file: x.png"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "read"
        mock_tool.ainvoke = AsyncMock(return_value=multimodal_result)

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        events = []
        async for event in agent.handle_message("read x.png"):
            events.append(event)

        # Find the ToolMessage in agent.messages
        tool_msgs = [m for m in agent.messages if isinstance(m, ToolMessage)]
        assert len(tool_msgs) == 1
        # ToolMessage.content should be the full multimodal list (not stringified)
        assert isinstance(tool_msgs[0].content, list)
        assert tool_msgs[0].content == multimodal_result

    @patch("src.agent.create_chat_model")
    async def test_mixed_tool_calls_string_and_multimodal(self, mock_create):
        """When multiple tools run, string and multimodal results should both work."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(name="bash", args='{"command": "ls"}', id="tc-1", index=0),
                        ToolCallChunk(name="read", args='{"file_path": "img.png"}', id="tc-2", index=1),
                    ],
                )
            else:
                yield AIMessageChunk(content="Done.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        bash_tool = MagicMock()
        bash_tool.name = "bash"
        bash_tool.ainvoke = AsyncMock(return_value="file1.txt\nimg.png")

        read_tool = MagicMock()
        read_tool.name = "read"
        read_tool.ainvoke = AsyncMock(return_value=[
            {"type": "text", "text": "Image file: img.png"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}},
        ])

        agent = ChatAgent(self._make_config(), tools=[bash_tool, read_tool])
        events = []
        async for event in agent.handle_message("list and show"):
            events.append(event)

        tool_results = [e for e in events if e.type == "tool_result"]
        assert len(tool_results) == 2
        # First tool (bash) returns plain string
        assert isinstance(tool_results[0].data["result"], str)
        assert "file1.txt" in tool_results[0].data["result"]
        # Second tool (read) returns display text only
        assert "base64" not in tool_results[1].data["result"]
        assert "img.png" in tool_results[1].data["result"]

    @patch("src.agent.create_chat_model")
    async def test_display_result_joins_multiple_text_blocks(self, mock_create):
        """If multimodal result has multiple text blocks, display should join them."""
        mock_create.return_value = MagicMock()
        multi_text_result = [
            {"type": "text", "text": "Part one."},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,x"}},
            {"type": "text", "text": "Part two."},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "read"
        mock_tool.ainvoke = AsyncMock(return_value=multi_text_result)

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("read", {"file_path": "x.png"})
        assert is_error is False
        assert isinstance(result, list)
        assert len(result) == 3
