"""Tests for the prompts module."""

from __future__ import annotations

from src.prompts.assembler import assemble_system_prompt
from src.prompts.base import BASE_PROMPT
from src.prompts.behaviors import (
    SAFETY_INSTRUCTIONS,
    TASK_EXECUTION_GUIDELINES,
    TOOL_USAGE_POLICY,
)
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

    def test_all_tools_have_rich_descriptions(self):
        for name, desc in TOOL_DESCRIPTIONS.items():
            assert len(desc) > 100, f"{name} description too short: {len(desc)} chars"

    def test_bash_description_contains_usage_rules(self):
        desc = TOOL_DESCRIPTIONS["bash"]
        assert "DO NOT" in desc

    def test_bash_description_contains_cross_tool_routing(self):
        desc = TOOL_DESCRIPTIONS["bash"]
        assert "read" in desc
        assert "edit" in desc
        assert "glob" in desc
        assert "grep" in desc

    def test_read_description_contains_line_number_format(self):
        desc = TOOL_DESCRIPTIONS["read"]
        assert "line_number" in desc or "line number" in desc.lower()

    def test_edit_description_contains_uniqueness_rule(self):
        desc = TOOL_DESCRIPTIONS["edit"]
        assert "unique" in desc.lower()

    def test_format_tool_descriptions(self):
        result = format_tool_descriptions(["bash", "read"])
        assert "bash" in result
        assert "read" in result
        assert "# Available Tools" in result

    def test_format_tool_descriptions_includes_section_header(self):
        result = format_tool_descriptions(["bash"])
        assert result.startswith("# Available Tools")

    def test_format_empty_list(self):
        result = format_tool_descriptions([])
        assert "# Available Tools" in result

    def test_format_unknown_tool(self):
        result = format_tool_descriptions(["unknown_tool"])
        assert "unknown_tool" in result


class TestBehaviorFragments:
    def test_tool_usage_policy_exists(self):
        assert len(TOOL_USAGE_POLICY) > 100

    def test_tool_usage_policy_contains_routing_rules(self):
        assert "read" in TOOL_USAGE_POLICY
        assert "edit" in TOOL_USAGE_POLICY
        assert "glob" in TOOL_USAGE_POLICY
        assert "grep" in TOOL_USAGE_POLICY

    def test_safety_instructions_exist(self):
        assert len(SAFETY_INSTRUCTIONS) > 100

    def test_safety_instructions_mention_destructive(self):
        assert "destructive" in SAFETY_INSTRUCTIONS.lower()

    def test_task_execution_guidelines_exist(self):
        assert len(TASK_EXECUTION_GUIDELINES) > 100


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

    def test_with_custom_base_prompt(self):
        result = assemble_system_prompt([], base_prompt="Custom base")
        assert "Custom base" in result

    def test_custom_base_replaces_default(self):
        result = assemble_system_prompt([], base_prompt="Custom base")
        assert BASE_PROMPT not in result
        assert "Custom base" in result

    def test_assembler_includes_behavior_fragments(self):
        result = assemble_system_prompt(["bash"])
        assert "Tool Usage Policy" in result

    def test_assembler_includes_safety_instructions(self):
        result = assemble_system_prompt(["bash"])
        assert "Safety Instructions" in result

    def test_assembler_includes_task_guidelines(self):
        result = assemble_system_prompt(["bash"])
        assert "Task Execution Guidelines" in result

    def test_assembler_order_base_then_behaviors_then_tools(self):
        result = assemble_system_prompt(["bash"])
        base_pos = result.index(BASE_PROMPT)
        policy_pos = result.index("Tool Usage Policy")
        safety_pos = result.index("Safety Instructions")
        task_pos = result.index("Task Execution Guidelines")
        tools_pos = result.index("# Available Tools")
        assert base_pos < policy_pos < safety_pos < task_pos < tools_pos

    def test_assembler_no_behaviors_without_tools(self):
        result = assemble_system_prompt([])
        assert "Tool Usage Policy" not in result
        assert "Safety Instructions" not in result
        assert "Task Execution Guidelines" not in result
