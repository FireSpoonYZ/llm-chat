from __future__ import annotations

from typing import Any


def _rtext(result: Any) -> str:
    if isinstance(result, dict):
        return str(result.get("text", ""))
    return str(result)


def _rdata(result: Any) -> dict[str, Any]:
    if isinstance(result, dict):
        data = result.get("data")
        if isinstance(data, dict):
            return data
    return {}


def _rmeta(result: Any) -> dict[str, Any]:
    if isinstance(result, dict):
        meta = result.get("meta")
        if isinstance(meta, dict):
            return meta
    return {}


def _rllm(result: Any) -> str | list[dict[str, Any]] | None:
    if isinstance(result, dict):
        llm_content = result.get("llm_content")
        if isinstance(llm_content, (list, str)):
            return llm_content
    return None


def _rerror(result: Any) -> str:
    if isinstance(result, dict):
        return str(result.get("error", ""))
    return ""


def _rsuccess(result: Any) -> bool:
    if isinstance(result, dict):
        return bool(result.get("success", False))
    return False
