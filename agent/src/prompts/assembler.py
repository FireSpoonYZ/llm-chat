"""Runtime system prompt assembler."""

from __future__ import annotations

from typing import Any

from .base import BASE_PROMPT
from .mcp import mcp_instructions
from .tools import format_tool_descriptions


def assemble_system_prompt(
    tool_names: list[str],
    mcp_servers: list[dict[str, Any]] | None = None,
    user_override: str | None = None,
) -> str:
    """Assemble the full system prompt from modular fragments.

    Args:
        tool_names: Names of enabled built-in tools.
        mcp_servers: MCP server configs (for MCP instruction fragment).
        user_override: Optional user-provided prompt additions.

    Returns:
        The assembled system prompt string.
    """
    parts = [BASE_PROMPT]

    if tool_names:
        parts.append(format_tool_descriptions(tool_names))

    if mcp_servers:
        mcp_text = mcp_instructions(mcp_servers)
        if mcp_text:
            parts.append(mcp_text)

    if user_override:
        parts.append(f"\n# Additional Instructions\n{user_override}")

    return "\n\n".join(parts)
