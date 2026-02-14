"""Shared fixtures and helpers for agent tests."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import MagicMock

import pytest
from langchain_core.messages import AIMessageChunk, ToolCallChunk

from src.agent import AgentConfig, ChatAgent, StreamEvent


@pytest.fixture
def workspace(tmp_path):
    """Create a temporary workspace directory."""
    return str(tmp_path)


def make_config(**overrides) -> AgentConfig:
    """Create an AgentConfig with sensible test defaults."""
    return AgentConfig({
        "conversation_id": "test-conv",
        "provider": "openai",
        "model": "gpt-4o",
        "api_key": "test-key",
        "system_prompt": "You are a test assistant.",
        **overrides,
    })


def tool_call_chunk(
    name: str, args: dict, tc_id: str = "tc-1"
) -> AIMessageChunk:
    """Create an AIMessageChunk containing a single complete tool call."""
    return AIMessageChunk(
        content="",
        tool_call_chunks=[
            ToolCallChunk(
                name=name, args=json.dumps(args), id=tc_id, index=0
            ),
        ],
    )


def text_chunk(text: str) -> AIMessageChunk:
    return AIMessageChunk(content=text, tool_call_chunks=[])
