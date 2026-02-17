from __future__ import annotations

from src.history_normalizer import normalize_history_content


def test_openai_normalizer_strips_response_ids_and_empty_text() -> None:
    content = [
        {"type": "text", "text": ""},
        {"type": "text", "text": "hello", "id": "rs_block"},
        {"type": "reasoning", "id": "rs_parent", "summary": [{"text": "plan", "id": "rs_child"}]},
    ]
    normalized = normalize_history_content("openai", content)
    assert isinstance(normalized, list)
    assert len(normalized) == 2
    dumped = str(normalized)
    assert "rs_parent" not in dumped
    assert "rs_child" not in dumped
    assert "rs_block" not in dumped


def test_non_openai_keeps_ids_but_cleans_empty_blocks() -> None:
    content = [
        {"type": "thinking", "thinking": ""},
        {"type": "thinking", "thinking": "hmm", "id": "rs_keep"},
    ]
    normalized = normalize_history_content("anthropic", content)
    assert normalized == [{"type": "thinking", "thinking": "hmm", "id": "rs_keep"}]


def test_non_list_content_returns_as_is() -> None:
    assert normalize_history_content("openai", "text") == "text"
