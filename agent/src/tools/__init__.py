"""Built-in tools for the Claude Chat agent."""

from __future__ import annotations

from typing import Optional

from langchain_core.tools import BaseTool

from .bash import BashTool
from .file_ops import ReadTool, WriteTool, EditTool
from .list_tool import ListTool
from .search import GlobTool, GrepTool
from .web import WebFetchTool, WebSearchTool
from .code_interpreter import CodeInterpreterTool
from .image_gen import ImageGenerationTool
from .explore import ExploreTool
from .question import QuestionTool
from .capabilities import annotate_builtin_tools, set_tool_capabilities

ALL_TOOLS = [
    BashTool,
    ReadTool,
    WriteTool,
    EditTool,
    ListTool,
    GlobTool,
    GrepTool,
    WebFetchTool,
    WebSearchTool,
    QuestionTool,
    CodeInterpreterTool,
    ImageGenerationTool,
    ExploreTool,
]


def create_all_tools(
    workspace: str = "/workspace",
    *,
    provider: str = "",
    api_key: str = "",
    endpoint_url: Optional[str] = None,
    model: str = "",
    image_provider: str = "",
    image_model: str = "",
    image_api_key: str = "",
    image_endpoint_url: Optional[str] = None,
) -> list[BaseTool]:
    """Create instances of all built-in tools."""
    tools: list[BaseTool] = [
        BashTool(workspace=workspace),
        ReadTool(workspace=workspace),
        WriteTool(workspace=workspace),
        EditTool(workspace=workspace),
        ListTool(workspace=workspace),
        GlobTool(workspace=workspace),
        GrepTool(workspace=workspace),
        WebFetchTool(),
        WebSearchTool(),
        QuestionTool(),
        CodeInterpreterTool(workspace=workspace),
    ]
    # Use dedicated image config if provided, otherwise skip image tool
    if image_provider and image_api_key and image_model:
        tools.append(ImageGenerationTool(
            workspace=workspace,
            provider=image_provider,
            api_key=image_api_key,
            endpoint_url=image_endpoint_url,
            model=image_model,
        ))
    annotate_builtin_tools(tools)
    return tools


def create_explore_tool(runner: object) -> BaseTool:
    tool = ExploreTool(runner=runner)
    set_tool_capabilities(tool, source="builtin", read_only=False)
    return tool
