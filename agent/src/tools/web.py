from __future__ import annotations

from typing import Type

import httpx
from bs4 import BeautifulSoup
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field


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
            soup = BeautifulSoup(body, "html.parser")
            # Remove script and style elements
            for element in soup(["script", "style", "noscript"]):
                element.decompose()
            text = soup.get_text(separator="\n", strip=True)
        else:
            text = body

        if len(text) > max_length:
            text = text[:max_length] + "\n... content truncated"

        return text
