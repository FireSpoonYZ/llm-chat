from __future__ import annotations

import json
from typing import Type

import html2text
import httpx
import httpx_sse
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

EXA_MCP_URL = "https://mcp.exa.ai/mcp"

_h2t = html2text.HTML2Text()
_h2t.ignore_links = False
_h2t.ignore_images = True
_h2t.body_width = 0  # no wrapping


class WebFetchInput(BaseModel):
    """Input for the WebFetchTool."""

    url: str = Field(description="The URL to fetch content from.")
    max_length: int = Field(
        default=50000,
        description="Maximum number of characters to return from the fetched content.",
    )


class WebFetchTool(BaseTool):
    """Fetch a URL and return its text content."""

    name: str = "web_fetch"
    description: str = (
        "Fetch content from a URL. Converts HTML to plain text by stripping tags. "
        "Returns the text content truncated to max_length characters."
    )
    args_schema: Type[BaseModel] = WebFetchInput

    def _run(self, url: str, max_length: int = 50000) -> str:
        raise NotImplementedError(
            "WebFetchTool does not support synchronous execution. Use the async interface."
        )

    async def _arun(self, url: str, max_length: int = 50000) -> str:
        try:
            async with httpx.AsyncClient(
                follow_redirects=True,
                timeout=httpx.Timeout(30.0),
            ) as client:
                response = await client.get(url)
                response.raise_for_status()
        except httpx.TimeoutException:
            return f"Error: request to '{url}' timed out after 30 seconds."
        except httpx.ConnectError as exc:
            return f"Error: could not connect to '{url}': {exc}"
        except httpx.HTTPStatusError as exc:
            return f"Error: HTTP {exc.response.status_code} for '{url}'."
        except httpx.HTTPError as exc:
            return f"Error: failed to fetch '{url}': {exc}"

        content_type = response.headers.get("content-type", "")
        body = response.text

        if "html" in content_type.lower():
            text = _h2t.handle(body)
        else:
            text = body

        if len(text) > max_length:
            text = text[:max_length] + "\n... content truncated"

        return text


class WebSearchInput(BaseModel):
    """Input for the WebSearchTool."""

    query: str = Field(description="The search query.")
    num_results: int = Field(
        default=5, description="Number of results to return (1-10)."
    )
    type: str = Field(
        default="auto",
        description="Search type: auto, fast, or deep.",
    )


class WebSearchTool(BaseTool):
    """Search the web using Exa AI MCP endpoint."""

    name: str = "web_search"
    description: str = (
        "Search the web using Exa AI. Returns relevant web page content "
        "for the given query. Use this to find up-to-date information."
    )
    args_schema: Type[BaseModel] = WebSearchInput

    def _run(self, query: str, num_results: int = 5, type: str = "auto") -> str:
        raise NotImplementedError(
            "WebSearchTool does not support synchronous execution. Use the async interface."
        )

    async def _arun(self, query: str, num_results: int = 5, type: str = "auto") -> str:
        payload = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "web_search_exa",
                "arguments": {
                    "query": query,
                    "numResults": num_results,
                    "type": type,
                    "livecrawl": "fallback",
                    "contextMaxCharacters": 10000,
                },
            },
        }

        try:
            async with httpx.AsyncClient(timeout=httpx.Timeout(25.0)) as client:
                async with httpx_sse.aconnect_sse(
                    client,
                    "POST",
                    EXA_MCP_URL,
                    json=payload,
                    headers={
                        "accept": "application/json, text/event-stream",
                        "content-type": "application/json",
                    },
                ) as event_source:
                    async for event in event_source.aiter_sse():
                        try:
                            data = json.loads(event.data)
                            content = data.get("result", {}).get("content") or []
                            if content:
                                return content[0].get("text", "")
                        except (json.JSONDecodeError, KeyError, IndexError):
                            continue
        except httpx.TimeoutException:
            return "Error: web search request timed out after 25 seconds."
        except httpx.HTTPError as exc:
            return f"Error: web search request failed: {exc}"

        return "No search results found. Please try a different query."
