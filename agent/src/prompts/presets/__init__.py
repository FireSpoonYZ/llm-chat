"""System prompt presets registry.

Provides built-in prompt presets and lookup functions.
"""

from __future__ import annotations

from dataclasses import dataclass

from .claude_ai import CLAUDE_AI_PRESET_CONTENT
from .claude_code import CLAUDE_CODE_PRESET_CONTENT
from .claude_cowork import CLAUDE_COWORK_PRESET_CONTENT
from .default import DEFAULT_PRESET_CONTENT


@dataclass(frozen=True)
class PromptPreset:
    """A system prompt preset."""

    id: str
    name: str
    description: str
    content: str

    def __post_init__(self) -> None:
        if not self.content.strip():
            raise ValueError("Preset content cannot be empty")


BUILTIN_PRESETS: dict[str, PromptPreset] = {
    "default": PromptPreset(
        id="default",
        name="Default",
        description="A concise general-purpose assistant prompt.",
        content=DEFAULT_PRESET_CONTENT,
    ),
    "claude-ai": PromptPreset(
        id="claude-ai",
        name="Claude AI",
        description="Comprehensive prompt modeled after Claude.ai behavior guidelines.",
        content=CLAUDE_AI_PRESET_CONTENT,
    ),
    "claude-code": PromptPreset(
        id="claude-code",
        name="Claude Code",
        description="Software engineering focused prompt based on Claude Code CLI.",
        content=CLAUDE_CODE_PRESET_CONTENT,
    ),
    "claude-cowork": PromptPreset(
        id="claude-cowork",
        name="Claude Cowork",
        description="Task-execution focused prompt inspired by Claude Cowork style.",
        content=CLAUDE_COWORK_PRESET_CONTENT,
    ),
}


def get_preset(preset_id: str) -> PromptPreset | None:
    """Get a preset by ID, or None if not found."""
    return BUILTIN_PRESETS.get(preset_id)


def list_presets() -> list[PromptPreset]:
    """Return all built-in presets."""
    return list(BUILTIN_PRESETS.values())
