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
    build_message_history,
)
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

    def test_null_system_prompt_uses_default(self):
        config = AgentConfig({"conversation_id": "c", "system_prompt": None})
        assert "helpful AI assistant" in config.system_prompt

    def test_empty_system_prompt_uses_default(self):
        config = AgentConfig({"conversation_id": "c", "system_prompt": ""})
        assert "helpful AI assistant" in config.system_prompt

    def test_missing_conversation_id_raises(self):
        with pytest.raises(KeyError):
            AgentConfig({})


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
