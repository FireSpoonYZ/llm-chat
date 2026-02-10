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

    def test_provider_name_case_insensitive(self):
        """Provider name should be normalized to lowercase."""
        with patch("src.providers._create_openai") as mock:
            mock.return_value = MagicMock()
            create_chat_model("OpenAI", "gpt-4o", "key123")
            mock.assert_called_once()

    def test_provider_name_strips_whitespace(self):
        with patch("src.providers._create_openai") as mock:
            mock.return_value = MagicMock()
            create_chat_model("  openai  ", "gpt-4o", "key123")
            mock.assert_called_once()

    @patch("src.providers._create_openai")
    def test_openai_provider(self, mock_create):
        mock_create.return_value = MagicMock()
        result = create_chat_model("openai", "gpt-4o", "sk-test")
        mock_create.assert_called_once_with(
            "gpt-4o", "sk-test", None, True, 0.0,
        )
        assert result is mock_create.return_value

    @patch("src.providers._create_anthropic")
    def test_anthropic_provider(self, mock_create):
        mock_create.return_value = MagicMock()
        result = create_chat_model("anthropic", "claude-sonnet-4-20250514", "sk-ant-test")
        mock_create.assert_called_once_with(
            "claude-sonnet-4-20250514", "sk-ant-test", None, True, 0.0,
        )
        assert result is mock_create.return_value

    @patch("src.providers._create_google")
    def test_google_provider(self, mock_create):
        mock_create.return_value = MagicMock()
        result = create_chat_model("google", "gemini-pro", "goog-key")
        mock_create.assert_called_once_with(
            "gemini-pro", "goog-key", True, 0.0,
        )
        assert result is mock_create.return_value

    @patch("src.providers._create_mistral")
    def test_mistral_provider(self, mock_create):
        mock_create.return_value = MagicMock()
        result = create_chat_model("mistral", "mistral-large", "mist-key")
        mock_create.assert_called_once_with(
            "mistral-large", "mist-key", None, True, 0.0,
        )
        assert result is mock_create.return_value

    @patch("src.providers._create_openai")
    def test_custom_endpoint_url(self, mock_create):
        mock_create.return_value = MagicMock()
        create_chat_model(
            "openai", "gpt-4o", "key",
            endpoint_url="https://custom.api.com/v1",
        )
        mock_create.assert_called_once_with(
            "gpt-4o", "key", "https://custom.api.com/v1", True, 0.0,
        )

    @patch("src.providers._create_openai")
    def test_custom_temperature(self, mock_create):
        mock_create.return_value = MagicMock()
        create_chat_model("openai", "gpt-4o", "key", temperature=0.7)
        mock_create.assert_called_once_with(
            "gpt-4o", "key", None, True, 0.7,
        )

    @patch("src.providers._create_openai")
    def test_streaming_disabled(self, mock_create):
        mock_create.return_value = MagicMock()
        create_chat_model("openai", "gpt-4o", "key", streaming=False)
        mock_create.assert_called_once_with(
            "gpt-4o", "key", None, False, 0.0,
        )


class TestSupportedProviders:
    def test_all_providers_listed(self):
        assert "openai" in SUPPORTED_PROVIDERS
        assert "anthropic" in SUPPORTED_PROVIDERS
        assert "google" in SUPPORTED_PROVIDERS
        assert "mistral" in SUPPORTED_PROVIDERS

    def test_provider_count(self):
        assert len(SUPPORTED_PROVIDERS) == 4


class TestOpenAICreation:
    @patch("langchain_openai.ChatOpenAI")
    def test_creates_with_correct_params(self, MockChatOpenAI):
        from src.providers import _create_openai
        _create_openai("gpt-4o", "sk-test", None, True, 0.0)
        MockChatOpenAI.assert_called_once_with(
            model="gpt-4o",
            api_key="sk-test",
            streaming=True,
            temperature=0.0,
        )

    @patch("langchain_openai.ChatOpenAI")
    def test_creates_with_custom_endpoint(self, MockChatOpenAI):
        from src.providers import _create_openai
        _create_openai("gpt-4o", "sk-test", "https://custom.com/v1", True, 0.0)
        MockChatOpenAI.assert_called_once_with(
            model="gpt-4o",
            api_key="sk-test",
            streaming=True,
            temperature=0.0,
            base_url="https://custom.com/v1",
        )


class TestAnthropicCreation:
    @patch("langchain_anthropic.ChatAnthropic")
    def test_creates_with_correct_params(self, MockChatAnthropic):
        from src.providers import _create_anthropic
        _create_anthropic("claude-sonnet-4-20250514", "sk-ant-test", None, True, 0.0)
        MockChatAnthropic.assert_called_once_with(
            model="claude-sonnet-4-20250514",
            api_key="sk-ant-test",
            streaming=True,
            temperature=0.0,
        )

    @patch("langchain_anthropic.ChatAnthropic")
    def test_creates_with_custom_endpoint(self, MockChatAnthropic):
        from src.providers import _create_anthropic
        _create_anthropic("claude-sonnet-4-20250514", "key", "https://custom.com", True, 0.5)
        MockChatAnthropic.assert_called_once_with(
            model="claude-sonnet-4-20250514",
            api_key="key",
            streaming=True,
            temperature=0.5,
            base_url="https://custom.com",
        )


class TestMistralCreation:
    @patch("langchain_mistralai.ChatMistralAI")
    def test_creates_with_endpoint(self, MockChatMistralAI):
        from src.providers import _create_mistral
        _create_mistral("mistral-large", "key", "https://custom.com", True, 0.0)
        MockChatMistralAI.assert_called_once_with(
            model="mistral-large",
            api_key="key",
            streaming=True,
            temperature=0.0,
            endpoint="https://custom.com",
        )
