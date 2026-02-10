"""MCP server lifecycle manager and LangChain tool bridge."""

from __future__ import annotations

import json
import logging
from typing import Any, Type

from langchain_core.tools import BaseTool
from pydantic import BaseModel, ConfigDict, Field

from .client import McpClient

logger = logging.getLogger(__name__)


class McpToolInput(BaseModel):
    """Generic input schema for MCP tools."""
    arguments: str = Field(
        default="{}",
        description="JSON-encoded arguments to pass to the MCP tool.",
    )


class McpLangChainTool(BaseTool):
    """A LangChain tool that delegates to an MCP server tool."""

    name: str = ""
    description: str = ""
    args_schema: Type[BaseModel] = McpToolInput
    mcp_client: Any = None  # McpClient instance
    mcp_tool_name: str = ""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    def _run(self, arguments: str = "{}") -> str:
        raise NotImplementedError("MCP tools are async-only. Use _arun.")

    async def _arun(self, arguments: str = "{}") -> str:
        if self.mcp_client is None:
            return "Error: MCP client not available"
        try:
            args = json.loads(arguments) if arguments else {}
        except json.JSONDecodeError:
            return f"Error: invalid JSON arguments: {arguments}"
        try:
            return await self.mcp_client.call_tool(self.mcp_tool_name, args)
        except Exception as exc:
            return f"Error calling MCP tool '{self.mcp_tool_name}': {exc}"


class McpManager:
    """Manages multiple MCP server connections and creates LangChain tools."""

    def __init__(self) -> None:
        self._clients: dict[str, McpClient] = {}

    @property
    def connected_servers(self) -> list[str]:
        return [name for name, c in self._clients.items() if c.is_connected]

    async def add_server(
        self,
        name: str,
        command: str,
        args: list[str] | None = None,
        env: dict[str, str] | None = None,
    ) -> McpClient:
        """Add and connect to an MCP server."""
        if name in self._clients:
            await self.remove_server(name)

        client = McpClient(name=name, command=command, args=args, env=env)
        try:
            await client.connect()
            await client.list_tools()
            self._clients[name] = client
            logger.info(
                "MCP server '%s' added with %d tools",
                name, len(client._tools),
            )
        except Exception as exc:
            logger.error("Failed to connect to MCP server '%s': %s", name, exc)
            await client.disconnect()
            raise
        return client

    async def remove_server(self, name: str) -> None:
        """Disconnect and remove an MCP server."""
        client = self._clients.pop(name, None)
        if client:
            await client.disconnect()

    async def shutdown(self) -> None:
        """Disconnect all MCP servers."""
        for name in list(self._clients.keys()):
            await self.remove_server(name)

    def get_langchain_tools(self) -> list[BaseTool]:
        """Create LangChain tool wrappers for all connected MCP server tools."""
        tools: list[BaseTool] = []
        for name, client in self._clients.items():
            if not client.is_connected:
                continue
            for schema in client.get_tool_schemas():
                tool = McpLangChainTool(
                    name=schema["name"],
                    description=schema["description"],
                    mcp_client=client,
                    mcp_tool_name=schema["mcp_tool_name"],
                )
                tools.append(tool)
        return tools

    async def setup_from_config(
        self, mcp_servers: list[dict[str, Any]]
    ) -> list[BaseTool]:
        """Initialize MCP servers from config and return LangChain tools.

        Args:
            mcp_servers: List of server configs from the init message, each with
                keys: name, transport, command, args, env_vars.

        Returns:
            List of LangChain tools from all successfully connected servers.
        """
        for server_config in mcp_servers:
            name = server_config.get("name", "")
            transport = server_config.get("transport", "stdio")
            if transport != "stdio":
                logger.warning(
                    "Skipping MCP server '%s': transport '%s' not supported",
                    name, transport,
                )
                continue

            command = server_config.get("command", "")
            if not command:
                logger.warning("Skipping MCP server '%s': no command", name)
                continue

            args_raw = server_config.get("args")
            args: list[str] = []
            if isinstance(args_raw, str):
                try:
                    args = json.loads(args_raw)
                except json.JSONDecodeError:
                    args = args_raw.split()
            elif isinstance(args_raw, list):
                args = args_raw

            env_raw = server_config.get("env_vars")
            env: dict[str, str] = {}
            if isinstance(env_raw, str):
                try:
                    env = json.loads(env_raw)
                except json.JSONDecodeError:
                    pass
            elif isinstance(env_raw, dict):
                env = env_raw

            try:
                await self.add_server(name, command, args, env)
            except Exception as exc:
                logger.error(
                    "Failed to start MCP server '%s': %s", name, exc
                )

        return self.get_langchain_tools()
