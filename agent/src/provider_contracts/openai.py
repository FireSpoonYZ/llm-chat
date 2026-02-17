"""OpenAI provider contract."""

from __future__ import annotations

from typing import Any

from .base import ProviderContract


class OpenAIProviderContract(ProviderContract):
    """OpenAI-specific reasoning/thinking behavior."""

    def build_thinking_kwargs(self, budget: int) -> dict[str, Any]:
        kwargs = self.build_budget_kwargs(budget)
        if self.capabilities.supports_reasoning:
            kwargs["reasoning"] = {"effort": "high", "summary": "auto"}
        return kwargs

    def extract_thinking_deltas(self, block: dict[str, Any]) -> list[str]:
        if block.get("type") == "reasoning":
            deltas: list[str] = []
            summaries = block.get("summary")
            if isinstance(summaries, list):
                for summary in summaries:
                    if isinstance(summary, dict):
                        text = str(summary.get("text", ""))
                        if text:
                            deltas.append(text)
            reasoning = str(block.get("reasoning", ""))
            if reasoning:
                deltas.append(reasoning)
            return deltas
        return super().extract_thinking_deltas(block)
