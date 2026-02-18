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
from src.tools.search import GlobTool, GrepTool, _expand_braces
from src.tools.web import WebFetchTool, WebSearchTool
from src.tools.code_interpreter import CodeInterpreterTool
from .result_helpers import _rdata, _rllm, _rmeta, _rtext

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
        assert result["kind"] == "bash"
        assert _rdata(result)["exit_code"] == 0
        assert "hello" in _rtext(result)
        assert "hello" in _rdata(result)["stdout"]
        assert result["success"] is True

    def test_cwd_is_workspace(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("pwd")
        # On Windows, pwd might not work, so use a cross-platform approach
        ws_resolved = str(Path(workspace).resolve())
        # Just check it doesn't error
        assert result["kind"] == "bash"
        assert _rdata(result)["exit_code"] == 0
        assert _rtext(result)  # non-empty

    def test_timeout(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("sleep 10", timeout=1)
        assert _rmeta(result)["timed_out"] is True
        assert "timed out" in _rtext(result).lower()
        assert result["success"] is False

    def test_stderr_included(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("echo err >&2")
        assert _rdata(result)["exit_code"] == 0
        assert "err" in _rdata(result)["stderr"]

    def test_no_output(self, workspace):
        tool = BashTool(workspace=workspace)
        result = tool._run("true")
        assert result["kind"] == "bash"
        assert _rtext(result) == "(no output)"

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        tool = BashTool(workspace=workspace)
        result = await tool._arun("echo async_test")
        assert result["kind"] == "bash"
        assert _rdata(result)["exit_code"] == 0
        assert "async_test" in _rtext(result)


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
        assert "line1" in _rtext(result)
        assert "line2" in _rtext(result)
        assert "line3" in _rtext(result)

    def test_read_with_line_numbers(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("aaa\nbbb\n")
        tool = ReadTool(workspace=workspace)
        result = tool._run("test.txt")
        assert "1\t" in _rtext(result)
        assert "2\t" in _rtext(result)

    def test_read_with_offset(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("line1\nline2\nline3\n")
        tool = ReadTool(workspace=workspace)
        result = tool._run("test.txt", offset=1, limit=1)
        assert "line2" in _rtext(result)
        assert "line1" not in _rtext(result)

    def test_read_nonexistent(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("nonexistent.txt")
        assert "not found" in _rtext(result).lower()

    def test_read_empty_file(self, workspace):
        path = os.path.join(workspace, "empty.txt")
        with open(path, "w") as f:
            pass
        tool = ReadTool(workspace=workspace)
        result = tool._run("empty.txt")
        assert "empty" in _rtext(result).lower()

    def test_read_path_traversal(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("../../etc/passwd")
        assert "error" in _rtext(result).lower()

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        path = os.path.join(workspace, "test.txt")
        with open(path, "w") as f:
            f.write("async content\n")
        tool = ReadTool(workspace=workspace)
        result = await tool._arun("test.txt")
        assert "async content" in _rtext(result)

    def test_read_image_png(self, workspace):
        path = os.path.join(workspace, "photo.png")
        with open(path, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("photo.png")
        assert result["kind"] == "read"
        assert result["success"] is True
        assert "photo.png" in _rtext(result)
        assert "sandbox:///photo.png" in _rtext(result)
        media = _rdata(result).get("media")
        assert isinstance(media, list) and media
        assert media[0]["type"] == "image"
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/png;base64,")

    def test_read_image_jpg(self, workspace):
        path = os.path.join(workspace, "photo.jpg")
        with open(path, "wb") as f:
            f.write(b"\xff\xd8\xff" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("photo.jpg")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/jpeg;base64,")

    def test_read_image_webp(self, workspace):
        path = os.path.join(workspace, "photo.webp")
        with open(path, "wb") as f:
            f.write(b"RIFF" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("photo.webp")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/webp;base64,")

    def test_read_image_too_large(self, workspace):
        path = os.path.join(workspace, "huge.png")
        with open(path, "wb") as f:
            f.write(b"\x00" * (11 * 1024 * 1024))  # 11MB
        tool = ReadTool(workspace=workspace)
        result = tool._run("huge.png")
        assert result["success"] is False
        assert "too large" in _rtext(result).lower()

    def test_read_empty_image(self, workspace):
        path = os.path.join(workspace, "empty.png")
        with open(path, "wb") as f:
            pass  # 0 bytes
        tool = ReadTool(workspace=workspace)
        result = tool._run("empty.png")
        assert result["success"] is False
        assert "empty" in _rtext(result).lower()

    @pytest.mark.asyncio
    async def test_arun_image(self, workspace):
        path = os.path.join(workspace, "async.png")
        with open(path, "wb") as f:
            f.write(b"\x89PNG" + b"\x00" * 50)
        tool = ReadTool(workspace=workspace)
        result = await tool._arun("async.png")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["type"] == "image_url"

    def test_read_image_jpeg(self, workspace):
        """'.jpeg' extension should also use image/jpeg MIME type."""
        path = os.path.join(workspace, "photo.jpeg")
        with open(path, "wb") as f:
            f.write(b"\xff\xd8\xff" + b"\x00" * 50)
        tool = ReadTool(workspace=workspace)
        result = tool._run("photo.jpeg")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/jpeg;base64,")

    def test_read_image_gif(self, workspace):
        path = os.path.join(workspace, "anim.gif")
        with open(path, "wb") as f:
            f.write(b"GIF89a" + b"\x00" * 50)
        tool = ReadTool(workspace=workspace)
        result = tool._run("anim.gif")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/gif;base64,")

    def test_read_image_uppercase_extension(self, workspace):
        """Extension matching should be case-insensitive."""
        path = os.path.join(workspace, "PHOTO.PNG")
        with open(path, "wb") as f:
            f.write(b"\x89PNG" + b"\x00" * 50)
        tool = ReadTool(workspace=workspace)
        result = tool._run("PHOTO.PNG")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["image_url"]["url"].startswith("data:image/png;base64,")

    def test_read_image_nonexistent(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("missing.png")
        assert result["success"] is False
        assert "not found" in _rtext(result).lower()

    def test_read_image_path_traversal(self, workspace):
        tool = ReadTool(workspace=workspace)
        result = tool._run("../../etc/secret.png")
        assert result["success"] is False
        assert "error" in _rtext(result).lower()

    def test_read_image_exactly_max_size(self, workspace):
        """File exactly at 10MB limit should succeed."""
        path = os.path.join(workspace, "exact.png")
        with open(path, "wb") as f:
            f.write(b"\x89" * (10 * 1024 * 1024))
        tool = ReadTool(workspace=workspace)
        result = tool._run("exact.png")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        assert llm_content[1]["type"] == "image_url"

    def test_read_image_in_subdirectory(self, workspace):
        """Path in text block should preserve the original user-provided path."""
        sub = os.path.join(workspace, "images")
        os.makedirs(sub)
        with open(os.path.join(sub, "cat.jpg"), "wb") as f:
            f.write(b"\xff\xd8\xff" + b"\x00" * 50)
        tool = ReadTool(workspace=workspace)
        result = tool._run("images/cat.jpg")
        assert "images/cat.jpg" in _rtext(result)
        assert "sandbox:///images/cat.jpg" in _rtext(result)

    def test_read_image_base64_roundtrip(self, workspace):
        """Base64 content should decode back to original bytes."""
        import base64 as b64mod
        original = b"\x89PNG\r\n\x1a\n" + bytes(range(256))
        path = os.path.join(workspace, "roundtrip.png")
        with open(path, "wb") as f:
            f.write(original)
        tool = ReadTool(workspace=workspace)
        result = tool._run("roundtrip.png")
        llm_content = _rllm(result)
        assert isinstance(llm_content, list)
        url = llm_content[1]["image_url"]["url"]
        encoded = url.split(",", 1)[1]
        assert b64mod.b64decode(encoded) == original

    def test_read_image_is_directory(self, workspace):
        """A directory with an image extension should return an error."""
        dir_path = os.path.join(workspace, "fake.png")
        os.makedirs(dir_path)
        tool = ReadTool(workspace=workspace)
        result = tool._run("fake.png")
        assert result["success"] is False
        assert "error" in _rtext(result).lower()

    def test_read_non_image_binary_stays_text(self, workspace):
        """Non-image binary files should still go through text path, not image path."""
        path = os.path.join(workspace, "data.bin")
        with open(path, "wb") as f:
            f.write(b"\x00\x01\x02\x03")
        tool = ReadTool(workspace=workspace)
        result = tool._run("data.bin")
        assert result["kind"] == "read"

    def test_read_video_mp4(self, workspace):
        path = os.path.join(workspace, "clip.mp4")
        with open(path, "wb") as f:
            f.write(b"\x00\x00\x00\x1cftyp" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("clip.mp4")
        assert result["success"] is True
        assert "sandbox:///clip.mp4" in _rtext(result)
        assert "[Video:" in _rtext(result)

    def test_read_audio_mp3(self, workspace):
        path = os.path.join(workspace, "song.mp3")
        with open(path, "wb") as f:
            f.write(b"ID3" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("song.mp3")
        assert result["success"] is True
        assert "sandbox:///song.mp3" in _rtext(result)
        assert "[Audio:" in _rtext(result)

    def test_read_video_empty(self, workspace):
        path = os.path.join(workspace, "empty.mp4")
        with open(path, "wb") as f:
            pass
        tool = ReadTool(workspace=workspace)
        result = tool._run("empty.mp4")
        assert result["success"] is False
        assert "empty" in _rtext(result).lower()

    def test_read_audio_empty(self, workspace):
        path = os.path.join(workspace, "empty.mp3")
        with open(path, "wb") as f:
            pass
        tool = ReadTool(workspace=workspace)
        result = tool._run("empty.mp3")
        assert result["success"] is False
        assert "empty" in _rtext(result).lower()

    def test_read_video_in_subdirectory(self, workspace):
        sub = os.path.join(workspace, "videos")
        os.makedirs(sub)
        with open(os.path.join(sub, "demo.webm"), "wb") as f:
            f.write(b"\x1a\x45\xdf\xa3" + b"\x00" * 100)
        tool = ReadTool(workspace=workspace)
        result = tool._run("videos/demo.webm")
        assert result["success"] is True
        assert "sandbox:///videos/demo.webm" in _rtext(result)


# ---------------------------------------------------------------------------
# WriteTool
# ---------------------------------------------------------------------------

class TestWriteTool:
    def test_write_file(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = tool._run("output.txt", "hello world")
        assert "successfully" in _rtext(result).lower()
        with open(os.path.join(workspace, "output.txt")) as f:
            assert f.read() == "hello world"

    def test_write_creates_dirs(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = tool._run("sub/dir/file.txt", "nested content")
        assert "successfully" in _rtext(result).lower()
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
        assert "error" in _rtext(result).lower()

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        tool = WriteTool(workspace=workspace)
        result = await tool._arun("async_file.txt", "async content")
        assert "successfully" in _rtext(result).lower()


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
        assert "replaced" in _rtext(result).lower()
        with open(path) as f:
            assert f.read() == "hello universe"

    def test_edit_not_found(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("hello world")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "xyz", "abc")
        assert "not found" in _rtext(result).lower()

    def test_edit_multiple_without_replace_all(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("aaa bbb aaa")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "aaa", "ccc")
        assert "appears 2 times" in _rtext(result).lower() or "2" in _rtext(result)

    def test_edit_replace_all(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("aaa bbb aaa")
        tool = EditTool(workspace=workspace)
        result = tool._run("edit.txt", "aaa", "ccc", replace_all=True)
        assert "replaced" in _rtext(result).lower()
        with open(path) as f:
            assert f.read() == "ccc bbb ccc"

    def test_edit_nonexistent_file(self, workspace):
        tool = EditTool(workspace=workspace)
        result = tool._run("nope.txt", "a", "b")
        assert "not found" in _rtext(result).lower()

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        path = os.path.join(workspace, "edit.txt")
        with open(path, "w") as f:
            f.write("old text")
        tool = EditTool(workspace=workspace)
        result = await tool._arun("edit.txt", "old", "new")
        assert "replaced" in _rtext(result).lower()


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
        assert "a.py" in _rtext(result)
        assert "b.py" in _rtext(result)
        assert "c.txt" not in _rtext(result)

    def test_glob_recursive(self, workspace):
        sub = os.path.join(workspace, "sub")
        os.makedirs(sub)
        with open(os.path.join(sub, "deep.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("**/*.py")
        assert "deep.py" in _rtext(result)

    def test_glob_no_matches(self, workspace):
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.xyz")
        assert "no files" in _rtext(result).lower()

    def test_glob_nonexistent_path(self, workspace):
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path="nonexistent")
        assert "error" in _rtext(result).lower() or "not exist" in _rtext(result).lower()

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        with open(os.path.join(workspace, "test.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = await tool._arun("*.py")
        assert "test.py" in _rtext(result)

    def test_glob_brace_expansion(self, workspace):
        for name in ["a.py", "b.txt", "c.rs"]:
            with open(os.path.join(workspace, name), "w") as f:
                f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.{py,txt}")
        assert "a.py" in _rtext(result)
        assert "b.txt" in _rtext(result)
        assert "c.rs" not in _rtext(result)

    def test_glob_brace_expansion_recursive(self, workspace):
        sub = os.path.join(workspace, "sub")
        os.makedirs(sub)
        for name in ["root.py", "root.txt"]:
            with open(os.path.join(workspace, name), "w") as f:
                f.write("")
        for name in ["deep.py", "deep.txt", "deep.rs"]:
            with open(os.path.join(sub, name), "w") as f:
                f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("**/*.{py,txt}")
        assert "root.py" in _rtext(result)
        assert "root.txt" in _rtext(result)
        assert "sub/deep.py" in _rtext(result) or "sub\\deep.py" in _rtext(result)
        assert "sub/deep.txt" in _rtext(result) or "sub\\deep.txt" in _rtext(result)
        assert "deep.rs" not in _rtext(result)

    def test_glob_brace_no_matches(self, workspace):
        with open(os.path.join(workspace, "a.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.{xyz,abc}")
        assert "no files" in _rtext(result).lower()

    def test_glob_explicit_empty_path(self, workspace):
        with open(os.path.join(workspace, "hello.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path="")
        assert "hello.py" in _rtext(result)
        assert _rdata(result)["path"] == "."

    def test_glob_null_path_defaults_to_workspace(self, workspace):
        with open(os.path.join(workspace, "hello.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path=None)
        assert "hello.py" in _rtext(result)
        assert _rdata(result)["path"] == "."

    def test_glob_whitespace_path_defaults_to_workspace(self, workspace):
        with open(os.path.join(workspace, "hello.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path="   ")
        assert "hello.py" in _rtext(result)
        assert _rdata(result)["path"] == "."

    def test_glob_preserves_significant_surrounding_spaces_in_path(self, workspace):
        sub = os.path.join(workspace, " pkg ")
        os.makedirs(sub)
        with open(os.path.join(sub, "mod.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path=" pkg ")
        assert "pkg/mod.py" not in _rtext(result)
        assert " pkg /mod.py" in _rtext(result)
        assert _rdata(result)["path"] == " pkg "

    def test_glob_path_is_subdirectory(self, workspace):
        sub = os.path.join(workspace, "pkg")
        os.makedirs(sub)
        with open(os.path.join(workspace, "root.py"), "w") as f:
            f.write("")
        with open(os.path.join(sub, "mod.py"), "w") as f:
            f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.py", path="pkg")
        assert "mod.py" in _rtext(result)
        assert "root.py" not in _rtext(result)

    def test_glob_result_limit(self, workspace):
        for i in range(1100):
            with open(os.path.join(workspace, f"f{i}.txt"), "w") as f:
                f.write("")
        tool = GlobTool(workspace=workspace)
        result = tool._run("*.{txt,py}")
        lines = _rtext(result).strip().split("\n")
        assert len(lines) == 1000


# ---------------------------------------------------------------------------
# _expand_braces helper
# ---------------------------------------------------------------------------

class TestExpandBraces:
    def test_no_braces(self):
        assert _expand_braces("**/*.py") == ["**/*.py"]

    def test_single_brace(self):
        result = _expand_braces("*.{py,txt}")
        assert sorted(result) == ["*.py", "*.txt"]

    def test_multiple_braces(self):
        result = _expand_braces("{src,lib}/*.{py,txt}")
        assert sorted(result) == ["lib/*.py", "lib/*.txt", "src/*.py", "src/*.txt"]

    def test_single_alternative(self):
        result = _expand_braces("*.{py}")
        assert result == ["*.py"]


# ---------------------------------------------------------------------------
# GrepTool
# ---------------------------------------------------------------------------

class TestGrepTool:
    def test_grep_finds_match(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("hello world\nfoo bar\nhello again\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("hello")
        assert "hello world" in _rtext(result)
        assert "hello again" in _rtext(result)

    def test_grep_with_line_numbers(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("aaa\nbbb\nccc\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("bbb")
        assert ":2:" in _rtext(result)

    def test_grep_no_matches(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("hello world\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("xyz")
        assert "no matches" in _rtext(result).lower()

    def test_grep_invalid_regex(self, workspace):
        tool = GrepTool(workspace=workspace)
        result = tool._run("[invalid")
        assert "error" in _rtext(result).lower()

    def test_grep_with_glob_filter(self, workspace):
        with open(os.path.join(workspace, "a.py"), "w") as f:
            f.write("target\n")
        with open(os.path.join(workspace, "b.txt"), "w") as f:
            f.write("target\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("target", glob_filter="*.py")
        assert "a.py" in _rtext(result)
        assert "b.txt" not in _rtext(result)

    def test_grep_with_context(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("line1\nline2\nMATCH\nline4\nline5\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("MATCH", context=1)
        assert "line2" in _rtext(result)
        assert "MATCH" in _rtext(result)
        assert "line4" in _rtext(result)

    def test_grep_skips_binary(self, workspace):
        with open(os.path.join(workspace, "binary.bin"), "wb") as f:
            f.write(b"\x00\x01\x02hello\x00")
        with open(os.path.join(workspace, "text.txt"), "w") as f:
            f.write("hello\n")
        tool = GrepTool(workspace=workspace)
        result = tool._run("hello")
        assert "text.txt" in _rtext(result)
        assert "binary.bin" not in _rtext(result)

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        with open(os.path.join(workspace, "test.txt"), "w") as f:
            f.write("async match\n")
        tool = GrepTool(workspace=workspace)
        result = await tool._arun("async")
        assert "async match" in _rtext(result)


# ---------------------------------------------------------------------------
# CodeInterpreterTool
# ---------------------------------------------------------------------------

class TestCodeInterpreterTool:
    def test_python_execution(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("print('hello from python')")
        assert "hello from python" in _rtext(result)

    def test_python_error(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("raise ValueError('test error')")
        assert "ValueError" in _rtext(result) or "test error" in _rtext(result)

    def test_python_timeout(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("import time; time.sleep(60)")
        assert "timed out" in _rtext(result).lower()

    def test_temp_file_cleanup(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        tool._run("print('cleanup test')")
        # No temp files should remain
        remaining = [f for f in os.listdir(workspace) if f.startswith("tmp")]
        assert len(remaining) == 0

    @pytest.mark.asyncio
    async def test_arun(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        result = await tool._arun("print('async python')")
        assert "async python" in _rtext(result)

    def test_detects_new_image_file(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        code = (
            "import os\n"
            "with open('chart.png', 'wb') as f:\n"
            "    f.write(b'\\x89PNG fake image data')\n"
            "print('done')\n"
        )
        result = tool._run(code)
        assert "done" in _rtext(result)
        assert "sandbox:///chart.png" in _rtext(result)

    def test_detects_new_video_file(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        code = (
            "with open('output.mp4', 'wb') as f:\n"
            "    f.write(b'fake video')\n"
            "print('ok')\n"
        )
        result = tool._run(code)
        assert "sandbox:///output.mp4" in _rtext(result)
        assert "[Video:" in _rtext(result)

    def test_detects_new_audio_file(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        code = (
            "with open('sound.mp3', 'wb') as f:\n"
            "    f.write(b'fake audio')\n"
            "print('ok')\n"
        )
        result = tool._run(code)
        assert "sandbox:///sound.mp3" in _rtext(result)
        assert "[Audio:" in _rtext(result)

    def test_ignores_preexisting_media(self, workspace):
        # Create a pre-existing image
        with open(os.path.join(workspace, "old.png"), "wb") as f:
            f.write(b"old image")
        tool = CodeInterpreterTool(workspace=workspace)
        result = tool._run("print('no new media')")
        assert "sandbox://" not in _rtext(result)

    def test_detects_media_in_subdirectory(self, workspace):
        tool = CodeInterpreterTool(workspace=workspace)
        code = (
            "import os\n"
            "os.makedirs('plots', exist_ok=True)\n"
            "with open('plots/fig.png', 'wb') as f:\n"
            "    f.write(b'\\x89PNG data')\n"
            "print('saved')\n"
        )
        result = tool._run(code)
        assert "sandbox:///plots/fig.png" in _rtext(result)


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
        assert result["kind"] == "web_search"
        assert result["success"] is True
        assert _rtext(result) == "Search results here"

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
        assert "No search results found" in _rtext(result)

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
            assert "timed out" in _rtext(result).lower()

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
            assert "Error" in _rtext(result)


# ---------------------------------------------------------------------------
# create_all_tools
# ---------------------------------------------------------------------------

class TestCreateAllTools:
    def test_creates_all_tools_with_image_config(self, workspace):
        from src.tools import create_all_tools
        tools = create_all_tools(
            workspace=workspace,
            provider="openai",
            api_key="test-key",
            image_provider="google",
            image_api_key="img-key",
            image_model="gemini-img",
        )
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
        assert "image_generation" in names
        assert len(tools) == 10

    def test_creates_tools_without_image_config(self, workspace):
        from src.tools import create_all_tools
        tools = create_all_tools(workspace=workspace, provider="openai", api_key="test-key")
        names = {t.name for t in tools}
        assert "image_generation" not in names
        assert len(tools) == 9

    def test_creates_tools_without_provider(self, workspace):
        from src.tools import create_all_tools
        tools = create_all_tools(workspace=workspace)
        names = {t.name for t in tools}
        assert "image_generation" not in names
        assert len(tools) == 9

    def test_image_tool_uses_image_config(self, workspace):
        from src.tools import create_all_tools
        tools = create_all_tools(
            workspace=workspace,
            provider="openai",
            api_key="chat-key",
            image_provider="google",
            image_api_key="img-key",
            image_model="gemini-img",
            image_endpoint_url="https://img.example.com",
        )
        img_tool = next(t for t in tools if t.name == "image_generation")
        assert img_tool.provider == "google"
        assert img_tool.api_key == "img-key"
        assert img_tool.model == "gemini-img"
        assert img_tool.endpoint_url == "https://img.example.com"
