"""History normalization utilities before LLM replay."""

from __future__ import annotations

from typing import Any

_OPENAI_RESPONSE_ID_PREFIXES = ("rs_", "resp_", "msg_", "item_")
_OPENAI_RESPONSE_ID_KEYS = {"id", "item_id", "response_id"}


def _is_empty_text_block(block: dict[str, Any]) -> bool:
    block_type = block.get("type")
    if block_type == "text":
        return not bool(str(block.get("text", "")).strip())
    if block_type == "thinking":
        return not bool(str(block.get("thinking", "")).strip())
    return False


def _strip_openai_response_ids(value: Any) -> Any:
    if isinstance(value, list):
        return [_strip_openai_response_ids(v) for v in value]
    if isinstance(value, dict):
        cleaned: dict[str, Any] = {}
        for key, val in value.items():
            if (
                key in _OPENAI_RESPONSE_ID_KEYS
                and isinstance(val, str)
                and val.startswith(_OPENAI_RESPONSE_ID_PREFIXES)
            ):
                continue
            cleaned[key] = _strip_openai_response_ids(val)
        return cleaned
    return value


def normalize_history_content(provider: str, content: Any) -> Any:
    """Normalize history content blocks before sending to providers."""
    if not isinstance(content, list):
        return content

    normalized = [b for b in content if not (isinstance(b, dict) and _is_empty_text_block(b))]
    if provider.lower() == "openai":
        normalized = _strip_openai_response_ids(normalized)
    return normalized
