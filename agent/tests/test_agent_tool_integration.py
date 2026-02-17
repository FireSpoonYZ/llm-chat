"""Integration tests: each built-in tool through the full agent loop."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from langchain_core.messages import AIMessageChunk, ToolCallChunk

from src.agent import AgentConfig, ChatAgent, StreamEvent
from src.tools import create_all_tools


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_config(**overrides) -> AgentConfig:
    return AgentConfig({
        "conversation_id": "integ-test",
        "provider": "openai",
        "model": "gpt-4o",
        "api_key": "test-key",
        "system_prompt": "You are a test assistant.",
        **overrides,
    })


def _tool_call_chunk(name: str, args: dict, tc_id: str = "tc-1") -> AIMessageChunk:
    """Create an AIMessageChunk containing a single complete tool call."""
    return AIMessageChunk(
        content="",
        tool_call_chunks=[
            ToolCallChunk(name=name, args=json.dumps(args), id=tc_id, index=0),
        ],
    )


def _text_chunk(text: str) -> AIMessageChunk:
    return AIMessageChunk(content=text, tool_call_chunks=[])


def _make_fake_astream(*iterations):
    """Build a fake astream that yields different chunks per call.

    Each positional arg is a list of AIMessageChunk for one iteration.
    """
    call_count = 0

    async def fake_astream(messages):
        nonlocal call_count
        chunks = iterations[call_count] if call_count < len(iterations) else [_text_chunk("")]
        call_count += 1
        for c in chunks:
            yield c

    return fake_astream


def _setup_mock_llm(mock_create, fake_astream):
    """Wire up mock_create to return a mock LLM with the given astream."""
    mock_llm = MagicMock()
    mock_llm.astream = fake_astream
    mock_llm.bind_tools = MagicMock(return_value=mock_llm)
    mock_create.return_value = mock_llm
    return mock_llm


async def _collect_events(agent: ChatAgent, message: str = "go") -> list[StreamEvent]:
    events: list[StreamEvent] = []
    async for event in agent.handle_message(message):
        events.append(event)
    return events


def _events_by_type(events: list[StreamEvent], t: str) -> list[StreamEvent]:
    return [e for e in events if e.type == t]


def _result_text(event: StreamEvent) -> str:
    result = event.data["result"]
    if isinstance(result, dict):
        return str(result.get("text", ""))
    return str(result)


# ---------------------------------------------------------------------------
# Fixtures (workspace provided by conftest.py)
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Tests — one per tool
# ---------------------------------------------------------------------------

class TestBashToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_bash_echo(self, mock_create, workspace):
        fake = _make_fake_astream(
            [_tool_call_chunk("bash", {"command": "echo hello"})],
            [_text_chunk("Done")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        bash = next(t for t in tools if t.name == "bash")
        agent = ChatAgent(_make_config(), tools=[bash])
        events = await _collect_events(agent)

        tc_events = _events_by_type(events, "tool_call")
        assert len(tc_events) == 1
        assert tc_events[0].data["tool_name"] == "bash"

        tr_events = _events_by_type(events, "tool_result")
        assert len(tr_events) == 1
        assert tr_events[0].data["result"]["kind"] == "bash"
        assert "hello" in _result_text(tr_events[0])
        assert tr_events[0].data["is_error"] is False

        complete = _events_by_type(events, "complete")
        assert len(complete) == 1
        assert complete[0].data["tool_calls"] is not None
        assert complete[0].data["tool_calls"][0]["name"] == "bash"
        assert complete[0].data["tool_calls"][0]["result"]["kind"] == "bash"


class TestReadToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_read_file(self, mock_create, workspace):
        # Create a file to read
        import os
        test_file = os.path.join(workspace, "test.txt")
        with open(test_file, "w") as f:
            f.write("hello world\n")

        fake = _make_fake_astream(
            [_tool_call_chunk("read", {"file_path": "test.txt"})],
            [_text_chunk("Got it")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        read_tool = next(t for t in tools if t.name == "read")
        agent = ChatAgent(_make_config(), tools=[read_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "read"
        assert "hello world" in _result_text(tr)
        assert tr.data["is_error"] is False


class TestWriteToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_write_file(self, mock_create, workspace):
        fake = _make_fake_astream(
            [_tool_call_chunk("write", {"file_path": "out.txt", "content": "hi there"})],
            [_text_chunk("Written")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        write_tool = next(t for t in tools if t.name == "write")
        agent = ChatAgent(_make_config(), tools=[write_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "write"
        assert "Successfully wrote" in _result_text(tr)
        assert tr.data["is_error"] is False

        # Verify file was actually written
        import os
        with open(os.path.join(workspace, "out.txt")) as f:
            assert f.read() == "hi there"


class TestEditToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_edit_file(self, mock_create, workspace):
        import os
        test_file = os.path.join(workspace, "edit_me.txt")
        with open(test_file, "w") as f:
            f.write("old text here\n")

        fake = _make_fake_astream(
            [_tool_call_chunk("edit", {
                "file_path": "edit_me.txt",
                "old_string": "old text",
                "new_string": "new text",
            })],
            [_text_chunk("Edited")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        edit_tool = next(t for t in tools if t.name == "edit")
        agent = ChatAgent(_make_config(), tools=[edit_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "edit"
        assert "Successfully replaced" in _result_text(tr)

        with open(test_file) as f:
            assert "new text here" in f.read()


class TestGlobToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_glob_pattern(self, mock_create, workspace):
        import os
        for name in ["a.txt", "b.txt", "c.py"]:
            with open(os.path.join(workspace, name), "w") as f:
                f.write("x")

        fake = _make_fake_astream(
            [_tool_call_chunk("glob", {"pattern": "*.txt"})],
            [_text_chunk("Found them")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        glob_tool = next(t for t in tools if t.name == "glob")
        agent = ChatAgent(_make_config(), tools=[glob_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "glob"
        assert "a.txt" in _result_text(tr)
        assert "b.txt" in _result_text(tr)
        assert "c.py" not in _result_text(tr)


class TestGrepToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_grep_pattern(self, mock_create, workspace):
        import os
        with open(os.path.join(workspace, "data.txt"), "w") as f:
            f.write("line one\nhello world\nline three\n")

        fake = _make_fake_astream(
            [_tool_call_chunk("grep", {"pattern": "hello"})],
            [_text_chunk("Found")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        grep_tool = next(t for t in tools if t.name == "grep")
        agent = ChatAgent(_make_config(), tools=[grep_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "grep"
        assert "hello world" in _result_text(tr)


class TestWebFetchToolIntegration:
    @patch("src.agent.create_chat_model")
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_web_fetch(self, mock_client_cls, mock_create, workspace):
        # Mock httpx response
        mock_response = MagicMock()
        mock_response.text = "<html><body>Example Page Content</body></html>"
        mock_response.headers = {"content-type": "text/html"}
        mock_response.raise_for_status = MagicMock()

        mock_client = AsyncMock()
        mock_client.get = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        fake = _make_fake_astream(
            [_tool_call_chunk("web_fetch", {"url": "https://example.com"})],
            [_text_chunk("Fetched")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        web_tool = next(t for t in tools if t.name == "web_fetch")
        agent = ChatAgent(_make_config(), tools=[web_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "web_fetch"
        assert "Example Page Content" in _result_text(tr)
        assert tr.data["is_error"] is False


class TestCodeInterpreterToolIntegration:
    @patch("src.agent.create_chat_model")
    async def test_code_interpreter_python(self, mock_create, workspace):
        fake = _make_fake_astream(
            [_tool_call_chunk("code_interpreter", {"code": "print(42)", "language": "python"})],
            [_text_chunk("Result")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        ci_tool = next(t for t in tools if t.name == "code_interpreter")
        agent = ChatAgent(_make_config(), tools=[ci_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "code_interpreter"
        assert "42" in _result_text(tr)
        assert tr.data["is_error"] is False


class TestWebSearchToolIntegration:
    @patch("src.agent.create_chat_model")
    @patch("src.tools.web.httpx_sse.aconnect_sse")
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_web_search(self, mock_client_cls, mock_aconnect, mock_create, workspace):
        # Mock SSE event
        mock_event = MagicMock()
        mock_event.data = json.dumps({
            "jsonrpc": "2.0", "id": 1,
            "result": {"content": [{"type": "text", "text": "Latest news results"}]},
        })

        mock_event_source = MagicMock()
        async def _aiter():
            yield mock_event
        mock_event_source.aiter_sse = _aiter

        mock_ctx = AsyncMock()
        mock_ctx.__aenter__ = AsyncMock(return_value=mock_event_source)
        mock_ctx.__aexit__ = AsyncMock(return_value=False)
        mock_aconnect.return_value = mock_ctx

        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        fake = _make_fake_astream(
            [_tool_call_chunk("web_search", {"query": "latest news"})],
            [_text_chunk("Here are the results")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        ws_tool = next(t for t in tools if t.name == "web_search")
        agent = ChatAgent(_make_config(), tools=[ws_tool])
        events = await _collect_events(agent)

        tr = _events_by_type(events, "tool_result")[0]
        assert tr.data["result"]["kind"] == "web_search"
        assert "Latest news results" in _result_text(tr)
        assert tr.data["is_error"] is False


# ---------------------------------------------------------------------------
# Ghost tool call (index gap) test
# ---------------------------------------------------------------------------

class TestGhostToolCallFiltering:
    @patch("src.agent.create_chat_model")
    async def test_index_gap_ghost_tool_call_filtered(self, mock_create, workspace):
        """When LLM sends tool_call_chunks starting at index=1, the ghost
        entry at index=0 should be filtered out and not executed."""

        # Chunk with index=1 (skipping 0) — creates a ghost at position 0
        ghost_chunk = AIMessageChunk(
            content="",
            tool_call_chunks=[
                ToolCallChunk(name="bash", args=json.dumps({"command": "echo hi"}),
                              id="tc-real", index=1),
            ],
        )

        fake = _make_fake_astream(
            [_text_chunk("Sure"), ghost_chunk],
            [_text_chunk("Done")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        bash = next(t for t in tools if t.name == "bash")
        agent = ChatAgent(_make_config(), tools=[bash])
        events = await _collect_events(agent)

        # Only the real tool call should appear — no ghost
        tc_events = _events_by_type(events, "tool_call")
        assert len(tc_events) == 1
        assert tc_events[0].data["tool_name"] == "bash"

        # No "Unknown tool: " error from the ghost entry
        tr_events = _events_by_type(events, "tool_result")
        assert len(tr_events) == 1
        assert tr_events[0].data["is_error"] is False


# ---------------------------------------------------------------------------
# Error preservation test
# ---------------------------------------------------------------------------

class TestToolErrorPreservesContent:
    @patch("src.agent.create_chat_model")
    async def test_tool_error_preserves_prior_content(self, mock_create, workspace):
        """When a tool errors, total_content still includes text from all iterations."""
        fake = _make_fake_astream(
            # Iteration 1: text + tool call
            [_text_chunk("Sure, "), _tool_call_chunk("bash", {"command": "exit 1"})],
            # Iteration 2: LLM responds after tool error
            [_text_chunk("The command failed.")],
        )
        _setup_mock_llm(mock_create, fake)

        tools = create_all_tools(workspace)
        bash = next(t for t in tools if t.name == "bash")
        agent = ChatAgent(_make_config(), tools=[bash])
        events = await _collect_events(agent)

        complete = _events_by_type(events, "complete")
        assert len(complete) == 1
        content = complete[0].data["content"]
        # Must contain text from BOTH iterations
        assert "Sure, " in content
        assert "The command failed." in content
        # tool_calls should be recorded (Format A: interleaved content blocks)
        blocks = complete[0].data["tool_calls"]
        assert blocks is not None
        tool_call_blocks = [b for b in blocks if b.get("type") == "tool_call"]
        assert len(tool_call_blocks) == 1
        assert tool_call_blocks[0]["name"] == "bash"
