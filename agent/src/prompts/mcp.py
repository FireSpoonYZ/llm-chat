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
        "Their tools are prefixed with `mcp_{server_name}_`.\n",
    ]

    for server in mcp_servers:
        name = server.get("name", "unknown")
        desc = server.get("description", "")
        parts.append(f"## {name}")
        if desc:
            parts.append(f"{desc}\n")
        parts.append(
            f"- Tools from this server are prefixed with `mcp_{name}_`"
        )
        parts.append(
            "- Check the tool's schema before calling it"
        )
        parts.append("")

    return "\n".join(parts)
