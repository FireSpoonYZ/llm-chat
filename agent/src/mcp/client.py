"""MCP client wrapper for connecting to MCP servers."""

from __future__ import annotations

import json
import logging
from typing import Any

from mcp import ClientSession, types
from mcp.client.stdio import StdioServerParameters, stdio_client

logger = logging.getLogger(__name__)


class McpClient:
    """Wraps an MCP client session for a single server."""

    def __init__(
        self,
        name: str,
        command: str,
        args: list[str] | None = None,
        env: dict[str, str] | None = None,
    ) -> None:
        self.name = name
        self.command = command
        self.args = args or []
        self.env = env or {}
        self._session: ClientSession | None = None
        self._read = None
        self._write = None
        self._stdio_cm = None
        self._session_cm = None
        self._tools: list[types.Tool] = []

    @property
    def is_connected(self) -> bool:
        return self._session is not None

    async def connect(self) -> None:
        """Start the MCP server process and initialize the session."""
        params = StdioServerParameters(
            command=self.command,
            args=self.args,
            env=self.env if self.env else None,
        )
        self._stdio_cm = stdio_client(params)
        self._read, self._write = await self._stdio_cm.__aenter__()
        self._session_cm = ClientSession(self._read, self._write)
        self._session = await self._session_cm.__aenter__()
        await self._session.initialize()
        logger.info("MCP server '%s' connected and initialized", self.name)

    async def disconnect(self) -> None:
        """Shut down the MCP session and server process."""
        if self._session_cm:
            try:
                await self._session_cm.__aexit__(None, None, None)
            except Exception:
                pass
            self._session_cm = None
            self._session = None
        if self._stdio_cm:
            try:
                await self._stdio_cm.__aexit__(None, None, None)
            except Exception:
                pass
            self._stdio_cm = None
        self._read = None
        self._write = None
        self._tools = []
        logger.info("MCP server '%s' disconnected", self.name)

    async def list_tools(self) -> list[types.Tool]:
        """List tools available from this MCP server."""
        if not self._session:
            raise RuntimeError(f"MCP server '{self.name}' is not connected")
        result = await self._session.list_tools()
        self._tools = result.tools
        return self._tools

    async def call_tool(
        self, tool_name: str, arguments: dict[str, Any] | None = None
    ) -> str:
        """Call a tool on this MCP server and return the text result."""
        if not self._session:
            raise RuntimeError(f"MCP server '{self.name}' is not connected")
        result = await self._session.call_tool(tool_name, arguments=arguments or {})
        parts: list[str] = []
        for content in result.content:
            if isinstance(content, types.TextContent):
                parts.append(content.text)
            elif isinstance(content, types.ImageContent):
                parts.append(f"[Image: {content.mimeType}]")
            else:
                parts.append(str(content))
        return "\n".join(parts) if parts else "(no output)"

    def get_tool_schemas(self) -> list[dict[str, Any]]:
        """Return JSON-serializable tool schemas for the cached tools."""
        schemas = []
        for tool in self._tools:
            schema: dict[str, Any] = {
                "name": f"mcp_{self.name}_{tool.name}",
                "description": tool.description or f"MCP tool: {tool.name}",
                "mcp_server": self.name,
                "mcp_tool_name": tool.name,
            }
            if tool.inputSchema:
                schema["input_schema"] = tool.inputSchema
            schemas.append(schema)
        return schemas
