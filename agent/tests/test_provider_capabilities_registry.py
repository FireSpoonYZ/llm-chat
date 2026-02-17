from __future__ import annotations

from src.provider_capabilities import get_provider_capabilities


def test_openai_capabilities() -> None:
    caps = get_provider_capabilities("openai")
    assert caps.provider == "openai"
    assert caps.token_limit_param == "max_completion_tokens"
    assert caps.supports_reasoning is True
    assert caps.supports_native_thinking is True


def test_anthropic_capabilities() -> None:
    caps = get_provider_capabilities("anthropic")
    assert caps.token_limit_param == "max_tokens"
    assert caps.supports_reasoning is False
    assert caps.supports_native_thinking is True
    assert caps.supports_cache_hints is True


def test_unknown_provider_falls_back_to_generic() -> None:
    caps = get_provider_capabilities("unknown")
    assert caps.provider == "unknown"
    assert caps.token_limit_param == "max_tokens"
    assert caps.supports_native_thinking is False
