"""Base provider contract for provider-specific request/response behavior."""

from __future__ import annotations

from typing import Any

from ..history_normalizer import normalize_history_content
from ..provider_capabilities import (
    ProviderCapabilities,
    get_provider_capabilities,
)


class ProviderContract:
    """Default provider contract implementation."""

    def __init__(self, provider: str) -> None:
        key = (provider or "").strip().lower()
        self.provider = key or "unknown"
        self.capabilities: ProviderCapabilities = get_provider_capabilities(self.provider)

    @property
    def token_limit_param(self) -> str:
        return self.capabilities.token_limit_param

    def build_budget_kwargs(self, budget: int) -> dict[str, Any]:
        return {self.token_limit_param: int(budget)}

    def build_thinking_kwargs(self, budget: int) -> dict[str, Any]:
        return self.build_budget_kwargs(budget)

    def normalize_history_content(self, content: Any) -> Any:
        return normalize_history_content(self.provider, content)

    def extract_thinking_deltas(self, block: dict[str, Any]) -> list[str]:
        if block.get("type") != "thinking":
            return []
        thinking = str(block.get("thinking", ""))
        return [thinking] if thinking else []

    def extract_text_delta(self, block: dict[str, Any]) -> str:
        if block.get("type") != "text":
            return ""
        return str(block.get("text", ""))
