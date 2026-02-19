from __future__ import annotations

import json
from typing import Any, Literal, Type

import html2text
import httpx
import httpx_sse
from bs4 import BeautifulSoup
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from .result_schema import make_tool_error, make_tool_success

EXA_MCP_URL = "https://mcp.exa.ai/mcp"
MAX_WEBFETCH_BYTES = 5 * 1024 * 1024

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
    format: Literal["text", "markdown", "html"] = Field(
        default="markdown",
        description="Desired output format: text, markdown, or html.",
    )


class WebFetchTool(BaseTool):
    """Fetch a URL and return its text content."""

    name: str = "web_fetch"
    description: str = (
        "Fetch content from a URL. Supports output formats text/markdown/html. "
        "Returns content truncated to max_length characters."
    )
    args_schema: Type[BaseModel] = WebFetchInput

    def _run(
        self,
        url: str,
        max_length: int = 50000,
        format: Literal["text", "markdown", "html"] = "markdown",
    ) -> dict[str, Any]:
        raise NotImplementedError(
            "WebFetchTool does not support synchronous execution. Use the async interface."
        )

    def _is_textual_content_type(self, content_type: str) -> bool:
        lowered = content_type.lower()
        return any(
            token in lowered
            for token in (
                "text/",
                "json",
                "xml",
                "javascript",
                "yaml",
                "csv",
                "html",
            )
        )

    def _format_response_body(
        self,
        *,
        body: str,
        content_type: str,
        format: Literal["text", "markdown", "html"],
    ) -> str:
        lowered_type = content_type.lower()
        if "html" in lowered_type:
            if format == "html":
                return body
            if format == "markdown":
                return _h2t.handle(body)
            return BeautifulSoup(body, "html.parser").get_text("\n", strip=True)

        if "json" in lowered_type:
            try:
                obj = json.loads(body)
            except json.JSONDecodeError:
                return body
            pretty = json.dumps(obj, ensure_ascii=False, indent=2)
            if format == "markdown":
                return f"```json\n{pretty}\n```"
            return pretty

        return body

    async def _arun(
        self,
        url: str,
        max_length: int = 50000,
        format: Literal["text", "markdown", "html"] = "markdown",
    ) -> dict[str, Any]:
        try:
            async with httpx.AsyncClient(
                follow_redirects=True,
                timeout=httpx.Timeout(30.0),
            ) as client:
                response = await client.get(url)
                response.raise_for_status()
        except httpx.TimeoutException:
            return make_tool_error(
                kind=self.name,
                error=f"request to '{url}' timed out after 30 seconds",
            )
        except httpx.ConnectError as exc:
            return make_tool_error(
                kind=self.name,
                error=f"could not connect to '{url}': {exc}",
            )
        except httpx.HTTPStatusError as exc:
            return make_tool_error(
                kind=self.name,
                error=f"HTTP {exc.response.status_code} for '{url}'",
                data={"status_code": exc.response.status_code, "url": str(exc.request.url)},
            )
        except httpx.HTTPError as exc:
            return make_tool_error(
                kind=self.name,
                error=f"failed to fetch '{url}': {exc}",
            )

        content_type = response.headers.get("content-type", "")
        if len(response.content) > MAX_WEBFETCH_BYTES:
            return make_tool_error(
                kind=self.name,
                error=(
                    f"response too large for '{url}': {len(response.content)} bytes "
                    f"(max {MAX_WEBFETCH_BYTES} bytes)"
                ),
            )
        if not self._is_textual_content_type(content_type):
            return make_tool_error(
                kind=self.name,
                error=f"unsupported content type for text fetch: '{content_type}'",
                data={"url": str(response.url), "content_type": content_type},
            )

        body = response.text
        text = self._format_response_body(
            body=body,
            content_type=content_type,
            format=format,
        )

        truncated = False
        original_char_count = len(text)
        if len(text) > max_length:
            text = text[:max_length] + "\n... content truncated"
            truncated = True

        return make_tool_success(
            kind=self.name,
            text=text,
            data={
                "url": str(response.url),
                "content_type": content_type,
                "format": format,
            },
            meta={
                "truncated": truncated,
                "char_count": len(text),
                "original_char_count": original_char_count,
                "bytes": len(response.content),
            },
        )


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

    def _run(self, query: str, num_results: int = 5, type: str = "auto") -> dict[str, Any]:
        raise NotImplementedError(
            "WebSearchTool does not support synchronous execution. Use the async interface."
        )

    async def _arun(self, query: str, num_results: int = 5, type: str = "auto") -> dict[str, Any]:
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
                                text = "\n\n".join(
                                    block.get("text", "")
                                    for block in content
                                    if isinstance(block, dict)
                                ).strip()
                                return make_tool_success(
                                    kind=self.name,
                                    text=text or "No search results found.",
                                    data={
                                        "query": query,
                                        "num_results": num_results,
                                        "type": type,
                                        "results": content,
                                    },
                                    meta={"result_count": len(content)},
                                )
                        except (json.JSONDecodeError, KeyError, IndexError):
                            continue
        except httpx.TimeoutException:
            return make_tool_error(
                kind=self.name,
                error="web search request timed out after 25 seconds",
            )
        except httpx.HTTPError as exc:
            return make_tool_error(
                kind=self.name,
                error=f"web search request failed: {exc}",
            )

        return make_tool_success(
            kind=self.name,
            text="No search results found. Please try a different query.",
            data={"query": query, "num_results": num_results, "type": type, "results": []},
            meta={"result_count": 0},
        )
