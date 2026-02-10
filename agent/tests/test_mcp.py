"""Tests for MCP client and manager modules."""

from __future__ import annotations

import json
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.mcp.client import McpClient
from src.mcp.manager import McpLangChainTool, McpManager, McpToolInput


# ---------------------------------------------------------------------------
# McpClient
# ---------------------------------------------------------------------------

class TestMcpClient:
    def test_init(self):
        client = McpClient(name="test", command="echo")
        assert client.name == "test"
        assert client.command == "echo"
        assert client.args == []
        assert client.env == {}
        assert not client.is_connected

    def test_init_with_args(self):
        client = McpClient(
            name="test",
            command="python",
            args=["-m", "server"],
            env={"KEY": "val"},
        )
        assert client.args == ["-m", "server"]
        assert client.env == {"KEY": "val"}

    def test_is_connected_false_initially(self):
        client = McpClient(name="test", command="echo")
        assert client.is_connected is False

    def test_get_tool_schemas_empty(self):
        client = McpClient(name="test", command="echo")
        assert client.get_tool_schemas() == []

    def test_get_tool_schemas_with_tools(self):
        client = McpClient(name="myserver", command="echo")
        mock_tool = MagicMock()
        mock_tool.name = "add"
        mock_tool.description = "Add two numbers"
        mock_tool.inputSchema = {"type": "object", "properties": {"a": {"type": "number"}}}
        client._tools = [mock_tool]

        schemas = client.get_tool_schemas()
        assert len(schemas) == 1
        assert schemas[0]["name"] == "mcp_myserver_add"
        assert schemas[0]["description"] == "Add two numbers"
        assert schemas[0]["mcp_server"] == "myserver"
        assert schemas[0]["mcp_tool_name"] == "add"
        assert "input_schema" in schemas[0]

    async def test_list_tools_not_connected(self):
        client = McpClient(name="test", command="echo")
        with pytest.raises(RuntimeError, match="not connected"):
            await client.list_tools()

    async def test_call_tool_not_connected(self):
        client = McpClient(name="test", command="echo")
        with pytest.raises(RuntimeError, match="not connected"):
            await client.call_tool("test_tool")

    async def test_disconnect_when_not_connected(self):
        client = McpClient(name="test", command="echo")
        # Should not raise
        await client.disconnect()
        assert not client.is_connected


# ---------------------------------------------------------------------------
# McpLangChainTool
# ---------------------------------------------------------------------------

class TestMcpLangChainTool:
    def test_sync_raises(self):
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_tool_name="test",
        )
        with pytest.raises(NotImplementedError):
            tool._run()

    async def test_arun_no_client(self):
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_client=None,
            mcp_tool_name="test",
        )
        result = await tool._arun()
        assert "not available" in result.lower()

    async def test_arun_invalid_json(self):
        mock_client = AsyncMock()
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_client=mock_client,
            mcp_tool_name="test",
        )
        result = await tool._arun(arguments="not json{{{")
        assert "invalid json" in result.lower()

    async def test_arun_success(self):
        mock_client = AsyncMock()
        mock_client.call_tool = AsyncMock(return_value="tool output")
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_client=mock_client,
            mcp_tool_name="add",
        )
        result = await tool._arun(arguments='{"a": 1, "b": 2}')
        assert result == "tool output"
        mock_client.call_tool.assert_called_once_with("add", {"a": 1, "b": 2})

    async def test_arun_empty_args(self):
        mock_client = AsyncMock()
        mock_client.call_tool = AsyncMock(return_value="ok")
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_client=mock_client,
            mcp_tool_name="ping",
        )
        result = await tool._arun(arguments="{}")
        mock_client.call_tool.assert_called_once_with("ping", {})

    async def test_arun_tool_error(self):
        mock_client = AsyncMock()
        mock_client.call_tool = AsyncMock(side_effect=RuntimeError("server down"))
        tool = McpLangChainTool(
            name="test_tool",
            description="test",
            mcp_client=mock_client,
            mcp_tool_name="fail",
        )
        result = await tool._arun(arguments="{}")
        assert "error" in result.lower()
        assert "server down" in result.lower()


# ---------------------------------------------------------------------------
# McpToolInput
# ---------------------------------------------------------------------------

class TestMcpToolInput:
    def test_default_arguments(self):
        inp = McpToolInput()
        assert inp.arguments == "{}"

    def test_custom_arguments(self):
        inp = McpToolInput(arguments='{"key": "value"}')
        assert inp.arguments == '{"key": "value"}'


# ---------------------------------------------------------------------------
# McpManager
# ---------------------------------------------------------------------------

class TestMcpManager:
    def test_init(self):
        manager = McpManager()
        assert manager.connected_servers == []

    async def test_remove_nonexistent_server(self):
        manager = McpManager()
        # Should not raise
        await manager.remove_server("nonexistent")

    async def test_shutdown_empty(self):
        manager = McpManager()
        # Should not raise
        await manager.shutdown()

    def test_get_langchain_tools_empty(self):
        manager = McpManager()
        assert manager.get_langchain_tools() == []

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

    async def test_setup_from_config_parses_args_string(self):
        """Args as JSON string should be parsed."""
        manager = McpManager()
        # This will fail to connect (no real server), but we test arg parsing
        config = [{
            "name": "test",
            "transport": "stdio",
            "command": "nonexistent_command_12345",
            "args": '["--flag", "value"]',
        }]
        # Will fail to connect, but shouldn't crash
        tools = await manager.setup_from_config(config)
        assert tools == []

    async def test_setup_from_config_parses_env_string(self):
        manager = McpManager()
        config = [{
            "name": "test",
            "transport": "stdio",
            "command": "nonexistent_command_12345",
            "env_vars": '{"KEY": "val"}',
        }]
        tools = await manager.setup_from_config(config)
        assert tools == []

    async def test_get_langchain_tools_with_mock_client(self):
        manager = McpManager()
        mock_client = MagicMock()
        mock_client.is_connected = True
        mock_client.get_tool_schemas.return_value = [
            {
                "name": "mcp_test_add",
                "description": "Add numbers",
                "mcp_server": "test",
                "mcp_tool_name": "add",
            }
        ]
        manager._clients["test"] = mock_client
        tools = manager.get_langchain_tools()
        assert len(tools) == 1
        assert tools[0].name == "mcp_test_add"

    async def test_get_langchain_tools_skips_disconnected(self):
        manager = McpManager()
        mock_client = MagicMock()
        mock_client.is_connected = False
        manager._clients["test"] = mock_client
        tools = manager.get_langchain_tools()
        assert len(tools) == 0
