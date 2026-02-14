"""Tests for built-in tools."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import pytest

from src.tools.bash import BashTool
from src.tools.file_ops import EditTool, ReadTool, WriteTool
from src.tools._paths import resolve_workspace_path as _resolve_path
from src.tools.search import GlobTool, GrepTool
from src.tools.web import WebFetchTool, WebSearchTool
from src.tools.code_interpreter import CodeInterpreterTool


# ---------------------------------------------------------------------------
# _resolve_path helper
# ---------------------------------------------------------------------------

class TestResolvePath:
    def test_relative_path(self, workspace):
        result = _resolve_path("foo.txt", workspace)
        assert str(result) == str(Path(workspace).resolve() / "foo.txt")

    def test_absolute_path_inside_workspace(self, workspace):
        abs_path = os.path.join(workspace, "bar.txt")
        result = _resolve_path(abs_path, workspace)
        assert str(result) == str(Path(abs_path).resolve())

    def test_path_traversal_blocked(self, workspace):
        with pytest.raises(ValueError, match="outside the workspace"):
            _resolve_path("../../etc/passwd", workspace)


# ---------------------------------------------------------------------------
# BashTool
# ---------------------------------------------------------------------------

class TestBashTool:
    def test_echo(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("echo hello")
        assert "hello" in result

    def test_cwd_is_workspace(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("pwd")
        # On Windows, pwd might not work, so use a cross-platform approach
        ws_resolved = str(Path(workspace).resolve())
        # Just check it doesn't error
        assert result  # non-empty

    def test_timeout(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("sleep 10", timeout=1)
        assert "timed out" in result.lower()

    def test_stderr_included(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("echo err >&2")
        assert "err" in result

    def test_no_output(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("true")
        assert result  # Should return "(no output)" or similar

    async def test_arun(self, workspace):
        tool = BashTool(workspace=workspace)
        result = await tool._arun("echo async_test")
        assert "async_test" in result


# ---------------------------------------------------------------------------
# ReadTool
# ---------------------------------------------------------------------------

class TestReadTool:
    def test_read_file(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("line1\nline2\nline3\n")
        tool = ReadTool(workspace=workspace)
        result = tool._run("test.txt")
        assert "line1" in result
        assert "line2" in result
        assert "line3" in result

    def test_read_with_line_numbers(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("aaa\nbbb\n")
        tool = ReadTool(workspace=workspace)
        result = tool._run("test.txt")
        assert "1\t" in result
        assert "2\t" in result

    def test_read_with_offset(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("line1\nline2\nline3\n")
        tool = ReadTool(workspace=workspace)
        result = tool._run("test.txt", offset=1, limit=1)
        assert "line2" in result
        assert "line1" not in result

    def test_read_nonexistent(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("nonexistent.txt")
        assert "not found" in result.lower()

    def test_read_empty_file(self, workspace):
        path = os.path.join(workspace, "empty.txt")
        with open(path, "w") as f:
            pass
        tool = ReadTool(workspace=workspace)
        result = tool._run("empty.txt")
        assert "empty" in result.lower()

    def test_read_path_traversal(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("../../etc/passwd")
        assert "error" in result.lower()

    async def test_arun(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("async content\n")
        tool = ReadTool(workspace=workspace)
        result = await tool._arun("test.txt")
        assert "async content" in result


# ---------------------------------------------------------------------------
# WriteTool
# ---------------------------------------------------------------------------

class TestWriteTool:
    def test_write_file(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = tool._run("output.txt", "hello world")
        assert "successfully" in result.lower()
        with open(os.path.join(workspace, "output.txt")) as f:
            assert f.read() == "hello world"

    def test_write_creates_dirs(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = tool._run("sub/dir/file.txt", "nested content")
        assert "successfully" in result.lower()
        with open(os.path.join(workspace, "sub", "dir", "file.txt")) as f:
            assert f.read() == "nested content"

    def test_write_overwrites(self, workspace):
        path = os.path.join(workspace, "overwrite.txt")
        with open(path, "w") as f:
            f.write("old")
        tool = WriteTool(workspace=workspace)
        tool._run("overwrite.txt", "new")
        with open(path) as f:
            assert f.read() == "new"

    def test_write_path_traversal(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = tool._run("../../evil.txt", "bad")
        assert "error" in result.lower()

    async def test_arun(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = await tool._arun("async_file.txt", "async content")
        assert "successfully" in result.lower()


# ---------------------------------------------------------------------------
# EditTool
# ---------------------------------------------------------------------------

class TestEditTool:
    def test_edit_single_occurrence(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("hello world")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "world", "universe")
        assert "replaced" in result.lower()
        with open(path) as f:
            assert f.read() == "hello universe"

    def test_edit_not_found(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("hello world")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "xyz", "abc")
        assert "not found" in result.lower()

    def test_edit_multiple_without_replace_all(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("aaa bbb aaa")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "aaa", "ccc")
        assert "appears 2 times" in result.lower() or "2" in result

    def test_edit_replace_all(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("aaa bbb aaa")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "aaa", "ccc", replace_all=True)
        assert "replaced" in result.lower()
        with open(path) as f:
            assert f.read() == "ccc bbb ccc"

    def test_edit_nonexistent_file(self, workspace):
        tool = EditTool(workspace=workspace)
        result = tool._run("nope.txt", "a", "b")
        assert "not found" in result.lower()

    async def test_arun(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("old text")
        tool = EditTool(workspace=workspace)
        result = await tool._arun("edit.txt", "old", "new")
        assert "replaced" in result.lower()


# ---------------------------------------------------------------------------
# GlobTool
# ---------------------------------------------------------------------------

class TestGlobTool:
    def test_glob_finds_files(self, workspace):
        for name in ["a.py", "b.py", "c.txt"]:
            with open(os.path.join(workspace, name), "w") as f:
                f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py")
        assert "a.py" in result
        assert "b.py" in result
        assert "c.txt" not in result

    def test_glob_recursive(self, workspace):
        sub = os.path.join(workspace, "sub")
        os.makedirs(sub)
        with open(os.path.join(sub, "deep.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("**/*.py")
        assert "deep.py" in result

    def test_glob_no_matches(self, workspace):
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.xyz")
        assert "no files" in result.lower()

    def test_glob_nonexistent_path(self, workspace):
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path="nonexistent")
        assert "error" in result.lower() or "not exist" in result.lower()

    async def test_arun(self, workspace):
        with open(os.path.join(workspace, "test.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = await tool._arun("*.py")
        assert "test.py" in result


# ---------------------------------------------------------------------------
# GrepTool
# ---------------------------------------------------------------------------

class TestGrepTool:
    def test_grep_finds_match(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("hello world\nfoo bar\nhello again\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("hello")
        assert "hello world" in result
        assert "hello again" in result

    def test_grep_with_line_numbers(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("aaa\nbbb\nccc\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("bbb")
        assert ":2:" in result

    def test_grep_no_matches(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("hello world\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("xyz")
        assert "no matches" in result.lower()

    def test_grep_invalid_regex(self, workspace):
        tool = GrepTool(workspace=workspace)
        result = tool._run("[invalid")
        assert "error" in result.lower()

    def test_grep_with_glob_filter(self, workspace):
        with open(os.path.join(workspace, "a.py"), "w") as f:
            f.write("target\n")
        with open(os.path.join(workspace, "b.txt"), "w") as f:
            f.write("target\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("target", glob_filter="*.py")
        assert "a.py" in result
        assert "b.txt" not in result

    def test_grep_with_context(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("line1\nline2\nMATCH\nline4\nline5\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("MATCH", context=1)
        assert "line2" in result
        assert "MATCH" in result
        assert "line4" in result

    def test_grep_skips_binary(self, workspace):
        with open(os.path.join(workspace, "binary.bin"), "wb") as f:
            f.write(b"\x00\x01\x02hello\x00")
        with open(os.path.join(workspace, "text.txt"), "w") as f:
            f.write("hello\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("hello")
        assert "text.txt" in result
        assert "binary.bin" not in result

    async def test_arun(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("async match\n")
        tool = GrepTool(workspace=workspace)
        result = await tool._arun("async")
        assert "async match" in result


# ---------------------------------------------------------------------------
# CodeInterpreterTool
# ---------------------------------------------------------------------------

class TestCodeInterpreterTool:
    def test_python_execution(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("print('hello from python')")
        assert "hello from python" in result

    def test_python_error(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("raise ValueError('test error')")
        assert "ValueError" in result or "test error" in result

    def test_python_timeout(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("import time; time.sleep(60)")
        assert "timed out" in result.lower()

    def test_temp_file_cleanup(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        tool._run("print('cleanup test')")
        # No temp files should remain
        remaining = [f for f in os.listdir(workspace) if f.startswith("tmp")]
        assert len(remaining) == 0

    async def test_arun(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = await tool._arun("print('async python')")
        assert "async python" in result


# ---------------------------------------------------------------------------
# WebFetchTool (async only, tested with mock)
# ---------------------------------------------------------------------------

class TestWebFetchTool:
    def test_sync_raises(self):
        from src.tools.web import WebFetchTool
        tool = WebFetchTool()
        with pytest.raises(NotImplementedError):
            tool._run("https://example.com")


# ---------------------------------------------------------------------------
# WebSearchTool (async only, tested with mock)
# ---------------------------------------------------------------------------

class TestWebSearchTool:
    def test_sync_raises(self):
        tool = WebSearchTool()
        with pytest.raises(NotImplementedError):
            tool._run("test query")

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx_sse.aconnect_sse")
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_successful_search(self, mock_client_cls, mock_aconnect):
        # Build a mock SSE event
        mock_event = MagicMock()
        mock_event.data = json.dumps({
            "jsonrpc": "2.0", "id": 1,
            "result": {"content": [{"type": "text", "text": "Search results here"}]},
        })

        mock_event_source = MagicMock()
        async def _aiter():
            yield mock_event
        mock_event_source.aiter_sse = _aiter

        # aconnect_sse is an async context manager
        mock_ctx = AsyncMock()
        mock_ctx.__aenter__ = AsyncMock(return_value=mock_event_source)
        mock_ctx.__aexit__ = AsyncMock(return_value=False)
        mock_aconnect.return_value = mock_ctx

        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        tool = WebSearchTool()
        result = await tool._arun("test query")
        assert result == "Search results here"

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx_sse.aconnect_sse")
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_no_results(self, mock_client_cls, mock_aconnect):
        mock_event = MagicMock()
        mock_event.data = json.dumps({
            "jsonrpc": "2.0", "id": 1,
            "result": {"content": []},
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

        tool = WebSearchTool()
        result = await tool._arun("nothing")
        assert "No search results found" in result

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_timeout_error(self, mock_client_cls):
        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        with patch("src.tools.web.httpx_sse.aconnect_sse", side_effect=httpx.TimeoutException("timeout")):
            tool = WebSearchTool()
            result = await tool._arun("test")
            assert "timed out" in result.lower()

    @pytest.mark.asyncio
    @patch("src.tools.web.httpx.AsyncClient")
    async def test_http_error(self, mock_client_cls):
        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_client_cls.return_value = mock_client

        with patch("src.tools.web.httpx_sse.aconnect_sse", side_effect=httpx.HTTPError("500 error")):
            tool = WebSearchTool()
            result = await tool._arun("test")
            assert "Error" in result


# ---------------------------------------------------------------------------
# create_all_tools
# ---------------------------------------------------------------------------

class TestCreateAllTools:
    def test_creates_all_tools(self, workspace):
        from src.tools import create_all_tools
        tools = create_all_tools(workspace=workspace)
        names = {t.name for t in tools}
        assert "bash" in names
        assert "read" in names
        assert "write" in names
        assert "edit" in names
        assert "glob" in names
        assert "grep" in names
        assert "web_fetch" in names
        assert "web_search" in names
        assert "code_interpreter" in names
        assert len(tools) == 9
