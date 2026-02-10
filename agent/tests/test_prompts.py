"""Tests for the prompts module."""

from __future__ import annotations

from src.prompts.assembler import assemble_system_prompt
from src.prompts.base import BASE_PROMPT
from src.prompts.mcp import mcp_instructions
from src.prompts.tools import TOOL_DESCRIPTIONS, format_tool_descriptions


class TestBasePrompt:
    def test_base_prompt_not_empty(self):
        assert len(BASE_PROMPT) > 100

    def test_base_prompt_mentions_tools(self):
        assert "tool" in BASE_PROMPT.lower()


class TestToolDescriptions:
    def test_all_tools_have_descriptions(self):
        expected = ["bash", "read", "write", "edit", "glob", "grep", "web_fetch", "code_interpreter"]
        for name in expected:
            assert name in TOOL_DESCRIPTIONS

    def test_format_tool_descriptions(self):
        result = format_tool_descriptions(["bash", "read"])
        assert "bash" in result
        assert "read" in result
        assert "# Available Tools" in result

    def test_format_empty_list(self):
        result = format_tool_descriptions([])
        assert "# Available Tools" in result

    def test_format_unknown_tool(self):
        result = format_tool_descriptions(["unknown_tool"])
        assert "unknown_tool" in result


class TestMcpInstructions:
    def test_empty_servers(self):
        assert mcp_instructions([]) == ""

    def test_single_server(self):
        servers = [{"name": "test-server", "description": "A test server"}]
        result = mcp_instructions(servers)
        assert "test-server" in result
        assert "A test server" in result
        assert "mcp_test-server_" in result

    def test_multiple_servers(self):
        servers = [
            {"name": "server1"},
            {"name": "server2", "description": "Second server"},
        ]
        result = mcp_instructions(servers)
        assert "server1" in result
        assert "server2" in result

    def test_no_description(self):
        servers = [{"name": "minimal"}]
        result = mcp_instructions(servers)
        assert "minimal" in result


class TestAssembleSystemPrompt:
    def test_base_only(self):
        result = assemble_system_prompt([])
        assert BASE_PROMPT in result

    def test_with_tools(self):
        result = assemble_system_prompt(["bash", "read"])
        assert BASE_PROMPT in result
        assert "bash" in result
        assert "read" in result

    def test_with_mcp(self):
        servers = [{"name": "test-mcp", "description": "Test MCP"}]
        result = assemble_system_prompt(["bash"], mcp_servers=servers)
        assert "test-mcp" in result

    def test_with_user_override(self):
        result = assemble_system_prompt([], user_override="Always respond in French")
        assert "Always respond in French" in result
        assert "Additional Instructions" in result

    def test_full_assembly(self):
        servers = [{"name": "mcp1"}]
        result = assemble_system_prompt(
            ["bash", "read"],
            mcp_servers=servers,
            user_override="Be brief",
        )
        assert BASE_PROMPT in result
        assert "bash" in result
        assert "mcp1" in result
        assert "Be brief" in result
