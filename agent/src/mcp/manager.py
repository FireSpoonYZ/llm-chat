"""MCP server lifecycle manager using langchain-mcp-adapters."""

from __future__ import annotations

import json
import logging
from typing import Any

from langchain_core.tools import BaseTool
from langchain_mcp_adapters.client import MultiServerMCPClient

logger = logging.getLogger(__name__)


class McpManager:
    """Manages MCP server connections via MultiServerMCPClient."""

    def __init__(self) -> None:
        self._client: MultiServerMCPClient | None = None
        self._server_names: list[str] = []

    @property
    def connected_servers(self) -> list[str]:
        return list(self._server_names)

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
        await self.shutdown()

        server_configs: dict[str, dict[str, Any]] = {}
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

            entry: dict[str, Any] = {
                "transport": "stdio",
                "command": command,
                "args": args,
            }
            if env:
                entry["env"] = env
            server_configs[name] = entry

        if not server_configs:
            return []

        self._client = MultiServerMCPClient(server_configs)
        self._server_names = list(server_configs.keys())

        try:
            tools = await self._client.get_tools()
            logger.info(
                "Connected to %d MCP servers, loaded %d tools",
                len(self._server_names), len(tools),
            )
            return tools
        except Exception as exc:
            logger.error("Failed to connect to MCP servers: %s", exc)
            await self.shutdown()
            raise

    async def shutdown(self) -> None:
        """Disconnect all MCP servers."""
        if self._client is not None:
            try:
                if hasattr(self._client, 'close'):
                    await self._client.close()
                else:
                    await self._client.__aexit__(None, None, None)
            except Exception:
                pass
            self._client = None
        self._server_names = []
