"""Tests for WebFetchTool async paths, cancellation, thinking deltas, and more."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest
from langchain_core.messages import AIMessageChunk, ToolCallChunk

from src.agent import ChatAgent, StreamEvent
from src.tools.web import WebFetchTool, WebSearchTool
from src.tools.search import GrepTool
from .result_helpers import _rtext


# ---------------------------------------------------------------------------
# Helpers (same as conftest but importable)
# ---------------------------------------------------------------------------

def _make_config(**overrides):
    from src.agent import AgentConfig
    return AgentConfig({
        "conversation_id": "test-conv",
        "provider": "openai",
        "model": "gpt-4o",
        "api_key": "test-key",
        "system_prompt": "You are a test assistant.",
        **overrides,
    })


# ---------------------------------------------------------------------------
# WebFetchTool async tests
# ---------------------------------------------------------------------------

class TestWebFetchToolAsync:
    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_html(self, mock_client_cls):
        mock_response = MagicMock()
        mock_response.text = (
            "<html><head><title>Test</title></head>"
            "<body><h1>Hello</h1><p>World</p></body></html>"
        )
        mock_response.headers = {"content-type": "text/html; charset=utf-8"}
        mock_response.raise_for_status = MagicMock()

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://example.com")
        # html2text converts to markdown — should preserve heading
        assert "Hello" in _rtext(result)
        assert "World" in _rtext(result)

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_plain_text(self, mock_client_cls):
        mock_response = MagicMock()
        mock_response.text = "Plain text content here"
        mock_response.headers = {"content-type": "text/plain"}
        mock_response.raise_for_status = MagicMock()

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://example.com/file.txt")
        assert _rtext(result) == "Plain text content here"

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_truncation(self, mock_client_cls):
        long_text = "x" * 60000
        mock_response = MagicMock()
        mock_response.text = long_text
        mock_response.headers = {"content-type": "text/plain"}
        mock_response.raise_for_status = MagicMock()

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://example.com/big", max_length=100)
        assert len(_rtext(result)) < 200
        assert "content truncated" in _rtext(result)

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_timeout(self, mock_client_cls):
        mock_client = AsyncMock()
        mock_client.get = AsyncMock(
            side_effect=httpx.TimeoutException("timed out")
        )
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://slow.example.com")
        assert "timed out" in _rtext(result).lower()

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_connect_error(self, mock_client_cls):
        mock_client = AsyncMock()
        mock_client.get = AsyncMock(
            side_effect=httpx.ConnectError("connection refused")
        )
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://down.example.com")
        assert "could not connect" in _rtext(result).lower()

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_http_status_error(self, mock_client_cls):
        mock_response = MagicMock()
        mock_response.status_code = 404
        error = httpx.HTTPStatusError(
            "Not Found", request=MagicMock(), response=mock_response
        )

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(side_effect=error)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://example.com/missing")
        assert "404" in _rtext(result)

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_fetch_generic_http_error(self, mock_client_cls):
        mock_client = AsyncMock()
        mock_client.get = AsyncMock(
            side_effect=httpx.HTTPError("something went wrong")
        )
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebFetchTool()
        result = await tool._arun("https://example.com/error")
        assert "error" in _rtext(result).lower()


# ---------------------------------------------------------------------------
# Cancellation during streaming
# ---------------------------------------------------------------------------

class TestCancelDuringStreaming:
    @patch("src.agent.create_chat_model")
    async def test_cancel_stops_iteration(self, mock_create):
        """Setting _cancelled mid-stream should stop yielding events."""
        chunks_yielded = 0

        async def fake_astream(messages):
            nonlocal chunks_yielded
            for i in range(10):
                chunks_yielded += 1
                yield AIMessageChunk(
                    content=f"chunk{i} ", tool_call_chunks=[]
                )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_create.return_value = mock_llm

        agent = ChatAgent(_make_config())
        events = []
        async for event in agent.handle_message("test"):
            events.append(event)
            if len(events) == 2:
                agent.cancel()

        # Should have stopped early — no complete event
        types = [e.type for e in events]
        assert "complete" not in types
        # Should have gotten some deltas but not all 10
        delta_count = sum(1 for e in events if e.type == "assistant_delta")
        assert delta_count < 10


# ---------------------------------------------------------------------------
# Thinking / reasoning delta tests
# ---------------------------------------------------------------------------

class TestThinkingDeltas:
    @patch("src.agent.create_chat_model")
    async def test_anthropic_thinking_blocks(self, mock_create):
        """Anthropic-style thinking blocks should emit thinking_delta events."""
        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[
                    {"type": "thinking", "thinking": "Let me think..."},
                ],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(
                content=[
                    {"type": "text", "text": "Here's my answer."},
                ],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="anthropic")
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("think about this", deep_thinking=True):
            events.append(event)

        thinking_events = [e for e in events if e.type == "thinking_delta"]
        assert len(thinking_events) >= 1
        assert "Let me think" in thinking_events[0].data["delta"]

        delta_events = [e for e in events if e.type == "assistant_delta"]
        assert any("answer" in e.data["delta"] for e in delta_events)

    @patch("src.agent.create_chat_model")
    async def test_openai_reasoning_blocks(self, mock_create):
        """OpenAI-style reasoning blocks should emit thinking_delta events."""
        async def fake_astream(messages):
            yield AIMessageChunk(
                content=[
                    {
                        "type": "reasoning",
                        "summary": [{"text": "Reasoning step 1"}],
                    },
                ],
                tool_call_chunks=[],
            )
            yield AIMessageChunk(
                content=[
                    {"type": "text", "text": "Final answer."},
                ],
                tool_call_chunks=[],
            )

        mock_llm = MagicMock()
        mock_llm.astream = fake_astream
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="openai")
        agent = ChatAgent(config)
        events = []
        async for event in agent.handle_message("reason", deep_thinking=True):
            events.append(event)

        thinking_events = [e for e in events if e.type == "thinking_delta"]
        assert len(thinking_events) >= 1
        assert "Reasoning step 1" in thinking_events[0].data["delta"]


# ---------------------------------------------------------------------------
# Max iterations guard
# ---------------------------------------------------------------------------

class TestMaxIterations:
    @patch("src.agent.create_chat_model")
    async def test_max_iterations_exceeded(self, mock_create):
        """Agent should stop after MAX_ITERATIONS and emit an error."""
        import src.agent as agent_module

        async def infinite_tool_calls(messages):
            yield AIMessageChunk(
                content="",
                tool_call_chunks=[
                    ToolCallChunk(
                        name="bash",
                        args='{"command": "echo hi"}',
                        id="tc-1",
                        index=0,
                    ),
                ],
            )

        mock_llm = MagicMock()
        mock_llm.astream = infinite_tool_calls
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        mock_tool = MagicMock()
        mock_tool.name = "bash"
        mock_tool.ainvoke = AsyncMock(return_value="hi")

        previous_max_iterations = agent_module.MAX_ITERATIONS
        agent_module.MAX_ITERATIONS = 2
        try:
            agent = ChatAgent(_make_config(), tools=[mock_tool])
            events = []
            async for event in agent.handle_message("loop forever"):
                events.append(event)
        finally:
            agent_module.MAX_ITERATIONS = previous_max_iterations

        error_events = [e for e in events if e.type == "error"]
        assert len(error_events) == 1
        assert "max_iterations" in error_events[0].data["code"]


# ---------------------------------------------------------------------------
# GrepTool regex tests
# ---------------------------------------------------------------------------

class TestGrepRegex:
    def test_grep_with_regex_groups(self, workspace):
        import os
        with open(os.path.join(workspace, "code.py"), "w") as f:
            f.write("def foo_bar():\n    pass\ndef baz_qux():\n    pass\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run(r"def (\w+)\(\)")
        assert "foo_bar" in _rtext(result)
        assert "baz_qux" in _rtext(result)

    def test_grep_with_character_class(self, workspace):
        import os
        with open(os.path.join(workspace, "data.txt"), "w") as f:
            f.write("abc123\nxyz789\nhello\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run(r"[a-z]+\d+")
        assert "abc123" in _rtext(result)
        assert "xyz789" in _rtext(result)
        assert "hello" not in _rtext(result)

    def test_grep_with_lookahead(self, workspace):
        import os
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("foobar\nfoobaz\nfoo\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run(r"foo(?=bar)")
        assert "foobar" in _rtext(result)
        assert "foobaz" not in _rtext(result)


# ---------------------------------------------------------------------------
# web_search missing from tool description test
# ---------------------------------------------------------------------------

class TestToolDescriptionCompleteness:
    def test_web_search_has_description(self):
        from src.prompts.tools import TOOL_DESCRIPTIONS
        assert "web_search" in TOOL_DESCRIPTIONS
        assert len(TOOL_DESCRIPTIONS["web_search"]) > 50


# ---------------------------------------------------------------------------
# Path traversal with shared prefix (the actual vulnerability)
# ---------------------------------------------------------------------------

class TestPathTraversalSharedPrefix:
    def test_shared_prefix_blocked(self, workspace):
        """Paths like /workspace2/foo must be rejected when workspace is /workspace."""
        from src.tools._paths import resolve_workspace_path
        # This is the exact case that str.startswith() would miss
        sibling = workspace + "2"
        import os
        os.makedirs(sibling, exist_ok=True)
        fake_path = os.path.join(sibling, "secret.txt")
        with open(fake_path, "w") as f:
            f.write("secret")

        with pytest.raises(ValueError, match="outside the workspace"):
            resolve_workspace_path(fake_path, workspace)


# ---------------------------------------------------------------------------
# _get_thinking_llm with custom thinking_budget
# ---------------------------------------------------------------------------

class TestGetThinkingLlmBudget:
    @patch("src.agent.create_chat_model")
    def test_default_budget_anthropic(self, mock_create):
        """_get_thinking_llm(None) should use default budget (128000) for Anthropic."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="anthropic")
        agent = ChatAgent(config)
        agent._get_thinking_llm(None)

        mock_llm.bind.assert_called_once_with(
            max_tokens=128000,
            thinking={"type": "enabled", "budget_tokens": 128000 - 1},
        )

    @patch("src.agent.create_chat_model")
    def test_custom_budget_anthropic(self, mock_create):
        """_get_thinking_llm(200000) should use custom budget for Anthropic."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="anthropic")
        agent = ChatAgent(config)
        agent._get_thinking_llm(200000)

        mock_llm.bind.assert_called_once_with(
            max_tokens=200000,
            thinking={"type": "enabled", "budget_tokens": 200000 - 1},
        )

    @patch("src.agent.create_chat_model")
    def test_custom_budget_google(self, mock_create):
        """_get_thinking_llm(100000) should use custom budget for Google."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="google")
        agent = ChatAgent(config)
        agent._get_thinking_llm(100000)

        mock_llm.bind.assert_called_once_with(
            max_output_tokens=100000,
            thinking_budget=100000 - 1,
        )

    @patch("src.agent.create_chat_model")
    def test_default_budget_google(self, mock_create):
        """_get_thinking_llm(None) should use default budget (128000) for Google."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="google")
        agent = ChatAgent(config)
        agent._get_thinking_llm(None)

        mock_llm.bind.assert_called_once_with(
            max_output_tokens=128000,
            thinking_budget=128000 - 1,
        )

    @patch("src.agent.create_chat_model")
    def test_openai_uses_budget(self, mock_create):
        """OpenAI should set max_completion_tokens from budget."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="openai")
        agent = ChatAgent(config)
        agent._get_thinking_llm(200000)

        mock_llm.bind.assert_called_once_with(
            max_completion_tokens=200000,
            reasoning={"effort": "high", "summary": "auto"},
        )

    @patch("src.agent.create_chat_model")
    def test_mistral_uses_budget_as_max_tokens(self, mock_create):
        """Mistral (no thinking) should set max_tokens from budget."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="mistral")
        agent = ChatAgent(config)
        agent._get_thinking_llm(50000)

        mock_llm.bind.assert_called_once_with(max_tokens=50000)


class TestGetBudgetedLlmBudget:
    @patch("src.agent.create_chat_model")
    def test_default_budget_openai(self, mock_create):
        """_get_budgeted_llm(None) should use default budget for OpenAI."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="openai")
        agent = ChatAgent(config)
        agent._get_budgeted_llm(None)

        mock_llm.bind.assert_called_once_with(max_completion_tokens=128000)

    @patch("src.agent.create_chat_model")
    def test_budgeted_llm_clamps_below_min(self, mock_create):
        """_get_budgeted_llm should clamp too-small values to a safe minimum."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="openai")
        agent = ChatAgent(config)
        agent._get_budgeted_llm(1)

        mock_llm.bind.assert_called_once_with(max_completion_tokens=1024)

    @patch("src.agent.create_chat_model")
    def test_budgeted_llm_clamps_above_max(self, mock_create):
        """_get_budgeted_llm should clamp too-large values to a safe maximum."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="openai")
        agent = ChatAgent(config)
        agent._get_budgeted_llm(2_000_000)

        mock_llm.bind.assert_called_once_with(max_completion_tokens=1_000_000)

    @patch("src.agent.create_chat_model")
    def test_thinking_llm_clamps_below_min_before_provider_specific_budget(self, mock_create):
        """_get_thinking_llm should clamp then derive provider-specific thinking budget."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="anthropic")
        agent = ChatAgent(config)
        agent._get_thinking_llm(1)

        mock_llm.bind.assert_called_once_with(
            max_tokens=1024,
            thinking={"type": "enabled", "budget_tokens": 1023},
        )

    @patch("src.agent.create_chat_model")
    def test_thinking_llm_google_clamps_above_max_before_provider_specific_budget(self, mock_create):
        """Google thinking params should derive from the clamped max budget."""
        mock_llm = MagicMock()
        mock_llm.bind = MagicMock(return_value=mock_llm)
        mock_llm.bind_tools = MagicMock(return_value=mock_llm)
        mock_create.return_value = mock_llm

        config = _make_config(provider="google")
        agent = ChatAgent(config)
        agent._get_thinking_llm(2_000_000)

        mock_llm.bind.assert_called_once_with(
            max_output_tokens=1_000_000,
            thinking_budget=999999,
        )
