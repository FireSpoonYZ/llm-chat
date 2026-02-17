from __future__ import annotations

from typing import Any


def make_tool_result(
    *,
    kind: str,
    text: str,
    success: bool,
    error: str | None = None,
    data: dict[str, Any] | None = None,
    meta: dict[str, Any] | None = None,
    llm_content: str | list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Build a normalized tool result envelope.

    `llm_content` is internal-only metadata used by the agent when constructing
    ToolMessage content for the model. It must be stripped before sending
    tool_result events to the frontend and before persisting to history.
    """
    result: dict[str, Any] = {
        "kind": kind,
        "text": text,
        "success": bool(success),
        "error": error if not success else None,
        "data": data or {},
        "meta": meta or {},
    }
    if llm_content is not None:
        result["llm_content"] = llm_content
    return result


def make_tool_success(
    *,
    kind: str,
    text: str,
    data: dict[str, Any] | None = None,
    meta: dict[str, Any] | None = None,
    llm_content: str | list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    return make_tool_result(
        kind=kind,
        text=text,
        success=True,
        data=data,
        meta=meta,
        llm_content=llm_content,
    )


def make_tool_error(
    *,
    kind: str,
    error: str,
    text: str | None = None,
    data: dict[str, Any] | None = None,
    meta: dict[str, Any] | None = None,
    llm_content: str | list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    rendered = text if text is not None else f"Error: {error}"
    return make_tool_result(
        kind=kind,
        text=rendered,
        success=False,
        error=error,
        data=data,
        meta=meta,
        llm_content=llm_content,
    )


def extract_text_from_legacy_list(result: list[Any]) -> str:
    return " ".join(
        block.get("text", "")
        for block in result
        if isinstance(block, dict) and block.get("type") == "text"
    )

