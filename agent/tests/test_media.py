"""Tests for the shared _media module."""

from __future__ import annotations

from pathlib import Path

from src.tools._media import (
    ALL_MEDIA_EXTENSIONS,
    MEDIA_TYPES,
    classify_media,
    format_sandbox_ref,
    sandbox_url,
)


class TestClassifyMedia:
    def test_image_extensions(self):
        for ext in (".png", ".jpg", ".jpeg", ".gif", ".webp"):
            assert classify_media(ext) == "image"

    def test_video_extensions(self):
        for ext in (".mp4", ".webm", ".mov"):
            assert classify_media(ext) == "video"

    def test_audio_extensions(self):
        for ext in (".mp3", ".wav", ".ogg", ".m4a"):
            assert classify_media(ext) == "audio"

    def test_case_insensitive(self):
        assert classify_media(".PNG") == "image"
        assert classify_media(".Mp4") == "video"
        assert classify_media(".WAV") == "audio"

    def test_unknown_returns_none(self):
        assert classify_media(".txt") is None
        assert classify_media(".py") is None
        assert classify_media(".bin") is None
        assert classify_media("") is None


class TestSandboxUrl:
    def test_simple_path(self):
        resolved = Path("/workspace/photo.png")
        assert sandbox_url(resolved, "/workspace") == "sandbox:///photo.png"

    def test_subdirectory(self):
        resolved = Path("/workspace/images/cat.jpg")
        assert sandbox_url(resolved, "/workspace") == "sandbox:///images/cat.jpg"

    def test_deep_path(self):
        resolved = Path("/workspace/a/b/c/file.mp4")
        assert sandbox_url(resolved, "/workspace") == "sandbox:///a/b/c/file.mp4"


class TestFormatSandboxRef:
    def test_image(self):
        result = format_sandbox_ref("photo.png", "image")
        assert result == "![photo.png](sandbox:///photo.png)"

    def test_video(self):
        result = format_sandbox_ref("clip.mp4", "video")
        assert result == "[Video: clip.mp4](sandbox:///clip.mp4)"

    def test_audio(self):
        result = format_sandbox_ref("song.mp3", "audio")
        assert result == "[Audio: song.mp3](sandbox:///song.mp3)"

    def test_subdirectory_image(self):
        result = format_sandbox_ref("images/cat.jpg", "image")
        assert result == "![cat.jpg](sandbox:///images/cat.jpg)"

    def test_subdirectory_video(self):
        result = format_sandbox_ref("output/vid.webm", "video")
        assert result == "[Video: vid.webm](sandbox:///output/vid.webm)"


class TestAllMediaExtensions:
    def test_is_union_of_all_types(self):
        expected = frozenset()
        for exts in MEDIA_TYPES.values():
            expected = expected | exts
        assert ALL_MEDIA_EXTENSIONS == expected

    def test_contains_known_extensions(self):
        for ext in (".png", ".mp4", ".mp3"):
            assert ext in ALL_MEDIA_EXTENSIONS
