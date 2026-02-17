"""Anthropic provider contract."""

from __future__ import annotations

from typing import Any

from .base import ProviderContract


class AnthropicProviderContract(ProviderContract):
    """Anthropic-specific thinking kwargs."""

    def build_thinking_kwargs(self, budget: int) -> dict[str, Any]:
        kwargs = self.build_budget_kwargs(budget)
        kwargs["thinking"] = {"type": "enabled", "budget_tokens": max(int(budget) - 1, 0)}
        return kwargs
