"""Tests for MCP manager module."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.mcp.manager import McpManager


class TestMcpManager:
    def test_init(self):
        manager = McpManager()
        assert manager.connected_servers == []

    async def test_shutdown_empty(self):
        manager = McpManager()
        await manager.shutdown()
        assert manager.connected_servers == []

    async def test_setup_from_config_skips_non_stdio(self):
        manager = McpManager()
        config = [{"name": "sse-server", "transport": "sse", "url": "http://example.com"}]
        tools = await manager.setup_from_config(config)
        assert tools == []
        assert manager.connected_servers == []

    async def test_setup_from_config_skips_no_command(self):
        manager = McpManager()
        config = [{"name": "bad", "transport": "stdio", "command": ""}]
        tools = await manager.setup_from_config(config)
        assert tools == []

    async def test_setup_from_config_empty_list(self):
        manager = McpManager()
        tools = await manager.setup_from_config([])
        assert tools == []
        assert manager.connected_servers == []

    async def test_setup_from_config_parses_args_json_string(self):
        """Args as JSON string should be parsed into a list."""
        manager = McpManager()
        mock_tool = MagicMock()
        mock_tool.name = "test_tool"

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=[mock_tool])

            config = [{
                "name": "test",
                "transport": "stdio",
                "command": "python",
                "args": '["--flag", "value"]',
            }]
            tools = await manager.setup_from_config(config)

            # Verify the client was created with parsed args
            call_args = MockClient.call_args[0][0]
            assert call_args["test"]["args"] == ["--flag", "value"]
            assert len(tools) == 1

    async def test_setup_from_config_parses_args_plain_string(self):
        """Non-JSON args string should be split by whitespace."""
        manager = McpManager()
        mock_tool = MagicMock()

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=[mock_tool])

            config = [{
                "name": "test",
                "transport": "stdio",
                "command": "python",
                "args": "--flag value",
            }]
            await manager.setup_from_config(config)

            call_args = MockClient.call_args[0][0]
            assert call_args["test"]["args"] == ["--flag", "value"]

    async def test_setup_from_config_parses_env_string(self):
        """env_vars as JSON string should be parsed into a dict."""
        manager = McpManager()
        mock_tool = MagicMock()

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=[mock_tool])

            config = [{
                "name": "test",
                "transport": "stdio",
                "command": "python",
                "env_vars": '{"KEY": "val"}',
            }]
            await manager.setup_from_config(config)

            call_args = MockClient.call_args[0][0]
            assert call_args["test"]["env"] == {"KEY": "val"}

    async def test_setup_from_config_success(self):
        """Successful setup returns tools and tracks server names."""
        manager = McpManager()
        mock_tools = [MagicMock(), MagicMock()]

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=mock_tools)

            config = [
                {"name": "server1", "transport": "stdio", "command": "cmd1"},
                {"name": "server2", "transport": "stdio", "command": "cmd2"},
            ]
            tools = await manager.setup_from_config(config)

            assert len(tools) == 2
            assert set(manager.connected_servers) == {"server1", "server2"}

    async def test_setup_from_config_connection_failure(self):
        """Connection failure should raise and clean up."""
        manager = McpManager()

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(
                side_effect=ConnectionError("refused")
            )
            instance.__aexit__ = AsyncMock()

            config = [{"name": "bad", "transport": "stdio", "command": "cmd"}]
            with pytest.raises(ConnectionError):
                await manager.setup_from_config(config)

            assert manager.connected_servers == []

    async def test_shutdown_clears_state(self):
        """Shutdown should clear client and server names."""
        manager = McpManager()
        mock_tools = [MagicMock()]

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=mock_tools)
            instance.__aexit__ = AsyncMock()

            config = [{"name": "srv", "transport": "stdio", "command": "cmd"}]
            await manager.setup_from_config(config)
            assert manager.connected_servers == ["srv"]

            await manager.shutdown()
            assert manager.connected_servers == []
            assert manager._client is None

    async def test_shutdown_calls_close_or_aexit(self):
        """Shutdown should call client.close() if available, else __aexit__."""
        manager = McpManager()
        mock_client = MagicMock()
        mock_client.close = AsyncMock()
        manager._client = mock_client
        manager._server_names = ["test"]

        await manager.shutdown()
        mock_client.close.assert_awaited_once()
        assert manager._client is None

    async def test_setup_replaces_previous_client(self):
        """Calling setup_from_config again should shut down the old client."""
        manager = McpManager()

        with patch(
            "src.mcp.manager.MultiServerMCPClient"
        ) as MockClient:
            instance = MockClient.return_value
            instance.get_tools = AsyncMock(return_value=[MagicMock()])
            instance.__aexit__ = AsyncMock()

            config = [{"name": "srv", "transport": "stdio", "command": "cmd"}]
            await manager.setup_from_config(config)

            await manager.setup_from_config(config)
            # A new client should have been created
            assert MockClient.call_count == 2
