"""Provider capability registry for contract-like LLM parameter mapping."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ProviderCapabilities:
    provider: str
    token_limit_param: str
    supports_reasoning: bool
    supports_native_thinking: bool
    supports_cache_hints: bool = False


_CAPABILITIES: dict[str, ProviderCapabilities] = {
    "openai": ProviderCapabilities(
        provider="openai",
        token_limit_param="max_completion_tokens",
        supports_reasoning=True,
        supports_native_thinking=True,
    ),
    "anthropic": ProviderCapabilities(
        provider="anthropic",
        token_limit_param="max_tokens",
        supports_reasoning=False,
        supports_native_thinking=True,
        supports_cache_hints=True,
    ),
    "google": ProviderCapabilities(
        provider="google",
        token_limit_param="max_output_tokens",
        supports_reasoning=False,
        supports_native_thinking=True,
    ),
    "mistral": ProviderCapabilities(
        provider="mistral",
        token_limit_param="max_tokens",
        supports_reasoning=False,
        supports_native_thinking=False,
    ),
}


def get_provider_capabilities(provider: str) -> ProviderCapabilities:
    key = (provider or "").strip().lower()
    if key in _CAPABILITIES:
        return _CAPABILITIES[key]
    return ProviderCapabilities(
        provider=key or "unknown",
        token_limit_param="max_tokens",
        supports_reasoning=False,
        supports_native_thinking=False,
    )
