"""Tests for the system prompt presets module."""

from __future__ import annotations

from pathlib import Path

import pytest

from src.prompts.presets import (
    BUILTIN_PRESETS,
    PromptPreset,
    get_preset,
    list_presets,
)


class TestPromptPreset:
    def test_preset_has_required_fields(self):
        preset = PromptPreset(
            id="test", name="Test", description="A test preset", content="Hello"
        )
        assert preset.id == "test"
        assert preset.name == "Test"
        assert preset.description == "A test preset"
        assert preset.content == "Hello"

    def test_preset_content_not_empty(self):
        with pytest.raises(ValueError, match="empty"):
            PromptPreset(id="bad", name="Bad", description="Bad", content="")

    def test_preset_content_whitespace_only(self):
        with pytest.raises(ValueError, match="empty"):
            PromptPreset(id="bad", name="Bad", description="Bad", content="   ")


class TestBuiltinPresets:
    def test_default_preset_exists(self):
        assert "default" in BUILTIN_PRESETS

    def test_claude_ai_preset_exists(self):
        assert "claude-ai" in BUILTIN_PRESETS

    def test_claude_ai_preset_contains_behavior(self):
        preset = BUILTIN_PRESETS["claude-ai"]
        assert "<claude_behavior>" in preset.content

    def test_claude_code_preset_exists(self):
        assert "claude-code" in BUILTIN_PRESETS

    def test_claude_code_preset_contains_behavior(self):
        preset = BUILTIN_PRESETS["claude-code"]
        assert "<claude_code_behavior>" in preset.content

    def test_claude_cowork_preset_exists(self):
        assert "claude-cowork" in BUILTIN_PRESETS

    def test_claude_cowork_preset_contains_behavior(self):
        preset = BUILTIN_PRESETS["claude-cowork"]
        assert "<behavior_instructions>" in preset.content
        assert "Available Tools" in preset.content
        assert "Claude uses bash, read, write, edit" not in preset.content

    def test_all_presets_have_unique_ids(self):
        ids = [p.id for p in BUILTIN_PRESETS.values()]
        assert len(ids) == len(set(ids))

    def test_get_preset_by_id(self):
        preset = get_preset("default")
        assert preset is not None
        assert preset.id == "default"

    def test_get_nonexistent_preset_returns_none(self):
        assert get_preset("nonexistent") is None

    def test_list_presets_returns_all(self):
        presets = list_presets()
        assert len(presets) == len(BUILTIN_PRESETS)
        ids = {p.id for p in presets}
        assert "default" in ids
        assert "claude-ai" in ids
        assert "claude-code" in ids
        assert "claude-cowork" in ids

    def test_claude_cowork_content_matches_backend_template(self):
        backend_cowork = (
            Path(__file__).resolve().parents[2]
            / "backend"
            / "src"
            / "prompts_content"
            / "claude_cowork.txt"
        ).read_text(encoding="utf-8")
        assert BUILTIN_PRESETS["claude-cowork"].content.strip() == backend_cowork.strip()
