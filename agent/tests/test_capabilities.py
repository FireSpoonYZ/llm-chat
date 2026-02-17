from __future__ import annotations

from src.tools.capabilities import annotate_mcp_tools, tool_is_read_only


class _FakeTool:
    def __init__(self, name: str, metadata: dict | None = None) -> None:
        self.name = name
        self.metadata = metadata or {}


def test_annotate_mcp_tools_applies_server_override() -> None:
    tool = _FakeTool("repo.search", {"mcp_server": "repo", "read_only": False})
    annotate_mcp_tools(
        [tool], mcp_servers=[{"name": "repo", "read_only_overrides": {"search": True}}],
    )
    assert tool_is_read_only(tool) is True
    assert tool.metadata.get("mcp_server") == "repo"


def test_annotate_mcp_tools_uses_unique_global_override_when_server_unknown() -> None:
    tool = _FakeTool("list_files")
    annotate_mcp_tools(
        [tool],
        mcp_servers=[
            {"name": "repo", "read_only_overrides": {"list_files": True}},
            {"name": "tickets", "read_only_overrides": {"create_ticket": False}},
        ],
    )
    assert tool_is_read_only(tool) is True


def test_annotate_mcp_tools_does_not_apply_ambiguous_global_override() -> None:
    tool = _FakeTool("search")
    annotate_mcp_tools(
        [tool],
        mcp_servers=[
            {"name": "repo", "read_only_overrides": {"search": True}},
            {"name": "tickets", "read_only_overrides": {"search": False}},
        ],
    )
    # Ambiguous key across servers must not be matched heuristically.
    assert tool_is_read_only(tool) is False


def test_annotate_mcp_tools_override_takes_precedence_over_metadata() -> None:
    tool = _FakeTool("repo.update", {"mcp_server": "repo", "readOnlyHint": True})
    annotate_mcp_tools(
        [tool], mcp_servers=[{"name": "repo", "read_only_overrides": {"update": False}}],
    )
    assert tool_is_read_only(tool) is False
