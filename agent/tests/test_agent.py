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
    build_message_history_from_parts,
    build_message_history,
    sanitize_delta,
)
from src.prompts.presets import BUILTIN_PRESETS
from src.tools.explore import ExploreTool
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, ToolMessage


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

    def test_build_message_history_from_parts_text(self):
        msgs = build_message_history_from_parts([
            {
                "role": "user",
                "parts": [{"type": "text", "text": "hi from parts"}],
            },
            {
                "role": "assistant",
                "parts": [{"type": "text", "text": "hello from parts"}],
            },
        ])
        assert len(msgs) == 2
        assert isinstance(msgs[0], HumanMessage)
        assert msgs[0].content == "hi from parts"
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "hello from parts"

    def test_build_message_history_from_parts_tool_roundtrip(self):
        msgs = build_message_history_from_parts([
            {
                "role": "assistant",
                "parts": [
                    {"type": "text", "text": "Running command"},
                    {
                        "type": "tool_call",
                        "tool_call_id": "tc-1",
                        "json_payload": {
                            "type": "tool_call",
                            "id": "tc-1",
                            "name": "bash",
                            "input": {"command": "ls"},
                        },
                    },
                    {
                        "type": "tool_result",
                        "tool_call_id": "tc-1",
                        "text": "file1\nfile2",
                    },
                ],
            },
        ])
        assert len(msgs) == 2
        assert isinstance(msgs[0], AIMessage)
        assert msgs[0].content == "Running command"
        assert len(msgs[0].tool_calls) == 1
        assert msgs[0].tool_calls[0]["id"] == "tc-1"
        assert msgs[0].tool_calls[0]["name"] == "bash"
        assert isinstance(msgs[1], ToolMessage)
        assert msgs[1].tool_call_id == "tc-1"
        assert msgs[1].content == "file1\nfile2"

    def test_build_message_history_from_parts_tool_result_json_payload_fallback(self):
        msgs = build_message_history_from_parts([
            {
                "role": "assistant",
                "parts": [
                    {
                        "type": "tool_call",
                        "tool_call_id": "tc-2",
                        "json_payload": {
                            "type": "tool_call",
                            "id": "tc-2",
                            "name": "bash",
                            "input": {"command": "pwd"},
                        },
                    },
                    {
                        "type": "tool_result",
                        "tool_call_id": "tc-2",
                        "text": "",
                        "json_payload": {"kind": "bash", "text": "/workspace"},
                    },
                ],
            },
        ])
        assert len(msgs) == 2
        assert isinstance(msgs[0], AIMessage)
        assert len(msgs[0].tool_calls) == 1
        assert isinstance(msgs[1], ToolMessage)
        assert msgs[1].tool_call_id == "tc-2"
        assert msgs[1].content == "/workspace"

    def test_build_message_history_from_parts_multi_iteration_order_preserved(self):
        msgs = build_message_history_from_parts([
            {
                "role": "assistant",
                "parts": [
                    {"seq": 0, "type": "text", "text": "Step 1"},
                    {
                        "seq": 1,
                        "type": "tool_call",
                        "tool_call_id": "tc-1",
                        "json_payload": {
                            "type": "tool_call",
                            "id": "tc-1",
                            "name": "bash",
                            "input": {"command": "ls"},
                        },
                    },
                    {
                        "seq": 2,
                        "type": "tool_result",
                        "tool_call_id": "tc-1",
                        "text": "file-a\nfile-b",
                    },
                    {"seq": 3, "type": "text", "text": "Step 2"},
                    {
                        "seq": 4,
                        "type": "tool_call",
                        "tool_call_id": "tc-2",
                        "json_payload": {
                            "type": "tool_call",
                            "id": "tc-2",
                            "name": "bash",
                            "input": {"command": "pwd"},
                        },
                    },
                    {
                        "seq": 5,
                        "type": "tool_result",
                        "tool_call_id": "tc-2",
                        "text": "/workspace",
                    },
                    {"seq": 6, "type": "text", "text": "Done"},
                ],
            },
        ])

        # AI(tool tc-1) + Tool(tc-1) + AI(tool tc-2) + Tool(tc-2) + AI(final)
        assert len(msgs) == 5
        assert isinstance(msgs[0], AIMessage)
        assert msgs[0].content == "Step 1"
        assert len(msgs[0].tool_calls) == 1
        assert msgs[0].tool_calls[0]["id"] == "tc-1"

        assert isinstance(msgs[1], ToolMessage)
        assert msgs[1].tool_call_id == "tc-1"
        assert msgs[1].content == "file-a\nfile-b"

        assert isinstance(msgs[2], AIMessage)
        assert msgs[2].content == "Step 2"
        assert len(msgs[2].tool_calls) == 1
        assert msgs[2].tool_calls[0]["id"] == "tc-2"

        assert isinstance(msgs[3], ToolMessage)
        assert msgs[3].tool_call_id == "tc-2"
        assert msgs[3].content == "/workspace"

        assert isinstance(msgs[4], AIMessage)
        assert msgs[4].content == "Done"
        assert msgs[4].tool_calls == []

    def test_build_message_history_from_parts_uses_seq_order(self):
        msgs = build_message_history_from_parts([
            {
                "role": "assistant",
                "parts": [
                    {"seq": 3, "type": "text", "text": "Done"},
                    {
                        "seq": 1,
                        "type": "tool_call",
                        "tool_call_id": "tc-1",
                        "json_payload": {
                            "type": "tool_call",
                            "id": "tc-1",
                            "name": "bash",
                            "input": {"command": "ls"},
                        },
                    },
                    {"seq": 0, "type": "text", "text": "Step 1"},
                    {"seq": 2, "type": "tool_result", "tool_call_id": "tc-1", "text": "ok"},
                ],
            },
        ])

        assert len(msgs) == 3
        assert isinstance(msgs[0], AIMessage)
        assert msgs[0].content == "Step 1"
        assert len(msgs[0].tool_calls) == 1
        assert msgs[0].tool_calls[0]["id"] == "tc-1"
        assert isinstance(msgs[1], ToolMessage)
        assert msgs[1].content == "ok"
        assert isinstance(msgs[2], AIMessage)
        assert msgs[2].content == "Done"

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

    def test_assistant_no_tool_calls_unchanged(self):
        """Assistant message without tool_calls should produce a single AIMessage."""
        history = [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello there"},
        ]
        msgs = build_message_history(history)
        assert len(msgs) == 2
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "hello there"
        assert msgs[1].tool_calls == []

    def test_single_tool_call_reconstruction(self):
        """Assistant with one tool_call should produce AIMessage + ToolMessage + AIMessage."""
        history = [
            {"role": "user", "content": "list files"},
            {
                "role": "assistant",
                "content": "Let me check.",
                "tool_calls": [
                    {"type": "text", "content": "Let me check."},
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "ls"},
                        "result": "file1.txt\nfile2.txt",
                        "isError": False,
                    },
                    {"type": "text", "content": "Here are the files."},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert isinstance(msgs[0], HumanMessage)
        # AIMessage with tool_calls
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "Let me check."
        assert len(msgs[1].tool_calls) == 1
        assert msgs[1].tool_calls[0]["name"] == "bash"
        assert msgs[1].tool_calls[0]["id"] == "tc1"
        # ToolMessage
        assert isinstance(msgs[2], ToolMessage)
        assert msgs[2].content == "file1.txt\nfile2.txt"
        assert msgs[2].tool_call_id == "tc1"
        # Final AIMessage
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "Here are the files."

    def test_structured_tool_result_reconstruction_uses_text(self):
        """Structured result objects should be converted to ToolMessage text."""
        history = [
            {"role": "user", "content": "run command"},
            {
                "role": "assistant",
                "content": "Done",
                "tool_calls": [
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "echo hi"},
                        "result": {"kind": "bash", "text": "hi\n", "exit_code": 0},
                        "isError": False,
                    },
                    {"type": "text", "content": "Done"},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert isinstance(msgs[1], AIMessage)
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        assert msgs[2].content == "hi\n"
        assert msgs[2].tool_call_id == "tc1"
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "Done"

    def test_multiple_tool_calls_same_iteration(self):
        """Multiple tool_calls before any text should all be in one AIMessage."""
        history = [
            {"role": "user", "content": "do stuff"},
            {
                "role": "assistant",
                "content": "Done",
                "tool_calls": [
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "ls"},
                        "result": "files",
                        "isError": False,
                    },
                    {
                        "type": "tool_call",
                        "id": "tc2",
                        "name": "bash",
                        "input": {"command": "pwd"},
                        "result": "/home",
                        "isError": False,
                    },
                    {"type": "text", "content": "Done"},
                ],
            },
        ]
        msgs = build_message_history(history)
        # AIMessage(tool_calls=[tc1, tc2]) + ToolMessage(tc1) + ToolMessage(tc2) + AIMessage("Done")
        assert isinstance(msgs[1], AIMessage)
        assert len(msgs[1].tool_calls) == 2
        assert isinstance(msgs[2], ToolMessage)
        assert msgs[2].tool_call_id == "tc1"
        assert isinstance(msgs[3], ToolMessage)
        assert msgs[3].tool_call_id == "tc2"
        assert isinstance(msgs[4], AIMessage)
        assert msgs[4].content == "Done"

    def test_multi_iteration_tool_calls(self):
        """Multiple rounds of tool calls (text → tool → text → tool → text)."""
        history = [
            {"role": "user", "content": "complex task"},
            {
                "role": "assistant",
                "content": "final",
                "tool_calls": [
                    {"type": "text", "content": "Step 1"},
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "step1"},
                        "result": "result1",
                        "isError": False,
                    },
                    {"type": "text", "content": "Step 2"},
                    {
                        "type": "tool_call",
                        "id": "tc2",
                        "name": "bash",
                        "input": {"command": "step2"},
                        "result": "result2",
                        "isError": False,
                    },
                    {"type": "text", "content": "final"},
                ],
            },
        ]
        msgs = build_message_history(history)
        # HumanMessage
        # AIMessage("Step 1", tool_calls=[tc1])
        # ToolMessage(tc1)
        # AIMessage("Step 2", tool_calls=[tc2])
        # ToolMessage(tc2)
        # AIMessage("final")
        assert len(msgs) == 6
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "Step 1"
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "Step 2"
        assert len(msgs[3].tool_calls) == 1
        assert isinstance(msgs[4], ToolMessage)
        assert isinstance(msgs[5], AIMessage)
        assert msgs[5].content == "final"
        assert msgs[5].tool_calls == []

    def test_thinking_blocks_skipped(self):
        """Thinking blocks in tool_calls should be ignored."""
        history = [
            {"role": "user", "content": "think and act"},
            {
                "role": "assistant",
                "content": "done",
                "tool_calls": [
                    {"type": "thinking", "content": "Let me think about this..."},
                    {"type": "text", "content": "I'll run a command."},
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "echo hi"},
                        "result": "hi",
                        "isError": False,
                    },
                    {"type": "text", "content": "done"},
                ],
            },
        ]
        msgs = build_message_history(history)
        # No thinking messages should appear
        for m in msgs:
            if isinstance(m, AIMessage):
                assert "think" not in m.content.lower() or m.content == "done"

        # Should still have: Human + AI(tool_calls) + Tool + AI
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "I'll run a command."
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "done"

    def test_no_text_before_tool_call(self):
        """Tool call with no preceding text should produce AIMessage with empty content."""
        history = [
            {"role": "user", "content": "go"},
            {
                "role": "assistant",
                "content": "result",
                "tool_calls": [
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "ls"},
                        "result": "files",
                        "isError": False,
                    },
                    {"type": "text", "content": "result"},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == ""
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "result"

    def test_tool_call_with_error(self):
        """Tool call with isError=True should still produce a ToolMessage."""
        history = [
            {"role": "user", "content": "run bad"},
            {
                "role": "assistant",
                "content": "failed",
                "tool_calls": [
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "bad_cmd"},
                        "result": "Tool error: command not found",
                        "isError": True,
                    },
                    {"type": "text", "content": "failed"},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert isinstance(msgs[1], AIMessage)
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        assert msgs[2].content == "Tool error: command not found"
        assert isinstance(msgs[3], AIMessage)
        assert msgs[3].content == "failed"

    def test_empty_tool_calls_list(self):
        """Empty tool_calls list should behave like no tool_calls."""
        history = [
            {"role": "assistant", "content": "just text", "tool_calls": []},
        ]
        msgs = build_message_history(history)
        assert len(msgs) == 1
        assert isinstance(msgs[0], AIMessage)
        assert msgs[0].content == "just text"
        assert msgs[0].tool_calls == []

    def test_only_tool_calls_no_final_text(self):
        """Tool calls with no trailing text block should still work."""
        history = [
            {"role": "user", "content": "go"},
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"type": "text", "content": "Running..."},
                    {
                        "type": "tool_call",
                        "id": "tc1",
                        "name": "bash",
                        "input": {"command": "ls"},
                        "result": "files",
                        "isError": False,
                    },
                ],
            },
        ]
        msgs = build_message_history(history)
        # AI("Running...", tool_calls=[tc1]) + ToolMessage
        assert isinstance(msgs[1], AIMessage)
        assert msgs[1].content == "Running..."
        assert len(msgs[1].tool_calls) == 1
        assert isinstance(msgs[2], ToolMessage)
        # No trailing AIMessage since there's no final text
        assert len(msgs) == 3

    def test_only_thinking_blocks_falls_back_to_content(self):
        """If tool_calls only has thinking blocks, fall back to content field."""
        history = [
            {
                "role": "assistant",
                "content": "Here is my answer.",
                "tool_calls": [
                    {"type": "thinking", "content": "Let me think deeply..."},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert len(msgs) == 1
        assert isinstance(msgs[0], AIMessage)
        assert msgs[0].content == "Here is my answer."

    def test_tool_call_missing_id_does_not_crash(self):
        """A tool_call block without 'id' should not raise KeyError."""
        history = [
            {"role": "user", "content": "go"},
            {
                "role": "assistant",
                "content": "done",
                "tool_calls": [
                    {
                        "type": "tool_call",
                        "name": "bash",
                        "input": {"command": "ls"},
                        "result": "files",
                        "isError": False,
                    },
                    {"type": "text", "content": "done"},
                ],
            },
        ]
        msgs = build_message_history(history)
        assert isinstance(msgs[1], AIMessage)
        assert len(msgs[1].tool_calls) == 1
        assert msgs[1].tool_calls[0]["id"] == ""
        assert isinstance(msgs[2], ToolMessage)
        assert msgs[2].tool_call_id == ""


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
        assert result["kind"] == "nonexistent"
        assert result["success"] is False
        assert "Unknown tool" in result["text"]

    @patch("src.agent.create_chat_model")
    async def test_execute_tool_success(self, mock_create):
        mock_create.return_value = MagicMock()
        mock_tool = MagicMock()
        mock_tool.name = "test_tool"
        mock_tool.ainvoke = AsyncMock(return_value="tool output")

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("test_tool", {"arg": "val"})
        assert is_error is False
        assert result["kind"] == "test_tool"
        assert result["success"] is True
        assert result["text"] == "tool output"
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
        assert result["kind"] == "bad_tool"
        assert result["success"] is False
        assert "tool broke" in result["text"]

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
        """When a tool returns a list, _execute_tool should normalize it with llm_content."""
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
        assert result["kind"] == "read"
        assert result["success"] is True
        assert isinstance(result["llm_content"], list)
        assert result["llm_content"] == multimodal_result

    @patch("src.agent.create_chat_model")
    async def test_structured_tool_result_keeps_llm_content(self, mock_create):
        """Structured dict results should preserve llm_content for ToolMessage context."""
        mock_create.return_value = MagicMock()
        llm_content = [
            {"type": "text", "text": "Generated image markdown"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc123"}},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "image_generation"
        mock_tool.ainvoke = AsyncMock(return_value={
            "kind": "image_generation",
            "text": "![Generated Image](sandbox:///generated_images/sample.png)",
            "success": True,
            "error": None,
            "data": {"media": [{"type": "image", "url": "sandbox:///generated_images/sample.png"}]},
            "meta": {"image_count": 1},
            "llm_content": llm_content,
        })

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        result, is_error = await agent._execute_tool("image_generation", {"prompt": "a cat"})
        assert is_error is False
        assert result["kind"] == "image_generation"
        assert result["success"] is True
        assert result["text"].startswith("![Generated Image]")
        assert result["llm_content"] == llm_content

    @patch("src.agent.create_chat_model")
    async def test_explore_tool_streams_subagent_trace_events(self, mock_create):
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(
                            name="explore",
                            args='{"description":"scan","prompt":"inspect"}',
                            id="tc-explore-1",
                            index=0,
                        ),
                    ],
                )
            else:
                yield AIMessageChunk(content="Task done.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        class _Runner:
            async def run_subagent(self, **kwargs):
                sink = kwargs.get("event_sink")
                if sink is not None:
                    await sink(StreamEvent("assistant_delta", {"delta": "Investigating"}))
                    await sink(StreamEvent("tool_call", {
                        "tool_call_id": "sub-tc-1",
                        "tool_name": "read",
                        "tool_input": {"file_path": "README.md"},
                    }))
                    await sink(StreamEvent("tool_result", {
                        "tool_call_id": "sub-tc-1",
                        "result": {"kind": "read", "text": "ok", "success": True, "error": None, "data": {}, "meta": {}},
                        "is_error": False,
                    }))
                    await sink(StreamEvent("complete", {"content": "Subagent summary"}))
                return {
                    "kind": "explore",
                    "text": "Subagent summary",
                    "success": True,
                    "error": None,
                    "data": {"trace": [{"type": "text", "content": "Investigating"}]},
                    "meta": {},
                }

        explore_tool = ExploreTool(runner=_Runner())
        agent = ChatAgent(self._make_config(), tools=[explore_tool])

        events = []
        async for event in agent.handle_message("run explore"):
            events.append(event)

        trace_events = [e for e in events if e.type == "subagent_trace_delta"]
        assert [e.data["event_type"] for e in trace_events] == [
            "assistant_delta",
            "tool_call",
            "tool_result",
            "complete",
        ]
        assert not any(e.type == "task_trace_delta" for e in events)
        assert trace_events[1].data["payload"]["tool_name"] == "read"

        tool_result_event = next(e for e in events if e.type == "tool_result")
        assert tool_result_event.data["result"]["kind"] == "explore"
        assert tool_result_event.data["is_error"] is False

    @patch("src.agent.create_chat_model")
    async def test_set_event_sink_alone_does_not_enable_subagent_trace(
        self, mock_create
    ):
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(_messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(
                            name="runtime_hook_tool",
                            args="{}",
                            id="tc-runtime-hook-1",
                            index=0,
                        ),
                    ],
                )
            else:
                yield AIMessageChunk(content="done", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        class _RuntimeHookTool:
            name = "runtime_hook_tool"

            def __init__(self) -> None:
                self.set_event_sink_called = False
                self._sink = None

            def set_event_sink(self, sink):
                self.set_event_sink_called = True
                self._sink = sink

            async def ainvoke(self, _args):
                if self._sink is not None:
                    await self._sink(StreamEvent("assistant_delta", {"delta": "unexpected"}))
                return {
                    "kind": "runtime_hook_tool",
                    "text": "ok",
                    "success": True,
                    "error": None,
                    "data": {},
                    "meta": {},
                }

        tool = _RuntimeHookTool()
        agent = ChatAgent(self._make_config(), tools=[tool])
        events = []
        async for event in agent.handle_message("run hook"):
            events.append(event)

        assert tool.set_event_sink_called is False
        assert not any(e.type == "subagent_trace_delta" for e in events)
        assert not any(e.type == "task_trace_delta" for e in events)

    @patch("src.agent.create_chat_model")
    async def test_runtime_event_opt_in_forwards_question_events(self, mock_create):
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(_messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(
                            name="question_hook_tool",
                            args="{}",
                            id="tc-question-hook-1",
                            index=0,
                        ),
                    ],
                )
            else:
                yield AIMessageChunk(content="continued", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        class _QuestionHookTool:
            name = "question_hook_tool"
            supports_runtime_events = True

            def __init__(self) -> None:
                self._sink = None

            def set_event_sink(self, sink):
                self._sink = sink

            async def ainvoke(self, _args):
                if self._sink is not None:
                    await self._sink({
                        "type": "question",
                        "data": {
                            "questionnaire_id": "qq-test",
                            "title": "Need details",
                            "questions": [
                                {
                                    "id": "q1",
                                    "question": "Choose one",
                                    "options": ["A", "B"],
                                    "multiple": False,
                                    "required": True,
                                }
                            ],
                        },
                    })
                return {
                    "kind": "question_hook_tool",
                    "text": "ok",
                    "success": True,
                    "error": None,
                    "data": {},
                    "meta": {},
                }

        tool = _QuestionHookTool()
        agent = ChatAgent(self._make_config(), tools=[tool])

        events = []
        async for event in agent.handle_message("ask"):
            events.append(event)

        question_event = next(e for e in events if e.type == "question")
        assert question_event.data["tool_call_id"] == "tc-question-hook-1"
        assert question_event.data["questionnaire_id"] == "qq-test"
        assert not any(e.type == "subagent_trace_delta" for e in events)
        assert any(e.type == "complete" for e in events)

    @patch("src.agent.create_chat_model")
    async def test_explore_tool_cancel_clears_event_sink(self, mock_create):
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        async def fake_astream(_messages):
            yield AIMessageChunk(
                content="",
                tool_call_chunks=[
                    ToolCallChunk(
                        name="explore",
                        args='{"description":"scan","prompt":"inspect"}',
                        id="tc-explore-cancel",
                        index=0,
                    ),
                ],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        class _Runner:
            def __init__(self) -> None:
                self.started = asyncio.Event()
                self.cancelled = asyncio.Event()

            async def run_subagent(self, **kwargs):
                sink = kwargs.get("event_sink")
                self.started.set()
                if sink is not None:
                    await sink(StreamEvent("assistant_delta", {"delta": "working"}))
                try:
                    while True:
                        await asyncio.sleep(0.05)
                except asyncio.CancelledError:
                    self.cancelled.set()
                    raise

        runner = _Runner()
        explore_tool = ExploreTool(runner=runner)
        agent = ChatAgent(self._make_config(), tools=[explore_tool])

        events: list[StreamEvent] = []

        async def _collect() -> None:
            async for event in agent.handle_message("run explore"):
                events.append(event)

        collect_task = asyncio.create_task(_collect())
        await asyncio.wait_for(runner.started.wait(), timeout=1)
        agent.cancel()
        await asyncio.wait_for(collect_task, timeout=1)

        assert runner.cancelled.is_set()
        assert explore_tool._event_sink is None
        assert any(e.type == "subagent_trace_delta" for e in events)
        error_event = next(e for e in events if e.type == "error")
        assert error_event.data["code"] == "cancelled"

    @patch("src.agent.create_chat_model")
    async def test_explore_tool_streams_large_trace_burst(self, mock_create):
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(_messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(
                            name="explore",
                            args='{"description":"scan","prompt":"inspect"}',
                            id="tc-explore-burst",
                            index=0,
                        ),
                    ],
                )
            else:
                yield AIMessageChunk(content="Task done.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        burst_size = 300

        class _Runner:
            async def run_subagent(self, **kwargs):
                sink = kwargs.get("event_sink")
                if sink is not None:
                    for _ in range(burst_size):
                        await sink(StreamEvent("assistant_delta", {"delta": "x"}))
                return {
                    "kind": "explore",
                    "text": "Subagent summary",
                    "success": True,
                    "error": None,
                    "data": {},
                    "meta": {},
                }

        explore_tool = ExploreTool(runner=_Runner())
        agent = ChatAgent(self._make_config(), tools=[explore_tool])

        events = []
        async for event in agent.handle_message("run explore"):
            events.append(event)

        trace_events = [e for e in events if e.type == "subagent_trace_delta"]
        assert len(trace_events) == burst_size
        assert all(e.data["event_type"] == "assistant_delta" for e in trace_events)

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
        assert tr.data["result"]["kind"] == "read"
        assert "base64" not in tr.data["result"]["text"]
        assert "img.png" in tr.data["result"]["text"]

        # The complete event's content blocks should also not contain base64
        complete_event = next(e for e in events if e.type == "complete")
        blocks = complete_event.data["tool_calls"]
        tool_block = next(b for b in blocks if b.get("type") == "tool_call")
        assert tool_block["result"]["kind"] == "read"
        assert "base64" not in tool_block["result"]["text"]

    @patch("src.agent.create_chat_model")
    async def test_tool_result_hides_llm_content_but_tool_message_keeps_it(self, mock_create):
        """tool_result should omit llm_content while ToolMessage keeps multimodal payload."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(
                            name="image_generation",
                            args='{"prompt":"a cat"}',
                            id="tc-img-1",
                            index=0,
                        ),
                    ],
                )
            else:
                yield AIMessageChunk(content="Done.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        llm_content = [
            {"type": "text", "text": "![Generated Image](sandbox:///generated_images/a.png)"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
        ]
        mock_tool = MagicMock()
        mock_tool.name = "image_generation"
        mock_tool.ainvoke = AsyncMock(return_value={
            "kind": "image_generation",
            "text": "![Generated Image](sandbox:///generated_images/a.png)",
            "success": True,
            "error": None,
            "data": {"media": [{"type": "image", "url": "sandbox:///generated_images/a.png"}]},
            "meta": {"image_count": 1},
            "llm_content": llm_content,
        })

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        events = []
        async for event in agent.handle_message("generate image"):
            events.append(event)

        tool_result_event = next(e for e in events if e.type == "tool_result")
        assert tool_result_event.data["result"]["kind"] == "image_generation"
        assert "llm_content" not in tool_result_event.data["result"]
        assert "sandbox:///" in tool_result_event.data["result"]["text"]

        tool_msgs = [m for m in agent.messages if isinstance(m, ToolMessage)]
        assert len(tool_msgs) == 1
        assert tool_msgs[0].content == llm_content

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
        bash_tool.ainvoke = AsyncMock(return_value={
            "kind": "bash",
            "text": "file1.txt\nimg.png",
            "stdout": "file1.txt\nimg.png",
            "stderr": "",
            "exit_code": 0,
            "timed_out": False,
            "truncated": False,
            "duration_ms": 5,
            "error": False,
        })

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
        # First tool (bash) returns structured bash result
        assert tool_results[0].data["result"]["kind"] == "bash"
        assert "file1.txt" in tool_results[0].data["result"]["text"]
        # Second tool (read) returns display text only
        assert tool_results[1].data["result"]["kind"] == "read"
        assert "base64" not in tool_results[1].data["result"]["text"]
        assert "img.png" in tool_results[1].data["result"]["text"]

    @patch("src.agent.create_chat_model")
    async def test_display_result_joins_multiple_text_blocks(self, mock_create):
        """If multimodal result has multiple text blocks, normalization should join them."""
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
        assert result["kind"] == "read"
        assert result["success"] is True
        assert result["text"] == "Part one. Part two."
        assert isinstance(result["llm_content"], list)
        assert len(result["llm_content"]) == 3


    @patch("src.agent.create_chat_model")
    async def test_thinking_blocks_preserved_in_messages_during_tool_loop(self, mock_create):
        """Thinking blocks should be preserved in AIMessage content during tool-call iterations."""
        from langchain_core.messages import AIMessageChunk, ToolCallChunk

        call_count = 0

        async def fake_astream(messages):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                # Iteration 1: thinking + text + tool call
                yield AIMessageChunk(
                    content=[{"type": "thinking", "thinking": "Let me search for this"}],
                    tool_call_chunks=[],
                )
                yield AIMessageChunk(
                    content=[{"type": "text", "text": "I'll search that."}],
                    tool_call_chunks=[],
                )
                yield AIMessageChunk(
                    content="",
                    tool_call_chunks=[
                        ToolCallChunk(name="web_search", args='{"query": "test"}', id="tc-1", index=0),
                    ],
                )
            else:
                # Iteration 2: final text
                yield AIMessageChunk(content="Here are the results.", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        mock_tool = MagicMock()
        mock_tool.name = "web_search"
        mock_tool.ainvoke = AsyncMock(return_value="search results here")

        agent = ChatAgent(self._make_config(), tools=[mock_tool])
        events = []
        async for event in agent.handle_message("search for test", deep_thinking=True):
            events.append(event)

        # Find the AIMessage with tool_calls (iteration 1)
        ai_with_tools = [
            m for m in agent.messages
            if isinstance(m, AIMessage) and m.tool_calls
        ]
        assert len(ai_with_tools) == 1
        msg = ai_with_tools[0]
        # Content should be a list (not a plain string) preserving thinking blocks
        assert isinstance(msg.content, list)
        # Should contain thinking block(s)
        thinking_blocks = [
            b for b in msg.content
            if isinstance(b, dict) and b.get("type") == "thinking"
        ]
        assert len(thinking_blocks) >= 1
        # Should also contain text block(s)
        text_blocks = [
            b for b in msg.content
            if isinstance(b, dict) and b.get("type") == "text"
        ]
        assert len(text_blocks) >= 1

    @patch("src.agent.create_chat_model")
    async def test_thinking_blocks_in_final_message_no_tool_calls(self, mock_create):
        """Thinking blocks should be preserved in the final AIMessage when no tool calls occur."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[{"type": "thinking", "thinking": "Deep thought here"}],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(
                content=[{"type": "text", "text": "My answer."}],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        agent = ChatAgent(self._make_config())
        events = []
        async for event in agent.handle_message("think about this", deep_thinking=True):
            events.append(event)

        # The final AIMessage should have list content with thinking blocks
        final_ai = agent.messages[-1]
        assert isinstance(final_ai, AIMessage)
        assert isinstance(final_ai.content, list)
        thinking_blocks = [
            b for b in final_ai.content
            if isinstance(b, dict) and b.get("type") == "thinking"
        ]
        assert len(thinking_blocks) >= 1
        text_blocks = [
            b for b in final_ai.content
            if isinstance(b, dict) and b.get("type") == "text"
        ]
        assert len(text_blocks) >= 1

    @patch("src.agent.create_chat_model")
    async def test_openai_strips_rs_ids_from_saved_content_blocks(self, mock_create):
        """OpenAI history should not keep rs_* ids that become invalid item refs."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[
                    {
                        "type": "reasoning",
                        "id": "rs_parent",
                        "summary": [{"type": "summary_text", "text": "plan", "id": "rs_child"}],
                    },
                ],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(
                content=[{"type": "text", "text": "Final answer"}],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(self._make_config(provider="openai"))
        async for _ in agent.handle_message("hi"):
            pass

        final_ai = agent.messages[-1]
        assert isinstance(final_ai, AIMessage)
        assert isinstance(final_ai.content, list)
        dumped = json.dumps(final_ai.content, ensure_ascii=False)
        assert "Final answer" in dumped
        assert "rs_parent" not in dumped
        assert "rs_child" not in dumped

    @patch("src.agent.create_chat_model")
    async def test_non_openai_preserves_content_ids(self, mock_create):
        """Only OpenAI should sanitize response/item ids from content blocks."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[{"type": "thinking", "thinking": "hmm", "id": "rs_keep"}],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(
                content=[{"type": "text", "text": "ok"}],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(
            self._make_config(provider="anthropic", model="claude-sonnet-4-20250514")
        )
        async for _ in agent.handle_message("hi"):
            pass

        final_ai = agent.messages[-1]
        assert isinstance(final_ai, AIMessage)
        assert isinstance(final_ai.content, list)
        dumped = json.dumps(final_ai.content, ensure_ascii=False)
        assert "rs_keep" in dumped


class TestSanitizeDelta:
    def test_normal_chinese_unchanged(self):
        assert sanitize_delta("你好世界") == "你好世界"

    def test_normal_ascii_unchanged(self):
        assert sanitize_delta("hello world") == "hello world"

    def test_empty_string(self):
        assert sanitize_delta("") == ""

    def test_single_replacement_stripped(self):
        assert sanitize_delta("你好\ufffd世界") == "你好世界"

    def test_multiple_replacements_stripped(self):
        assert sanitize_delta("\ufffd你\ufffd好\ufffd") == "你好"

    def test_only_replacement_chars(self):
        assert sanitize_delta("\ufffd\ufffd\ufffd") == ""

    def test_logs_warning(self, caplog):
        import logging
        with caplog.at_level(logging.WARNING, logger="claude-chat-agent"):
            sanitize_delta("abc\ufffdef")
        assert "Stripped 1 U+FFFD" in caplog.text


class TestStreamingSanitization:
    """Integration test: U+FFFD in LLM chunks must not reach assistant_delta or complete events."""

    @patch("src.agent.create_chat_model")
    async def test_ufffd_stripped_from_stream(self, mock_create):
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(content="你好\ufffd世界", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        config = AgentConfig({"conversation_id": "test", "api_key": "k"})
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("hi"):
            events.append(event)

        deltas = [e for e in events if e.type == "assistant_delta"]
        assert len(deltas) == 1
        assert "\ufffd" not in deltas[0].data["delta"]
        assert deltas[0].data["delta"] == "你好世界"

        complete = next(e for e in events if e.type == "complete")
        assert "\ufffd" not in complete.data["content"]
        assert complete.data["content"] == "你好世界"

    @patch("src.agent.create_chat_model")
    async def test_ufffd_in_thinking_block_stripped(self, mock_create):
        """U+FFFD in thinking content blocks should be stripped."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[{"type": "thinking", "thinking": "思考\ufffd中"}],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(content="结果", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        config = AgentConfig({"conversation_id": "test", "api_key": "k"})
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("think"):
            events.append(event)

        thinking_deltas = [e for e in events if e.type == "thinking_delta"]
        assert len(thinking_deltas) == 1
        assert "\ufffd" not in thinking_deltas[0].data["delta"]
        assert thinking_deltas[0].data["delta"] == "思考中"

    @patch("src.agent.create_chat_model")
    async def test_ufffd_in_text_block_list_stripped(self, mock_create):
        """U+FFFD in list-style text content blocks should be stripped."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[{"type": "text", "text": "你\ufffd好"}],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        config = AgentConfig({"conversation_id": "test", "api_key": "k"})
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("hi"):
            events.append(event)

        deltas = [e for e in events if e.type == "assistant_delta"]
        assert len(deltas) == 1
        assert deltas[0].data["delta"] == "你好"

    @patch("src.agent.create_chat_model")
    async def test_pure_ufffd_chunk_produces_no_delta(self, mock_create):
        """A chunk containing only U+FFFD should not emit an assistant_delta event."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(content="\ufffd", tool_call_chunks=[])
            yield AIMessageChunk(content="ok", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        config = AgentConfig({"conversation_id": "test", "api_key": "k"})
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("hi"):
            events.append(event)

        deltas = [e for e in events if e.type == "assistant_delta"]
        # Only the "ok" chunk should produce a delta, not the pure U+FFFD one
        assert len(deltas) == 1
        assert deltas[0].data["delta"] == "ok"

        complete = next(e for e in events if e.type == "complete")
        assert complete.data["content"] == "ok"

    @patch("src.agent.create_chat_model")
    async def test_multiple_chunks_with_ufffd(self, mock_create):
        """U+FFFD across multiple chunks should all be stripped, content accumulated correctly."""
        from langchain_core.messages import AIMessageChunk

        async def fake_astream(messages):
            yield AIMessageChunk(content="你\ufffd", tool_call_chunks=[])
            yield AIMessageChunk(content="\ufffd好", tool_call_chunks=[])
            yield AIMessageChunk(content="世界", tool_call_chunks=[])

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        config = AgentConfig({"conversation_id": "test", "api_key": "k"})
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("hi"):
            events.append(event)

        deltas = [e for e in events if e.type == "assistant_delta"]
        combined = "".join(e.data["delta"] for e in deltas)
        assert "\ufffd" not in combined
        assert combined == "你好世界"

        complete = next(e for e in events if e.type == "complete")
        assert complete.data["content"] == "你好世界"
