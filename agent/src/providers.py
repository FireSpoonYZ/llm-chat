"""Multi-provider LLM factory for LangChain."""

from __future__ import annotations

from typing import Any

from langchain.chat_models import init_chat_model
from langchain_core.language_models.chat_models import BaseChatModel


SUPPORTED_PROVIDERS = ("openai", "anthropic", "google", "mistral")

# Map our provider names to init_chat_model's model_provider values.
_PROVIDER_MAP = {
    "openai": "openai",
    "anthropic": "anthropic",
    "google": "google_genai",
    "mistral": "mistralai",
}


def create_chat_model(
    provider: str,
    model: str,
    api_key: str,
    *,
    endpoint_url: str | None = None,
    streaming: bool = True,
    temperature: float = 0.0,
    **kwargs: Any,
) -> BaseChatModel:
    """Create a LangChain chat model for the given provider.

    Args:
        provider: One of 'openai', 'anthropic', 'google', 'mistral'.
        model: Model name (e.g. 'gpt-4o', 'claude-sonnet-4-20250514').
        api_key: Provider API key.
        endpoint_url: Optional custom endpoint URL.
        streaming: Whether to enable streaming.
        temperature: Sampling temperature.
        **kwargs: Additional provider-specific kwargs.

    Returns:
        A configured BaseChatModel instance.

    Raises:
        ValueError: If the provider is not supported.
    """
    provider = provider.lower().strip()
    if provider not in SUPPORTED_PROVIDERS:
        raise ValueError(
            f"Unsupported provider: {provider!r}. "
            f"Supported: {', '.join(SUPPORTED_PROVIDERS)}"
        )

    model_provider = _PROVIDER_MAP[provider]
    params: dict[str, Any] = {
        "api_key": api_key,
        "streaming": streaming,
        "temperature": temperature,
        **kwargs,
    }
    if endpoint_url:
        if provider == "mistral":
            params["endpoint"] = endpoint_url
        elif provider != "google":
            params["base_url"] = endpoint_url

    return init_chat_model(
        model=model,
        model_provider=model_provider,
        **params,
    )
