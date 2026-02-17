"""Provider contract registry."""

from __future__ import annotations

from .anthropic import AnthropicProviderContract
from .base import ProviderContract
from .google import GoogleProviderContract
from .mistral import MistralProviderContract
from .openai import OpenAIProviderContract


def get_provider_contract(provider: str) -> ProviderContract:
    key = (provider or "").strip().lower()
    if key == "openai":
        return OpenAIProviderContract(key)
    if key == "anthropic":
        return AnthropicProviderContract(key)
    if key == "google":
        return GoogleProviderContract(key)
    if key == "mistral":
        return MistralProviderContract(key)
    return ProviderContract(key)


__all__ = ["ProviderContract", "get_provider_contract"]
