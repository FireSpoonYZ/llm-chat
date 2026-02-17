"""MCP instruction fragments for system prompt composition."""

from __future__ import annotations

from typing import Any


def mcp_instructions(mcp_servers: list[dict[str, Any]]) -> str:
    """Generate MCP-specific instructions for the system prompt.

    Args:
        mcp_servers: List of MCP server configs with name, description, etc.

    Returns:
        Instruction text for the system prompt, or empty string if no servers.
    """
    if not mcp_servers:
        return ""

    parts = [
        "\n# MCP Server Tools\n",
        "The following MCP (Model Context Protocol) servers are available. "
        "Their tools use the original names defined by each server.\n",
        "General guidelines for MCP tools:\n"
        "- Check the tool's input schema before calling it to ensure correct parameters.\n"
        "- If an MCP tool call fails, report the error clearly and do not retry "
        "with the same parameters.\n"
        "- When both a built-in tool and an MCP tool can accomplish a task, "
        "prefer the built-in tool unless the MCP tool offers specific advantages.\n",
    ]

    for server in mcp_servers:
        name = server.get("name", "unknown")
        desc = server.get("description", "")
        parts.append(f"## {name}")
        if desc:
            parts.append(f"{desc}")
        parts.append("")

    return "\n".join(parts)
