"""Built-in tools for the Claude Chat agent."""

from __future__ import annotations

from langchain_core.tools import BaseTool

from .bash import BashTool
from .file_ops import ReadTool, WriteTool, EditTool
from .search import GlobTool, GrepTool
from .web import WebFetchTool, WebSearchTool
from .code_interpreter import CodeInterpreterTool

ALL_TOOLS = [
    BashTool,
    ReadTool,
    WriteTool,
    EditTool,
    GlobTool,
    GrepTool,
    WebFetchTool,
    WebSearchTool,
    CodeInterpreterTool,
]


def create_all_tools(workspace: str = "/workspace") -> list[BaseTool]:
    """Create instances of all built-in tools."""
    return [
        BashTool(workspace=workspace),
        ReadTool(workspace=workspace),
        WriteTool(workspace=workspace),
        EditTool(workspace=workspace),
        GlobTool(workspace=workspace),
        GrepTool(workspace=workspace),
        WebFetchTool(),
        WebSearchTool(),
        CodeInterpreterTool(workspace=workspace),
    ]
