"""Tests for the main module (AgentSession)."""

from __future__ import annotations

import asyncio
import json
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.agent import StreamEvent
from src.main import AgentSession
from src.prompts.tools import TOOL_DESCRIPTIONS


class TestAgentSession:
    def test_init(self):
        session = AgentSession("ws://localhost:3001/internal/ws", "token123")
        assert session.ws_url == "ws://localhost:3001/internal/ws"
        assert session.token == "token123"
        assert session.agent is None
        assert session._current_task is None
        assert session._shutdown is False

    async def test_handle_init_creates_agent(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent:
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
            })
            assert session.agent is not None

    async def test_handle_user_message_without_init(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()
        session.agent = None

        await session._handle_user_message({"content": "hello"})
        # Should send error
        session.ws.send.assert_called_once()
        sent = json.loads(session.ws.send.call_args[0][0])
        assert sent["type"] == "error"
        assert sent["code"] == "not_initialized"

    async def test_handle_user_message_empty_content(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()
        session.agent = MagicMock()

        await session._handle_user_message({"content": ""})
        # Should not start agent
        assert session._current_task is None

    async def test_handle_cancel(self):
        session = AgentSession("ws://test", "tok")
        session.agent = MagicMock()
        mock_task = MagicMock()
        mock_task.done.return_value = False
        session._current_task = mock_task

        session._handle_cancel()
        session.agent.cancel.assert_called_once()
        mock_task.cancel.assert_called_once()

    async def test_handle_cancel_no_agent(self):
        session = AgentSession("ws://test", "tok")
        session._handle_cancel()  # Should not raise

    async def test_handle_message_invalid_json(self):
        session = AgentSession("ws://test", "tok")
        # Should not raise
        await session._handle_message("not json{{{")

    async def test_handle_message_unknown_type(self):
        session = AgentSession("ws://test", "tok")
        # Should not raise
        await session._handle_message(json.dumps({"type": "unknown_msg"}))

    async def test_handle_message_dispatches_init(self):
        session = AgentSession("ws://test", "tok")
        session._handle_init = AsyncMock()
        msg = {"type": "init", "conversation_id": "c1"}
        await session._handle_message(json.dumps(msg))
        session._handle_init.assert_called_once()

    async def test_handle_message_dispatches_user_message(self):
        session = AgentSession("ws://test", "tok")
        session._handle_user_message = AsyncMock()
        msg = {"type": "user_message", "content": "hi"}
        await session._handle_message(json.dumps(msg))
        session._handle_user_message.assert_called_once()

    async def test_handle_message_dispatches_cancel(self):
        session = AgentSession("ws://test", "tok")
        session._handle_cancel = MagicMock()
        msg = {"type": "cancel"}
        await session._handle_message(json.dumps(msg))
        session._handle_cancel.assert_called_once()

    async def test_handle_message_dispatches_truncate_history(self):
        session = AgentSession("ws://test", "tok")
        session._handle_truncate_history = MagicMock()
        msg = {"type": "truncate_history", "keep_turns": 2}
        await session._handle_message(json.dumps(msg))
        session._handle_truncate_history.assert_called_once()

    async def test_handle_truncate_history_calls_agent(self):
        session = AgentSession("ws://test", "tok")
        session.agent = MagicMock()
        session._handle_truncate_history({"keep_turns": 3})
        session.agent.truncate_history.assert_called_once_with(3)

    async def test_handle_truncate_history_no_agent(self):
        session = AgentSession("ws://test", "tok")
        session.agent = None
        # Should not raise
        session._handle_truncate_history({"keep_turns": 1})

    def test_shutdown(self):
        session = AgentSession("ws://test", "tok")
        session.agent = MagicMock()
        mock_task = MagicMock()
        mock_task.done.return_value = False
        session._current_task = mock_task

        session.shutdown()
        assert session._shutdown is True
        session.agent.cancel.assert_called_once()
        mock_task.cancel.assert_called_once()

    async def test_send_error(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()
        await session._send_error("test_code", "test message")
        sent = json.loads(session.ws.send.call_args[0][0])
        assert sent["type"] == "error"
        assert sent["code"] == "test_code"
        assert sent["message"] == "test message"

    async def test_handle_init_calls_assembler_when_tools_enabled(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent:
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                "tools_enabled": True,
            })
            # The config passed to ChatAgent should contain tool descriptions
            config = MockAgent.call_args[0][0]
            assert "Available Tools" in config.system_prompt

    async def test_handle_init_preserves_custom_prompt_with_tools(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent:
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                "system_prompt": "You are a pirate.",
                "tools_enabled": True,
            })
            config = MockAgent.call_args[0][0]
            assert "You are a pirate." in config.system_prompt
            assert "Available Tools" in config.system_prompt

    async def test_handle_init_no_assembler_when_tools_disabled(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent:
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                "tools_enabled": False,
            })
            config = MockAgent.call_args[0][0]
            assert "Available Tools" not in config.system_prompt

    async def test_handle_init_passes_image_config_to_tools(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent, \
             patch("src.main.create_all_tools") as mock_create_tools:
            mock_create_tools.return_value = []
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                "tools_enabled": True,
                "image_provider": "google",
                "image_model": "gemini-img",
                "image_api_key": "img-key",
                "image_endpoint_url": "https://img.example.com",
            })
            mock_create_tools.assert_called_once()
            kwargs = mock_create_tools.call_args[1]
            assert kwargs["image_provider"] == "google"
            assert kwargs["image_model"] == "gemini-img"
            assert kwargs["image_api_key"] == "img-key"
            assert kwargs["image_endpoint_url"] == "https://img.example.com"

    async def test_handle_init_no_image_config_passes_empty(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        with patch("src.main.ChatAgent") as MockAgent, \
             patch("src.main.create_all_tools") as mock_create_tools:
            mock_create_tools.return_value = []
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                "tools_enabled": True,
            })
            kwargs = mock_create_tools.call_args[1]
            assert kwargs["image_provider"] == ""
            assert kwargs["image_model"] == ""
            assert kwargs["image_api_key"] == ""
            assert kwargs["image_endpoint_url"] is None

    async def test_task_tool_works_when_init_omits_subagent_fields(self):
        session = AgentSession("ws://test", "tok")
        session.ws = AsyncMock()

        class _FakeParentAgent:
            def __init__(self, _config, tools=()):
                self.tools = list(tools)

        class _FakeExploreAgent:
            def __init__(self, _config, tools=()):
                self.tools = list(tools)

            async def handle_message(
                self,
                _prompt,
                deep_thinking: bool = False,
                thinking_budget: int | None = None,
            ):
                yield StreamEvent("assistant_delta", {"delta": "subagent done"})
                yield StreamEvent("complete", {"content": "subagent done"})

        with patch("src.main.ChatAgent", _FakeParentAgent), patch(
            "src.subagents.ChatAgent", _FakeExploreAgent
        ):
            await session._handle_init({
                "conversation_id": "conv-1",
                "provider": "openai",
                "model": "gpt-4o",
                "api_key": "key",
                # intentionally omit subagent_provider/subagent_model
                "tools_enabled": True,
            })

            task_tool = next(t for t in session.agent.tools if t.name == "task")
            result = await task_tool._arun(
                subagent_type="explore",
                description="investigate repo",
                prompt="check architecture docs",
            )
            assert result["success"] is True
            assert result["data"]["subagent_type"] == "explore"
            assert "subagent done" in result["text"]
