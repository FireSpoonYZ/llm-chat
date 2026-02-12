"""Tests for the providers module."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from src.providers import SUPPORTED_PROVIDERS, create_chat_model


class TestCreateChatModel:
    """Tests for create_chat_model factory."""

    def test_unsupported_provider_raises(self):
        with pytest.raises(ValueError, match="Unsupported provider"):
            create_chat_model("nonexistent", "model", "key")

    def test_unsupported_provider_message_lists_supported(self):
        with pytest.raises(ValueError, match="openai"):
            create_chat_model("bad", "model", "key")

    @patch("src.providers.init_chat_model")
    def test_provider_name_case_insensitive(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("OpenAI", "gpt-4o", "key123")
        mock_init.assert_called_once()
        assert mock_init.call_args.kwargs["model_provider"] == "openai"

    @patch("src.providers.init_chat_model")
    def test_provider_name_strips_whitespace(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("  openai  ", "gpt-4o", "key123")
        mock_init.assert_called_once()

    @patch("src.providers.init_chat_model")
    def test_openai_provider(self, mock_init):
        mock_init.return_value = MagicMock()
        result = create_chat_model("openai", "gpt-4o", "sk-test")
        mock_init.assert_called_once_with(
            model="gpt-4o",
            model_provider="openai",
            api_key="sk-test",
            streaming=True,
            temperature=0.0,
        )
        assert result is mock_init.return_value

    @patch("src.providers.init_chat_model")
    def test_anthropic_provider(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("anthropic", "claude-sonnet-4-20250514", "sk-ant-test")
        mock_init.assert_called_once_with(
            model="claude-sonnet-4-20250514",
            model_provider="anthropic",
            api_key="sk-ant-test",
            streaming=True,
            temperature=0.0,
        )

    @patch("src.providers.init_chat_model")
    def test_google_provider(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("google", "gemini-pro", "goog-key")
        mock_init.assert_called_once_with(
            model="gemini-pro",
            model_provider="google_genai",
            api_key="goog-key",
            streaming=True,
            temperature=0.0,
        )

    @patch("src.providers.init_chat_model")
    def test_mistral_provider(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("mistral", "mistral-large", "mist-key")
        mock_init.assert_called_once_with(
            model="mistral-large",
            model_provider="mistralai",
            api_key="mist-key",
            streaming=True,
            temperature=0.0,
        )

    @patch("src.providers.init_chat_model")
    def test_openai_custom_endpoint(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model(
            "openai", "gpt-4o", "key",
            endpoint_url="https://custom.api.com/v1",
        )
        mock_init.assert_called_once_with(
            model="gpt-4o",
            model_provider="openai",
            api_key="key",
            streaming=True,
            temperature=0.0,
            base_url="https://custom.api.com/v1",
        )

    @patch("src.providers.init_chat_model")
    def test_anthropic_custom_endpoint(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model(
            "anthropic", "claude-sonnet-4-20250514", "key",
            endpoint_url="https://custom.com",
        )
        mock_init.assert_called_once_with(
            model="claude-sonnet-4-20250514",
            model_provider="anthropic",
            api_key="key",
            streaming=True,
            temperature=0.0,
            base_url="https://custom.com",
        )

    @patch("src.providers.init_chat_model")
    def test_mistral_custom_endpoint(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model(
            "mistral", "mistral-large", "key",
            endpoint_url="https://custom.com",
        )
        mock_init.assert_called_once_with(
            model="mistral-large",
            model_provider="mistralai",
            api_key="key",
            streaming=True,
            temperature=0.0,
            endpoint="https://custom.com",
        )

    @patch("src.providers.init_chat_model")
    def test_google_ignores_endpoint_url(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model(
            "google", "gemini-pro", "key",
            endpoint_url="https://custom.com",
        )
        # Google should not get base_url or endpoint
        mock_init.assert_called_once_with(
            model="gemini-pro",
            model_provider="google_genai",
            api_key="key",
            streaming=True,
            temperature=0.0,
        )

    @patch("src.providers.init_chat_model")
    def test_custom_temperature(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("openai", "gpt-4o", "key", temperature=0.7)
        assert mock_init.call_args.kwargs["temperature"] == 0.7

    @patch("src.providers.init_chat_model")
    def test_streaming_disabled(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("openai", "gpt-4o", "key", streaming=False)
        assert mock_init.call_args.kwargs["streaming"] is False

    @patch("src.providers.init_chat_model")
    def test_extra_kwargs_passed_through(self, mock_init):
        mock_init.return_value = MagicMock()
        create_chat_model("openai", "gpt-4o", "key", max_tokens=100)
        assert mock_init.call_args.kwargs["max_tokens"] == 100


class TestSupportedProviders:
    def test_all_providers_listed(self):
        assert "openai" in SUPPORTED_PROVIDERS
        assert "anthropic" in SUPPORTED_PROVIDERS
        assert "google" in SUPPORTED_PROVIDERS
        assert "mistral" in SUPPORTED_PROVIDERS

    def test_provider_count(self):
        assert len(SUPPORTED_PROVIDERS) == 4
