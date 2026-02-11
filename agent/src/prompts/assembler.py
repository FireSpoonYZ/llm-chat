"""Runtime system prompt assembler."""

from __future__ import annotations

from typing import Any

from .base import BASE_PROMPT
from .behaviors import (
    SAFETY_INSTRUCTIONS,
    TASK_EXECUTION_GUIDELINES,
    TOOL_USAGE_POLICY,
)
from .mcp import mcp_instructions
from .tools import format_tool_descriptions


def assemble_system_prompt(
    tool_names: list[str],
    mcp_servers: list[dict[str, Any]] | None = None,
    user_override: str | None = None,
    base_prompt: str | None = None,
) -> str:
    """Assemble the full system prompt from modular fragments.

    Order: base → behaviors (if tools) → tool descriptions → mcp → user override.
    """
    parts = [base_prompt if base_prompt is not None else BASE_PROMPT]

    if tool_names:
        parts.append(TOOL_USAGE_POLICY)
        parts.append(SAFETY_INSTRUCTIONS)
        parts.append(TASK_EXECUTION_GUIDELINES)
        parts.append(format_tool_descriptions(tool_names))

    if mcp_servers:
        mcp_text = mcp_instructions(mcp_servers)
        if mcp_text:
            parts.append(mcp_text)

    if user_override:
        parts.append(f"\n# Additional Instructions\n{user_override}")

    return "\n\n".join(parts)
