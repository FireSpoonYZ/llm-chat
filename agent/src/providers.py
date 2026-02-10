"""Multi-provider LLM factory for LangChain."""

from __future__ import annotations

from typing import Any

from langchain_core.language_models.chat_models import BaseChatModel


SUPPORTED_PROVIDERS = ("openai", "anthropic", "google", "mistral")


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

    if provider == "openai":
        return _create_openai(model, api_key, endpoint_url, streaming, temperature, **kwargs)
    elif provider == "anthropic":
        return _create_anthropic(model, api_key, endpoint_url, streaming, temperature, **kwargs)
    elif provider == "google":
        return _create_google(model, api_key, streaming, temperature, **kwargs)
    elif provider == "mistral":
        return _create_mistral(model, api_key, endpoint_url, streaming, temperature, **kwargs)
    else:
        raise ValueError(
            f"Unsupported provider: {provider!r}. "
            f"Supported: {', '.join(SUPPORTED_PROVIDERS)}"
        )


def _create_openai(
    model: str,
    api_key: str,
    endpoint_url: str | None,
    streaming: bool,
    temperature: float,
    **kwargs: Any,
) -> BaseChatModel:
    from langchain_openai import ChatOpenAI

    params: dict[str, Any] = {
        "model": model,
        "api_key": api_key,
        "streaming": streaming,
        "temperature": temperature,
        **kwargs,
    }
    if endpoint_url:
        params["base_url"] = endpoint_url
    return ChatOpenAI(**params)


def _create_anthropic(
    model: str,
    api_key: str,
    endpoint_url: str | None,
    streaming: bool,
    temperature: float,
    **kwargs: Any,
) -> BaseChatModel:
    from langchain_anthropic import ChatAnthropic

    params: dict[str, Any] = {
        "model": model,
        "api_key": api_key,
        "streaming": streaming,
        "temperature": temperature,
        **kwargs,
    }
    if endpoint_url:
        params["base_url"] = endpoint_url
    return ChatAnthropic(**params)


def _create_google(
    model: str,
    api_key: str,
    streaming: bool,
    temperature: float,
    **kwargs: Any,
) -> BaseChatModel:
    from langchain_google_genai import ChatGoogleGenerativeAI

    return ChatGoogleGenerativeAI(
        model=model,
        google_api_key=api_key,
        streaming=streaming,
        temperature=temperature,
        **kwargs,
    )


def _create_mistral(
    model: str,
    api_key: str,
    endpoint_url: str | None,
    streaming: bool,
    temperature: float,
    **kwargs: Any,
) -> BaseChatModel:
    from langchain_mistralai import ChatMistralAI

    params: dict[str, Any] = {
        "model": model,
        "api_key": api_key,
        "streaming": streaming,
        "temperature": temperature,
        **kwargs,
    }
    if endpoint_url:
        params["endpoint"] = endpoint_url
    return ChatMistralAI(**params)
