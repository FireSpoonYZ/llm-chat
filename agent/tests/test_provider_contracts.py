from __future__ import annotations

from src.provider_contracts import get_provider_contract


def test_openai_contract_budget_and_thinking_kwargs() -> None:
    contract = get_provider_contract("openai")
    assert contract.provider == "openai"
    assert contract.build_budget_kwargs(2048) == {"max_completion_tokens": 2048}
    assert contract.build_thinking_kwargs(2048) == {
        "max_completion_tokens": 2048,
        "reasoning": {"effort": "high", "summary": "auto"},
    }


def test_anthropic_contract_thinking_kwargs() -> None:
    contract = get_provider_contract("anthropic")
    assert contract.build_budget_kwargs(4096) == {"max_tokens": 4096}
    assert contract.build_thinking_kwargs(4096) == {
        "max_tokens": 4096,
        "thinking": {"type": "enabled", "budget_tokens": 4095},
    }


def test_google_contract_thinking_kwargs() -> None:
    contract = get_provider_contract("google")
    assert contract.build_budget_kwargs(10000) == {"max_output_tokens": 10000}
    assert contract.build_thinking_kwargs(10000) == {
        "max_output_tokens": 10000,
        "thinking_budget": 9999,
    }


def test_unknown_contract_falls_back_to_generic_budget() -> None:
    contract = get_provider_contract("unknown")
    assert contract.build_budget_kwargs(512) == {"max_tokens": 512}
    assert contract.build_thinking_kwargs(512) == {"max_tokens": 512}


def test_openai_contract_extracts_reasoning_and_text() -> None:
    contract = get_provider_contract("openai")
    reasoning_block = {
        "type": "reasoning",
        "summary": [
            {"type": "summary_text", "text": "step one"},
            {"type": "summary_text", "text": "step two"},
        ],
    }
    text_block = {"type": "text", "text": "final"}

    assert contract.extract_thinking_deltas(reasoning_block) == ["step one", "step two"]
    assert contract.extract_text_delta(text_block) == "final"


def test_base_contract_extracts_default_thinking_block() -> None:
    contract = get_provider_contract("mistral")
    assert contract.extract_thinking_deltas({"type": "thinking", "thinking": "hmm"}) == ["hmm"]
