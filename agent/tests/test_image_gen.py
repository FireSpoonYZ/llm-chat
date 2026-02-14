"""Tests for the image generation tool."""

from __future__ import annotations

import base64
import os
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.tools.image_gen import ImageGenerationTool, _DATA_URI_RE, _GOOGLE_ASPECT_RATIOS


FAKE_IMAGE_DATA = b"\x89PNG\r\n\x1a\n" + b"\x00" * 100
FAKE_B64 = base64.b64encode(FAKE_IMAGE_DATA).decode()


def _openai_response(*b64_results: str, fmt: str = "png") -> MagicMock:
    """Build a mock OpenAI chat.completions.create result with data URIs."""
    resp = MagicMock()
    parts = [f"![image](data:image/{fmt};base64,{b64})" for b64 in b64_results]
    content = "\n\n".join(parts)
    resp.choices = [MagicMock()]
    resp.choices[0].message.content = content
    return resp


def _google_response(*image_datas: bytes) -> MagicMock:
    """Build a mock Google generate_content result."""
    resp = MagicMock()
    parts = []
    for data in image_datas:
        part = MagicMock()
        part.inline_data = MagicMock()
        part.inline_data.data = data
        parts.append(part)
    resp.candidates = [MagicMock()]
    resp.candidates[0].content.parts = parts
    return resp


class TestImageGenerationTool:
    def test_sync_raises(self):
        tool = ImageGenerationTool(
            workspace="/workspace", provider="openai", api_key="test-key", model="gpt-4o",
        )
        with pytest.raises(NotImplementedError):
            tool._run(prompt="a cat")

    @pytest.mark.asyncio
    async def test_empty_model_raises(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="",
        )
        with pytest.raises(ValueError, match="No model specified"):
            await tool._arun(prompt="a cat")

    # --- OpenAI tests ---

    @pytest.mark.asyncio
    async def test_openai_generates_image(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="a cute cat")

        assert "sandbox://" in result
        gen_dir = os.path.join(workspace, "generated_images")
        assert os.path.isdir(gen_dir)
        assert len(os.listdir(gen_dir)) == 1

    @pytest.mark.asyncio
    async def test_openai_passes_model_and_messages(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="test", size="1536x1024", quality="high")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            assert call_kwargs["model"] == "gpt-4o"
            assert call_kwargs["messages"] == [{"role": "user", "content": "test"}]

    @pytest.mark.asyncio
    async def test_openai_with_endpoint_url(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test",
            model="gpt-4o", endpoint_url="https://custom.api.com/v1",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="test")

            mock_cls.assert_called_once_with(api_key="sk-test", base_url="https://custom.api.com/v1")

    @pytest.mark.asyncio
    async def test_openai_multiple_images(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="two cats", n=2)

        assert result.count("sandbox://") == 2
        assert len(os.listdir(os.path.join(workspace, "generated_images"))) == 2
        assert mock_client.chat.completions.create.call_count == 2

    @pytest.mark.asyncio
    async def test_openai_api_error(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(side_effect=Exception("API rate limit"))
            mock_cls.return_value = mock_client

            with pytest.raises(Exception, match="API rate limit"):
                await tool._arun(prompt="test")

    @pytest.mark.asyncio
    async def test_openai_no_image_in_response(self, workspace):
        """When model returns text without data URIs, return a helpful message."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        resp = MagicMock()
        resp.choices = [MagicMock()]
        resp.choices[0].message.content = "I cannot generate images."
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=resp)
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert "No images were generated" in result

    @pytest.mark.asyncio
    async def test_openai_jpeg_format(self, workspace):
        """Verify JPEG data URIs are parsed and saved as .jpg."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(
                return_value=_openai_response(FAKE_B64, fmt="jpeg")
            )
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert "sandbox://" in result
        assert ".jpg" in result

    # --- Google tests ---

    @pytest.mark.asyncio
    async def test_google_generates_image(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key", model="gemini-2.0-flash",
        )
        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types") as mock_types:
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(
                return_value=_google_response(FAKE_IMAGE_DATA)
            )
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="a dog")

        assert "sandbox://" in result
        assert ".png" in result

    @pytest.mark.asyncio
    async def test_google_passes_model_and_config(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types") as mock_types:
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(
                return_value=_google_response(FAKE_IMAGE_DATA)
            )
            mock_genai.Client.return_value = mock_client

            await tool._arun(prompt="test", size="1024x1536")

            call_kwargs = mock_client.aio.models.generate_content.call_args[1]
            assert call_kwargs["model"] == "gemini-2.0-flash"
            assert call_kwargs["contents"] == "test"
            # Verify config was built with correct types calls
            mock_types.GenerateContentConfig.assert_called_once()
            mock_types.ImageConfig.assert_called_once_with(aspect_ratio="9:16")

    @pytest.mark.asyncio
    async def test_google_with_endpoint_url(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash", endpoint_url="https://custom.google.proxy/v1",
        )
        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types") as mock_types:
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(
                return_value=_google_response(FAKE_IMAGE_DATA)
            )
            mock_genai.Client.return_value = mock_client

            await tool._arun(prompt="test")

            mock_genai.Client.assert_called_once_with(
                api_key="google-key",
                http_options={"base_url": "https://custom.google.proxy/v1"},
            )

    @pytest.mark.asyncio
    async def test_google_multiple_images(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types") as mock_types:
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(
                return_value=_google_response(FAKE_IMAGE_DATA)
            )
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="dogs", n=3)

        assert result.count("sandbox://") == 3
        assert mock_client.aio.models.generate_content.call_count == 3

    @pytest.mark.asyncio
    async def test_google_aspect_ratio_mapping(self, workspace):
        """Verify all size→aspect_ratio mappings."""
        assert _GOOGLE_ASPECT_RATIOS == {
            "1024x1024": "1:1",
            "1024x1536": "9:16",
            "1536x1024": "16:9",
        }

    # --- Provider tests ---

    @pytest.mark.asyncio
    async def test_unsupported_provider_mistral(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="mistral", api_key="test-key", model="m",
        )
        with pytest.raises(ValueError, match="does not support image generation"):
            await tool._arun(prompt="test")

    @pytest.mark.asyncio
    async def test_unsupported_provider_anthropic(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="anthropic", api_key="test-key", model="m",
        )
        with pytest.raises(ValueError, match="does not support image generation"):
            await tool._arun(prompt="test")

    def test_tool_metadata(self):
        tool = ImageGenerationTool(
            workspace="/workspace", provider="openai", api_key="test", model="gpt-4o",
        )
        assert tool.name == "image_generation"
        assert "image" in tool.description.lower()

    @pytest.mark.asyncio
    async def test_uses_conversation_model_openai(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test",
            model="my-custom-model",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="test")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            assert call_kwargs["model"] == "my-custom-model"

    @pytest.mark.asyncio
    async def test_uses_conversation_model_google(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="my-gemini-model",
        )
        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types") as mock_types:
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(
                return_value=_google_response(FAKE_IMAGE_DATA)
            )
            mock_genai.Client.return_value = mock_client

            await tool._arun(prompt="test")

            call_kwargs = mock_client.aio.models.generate_content.call_args[1]
            assert call_kwargs["model"] == "my-gemini-model"
