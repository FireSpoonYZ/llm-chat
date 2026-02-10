"""Tests for the main module (AgentSession)."""

from __future__ import annotations

import asyncio
import json
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.main import AgentSession


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
