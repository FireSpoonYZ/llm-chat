"""Tests for the image generation tool."""

from __future__ import annotations

import base64
import os
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.tools.image_gen import (
    ImageGenerationInput,
    ImageGenerationTool,
    _DATA_URI_RE,
    _GOOGLE_SUPPORTED_RATIOS,
    _MIME_TYPES,
    _compute_google_aspect_ratio,
    _compute_google_image_size,
    _parse_size,
    _strip_data_uri_images,
)
from .result_helpers import _rdata, _rerror, _rllm, _rsuccess, _rtext


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
        result = await tool._arun(prompt="a cat")
        assert _rsuccess(result) is False
        assert "No model specified" in _rtext(result)

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

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)
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
            expected_prompt = "test Output the image at 1536x1024 resolution. Use high quality."
            assert call_kwargs["messages"] == [{"role": "user", "content": expected_prompt}]

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

        assert _rsuccess(result) is True
        assert _rtext(result).count("sandbox://") == 2
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)
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

            result = await tool._arun(prompt="test")
            assert _rsuccess(result) is False
            assert "API rate limit" in _rtext(result) or "API rate limit" in _rerror(result)

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

        assert _rsuccess(result) is False
        assert "No images were generated" in _rtext(result)
        assert "I cannot generate images." in _rtext(result)
        assert _rdata(result).get("model_output") == "I cannot generate images."
        assert _rdata(result).get("raw_output") == "I cannot generate images."
        assert _rllm(result) == "I cannot generate images."

    @pytest.mark.asyncio
    async def test_openai_string_response_with_data_uri(self, workspace):
        """OpenAI-compatible proxies may return plain string responses."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        response_text = f"Here is your image: data:image/png;base64,{FAKE_B64}"
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=response_text)
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)

    @pytest.mark.asyncio
    async def test_openai_string_response_without_data_uri(self, workspace):
        """Plain string responses without image payload should fail gracefully."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        response_text = "I can only describe the scene in words."
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=response_text)
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert _rsuccess(result) is False
        assert "No images were generated" in _rtext(result)
        assert "I can only describe the scene in words." in _rtext(result)
        assert "str' object has no attribute 'choices" not in _rtext(result)
        assert _rdata(result).get("model_output") == "I can only describe the scene in words."
        assert _rdata(result).get("raw_output") == "I can only describe the scene in words."
        assert _rllm(result) == "I can only describe the scene in words."

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

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)
        assert ".jpg" in _rtext(result)
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)

    @pytest.mark.asyncio
    async def test_openai_prompt_includes_size_hint(self, workspace):
        """Non-default size should append resolution hint to prompt."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="a cat", size="1024x1536")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            sent_prompt = call_kwargs["messages"][0]["content"]
            assert "1024x1536 resolution" in sent_prompt

    @pytest.mark.asyncio
    async def test_openai_prompt_includes_quality_hint(self, workspace):
        """Non-auto quality should append quality hint to prompt."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="a cat", quality="high")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            sent_prompt = call_kwargs["messages"][0]["content"]
            assert "high quality" in sent_prompt

    @pytest.mark.asyncio
    async def test_openai_default_size_no_hint(self, workspace):
        """Default 1024x1024 size and auto quality should not append hints."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="a cat", size="1024x1024", quality="auto")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            sent_prompt = call_kwargs["messages"][0]["content"]
            assert sent_prompt == "a cat"

    @pytest.mark.asyncio
    async def test_openai_empty_quality_no_hint(self, workspace):
        """Empty string quality should not append hint."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="a cat", quality="")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            sent_prompt = call_kwargs["messages"][0]["content"]
            assert sent_prompt == "a cat"

    @pytest.mark.asyncio
    async def test_openai_ref_image_with_size_quality_hints(self, workspace):
        """Reference image + non-default size/quality should include hints in text part."""
        img_path = os.path.join(workspace, "ref.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(
                prompt="make it blue", reference_image="ref.png",
                size="1024x1536", quality="high",
            )

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            content = call_kwargs["messages"][0]["content"]
            assert isinstance(content, list)
            text_part = content[1]["text"]
            assert "1024x1536 resolution" in text_part
            assert "high quality" in text_part

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

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)
        assert ".png" in _rtext(result)
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)

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
            mock_types.ImageConfig.assert_called_once_with(aspect_ratio="2:3", image_size="2K")

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

        assert _rsuccess(result) is True
        assert _rtext(result).count("sandbox://") == 3
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert llm_content == _rtext(result)
        assert mock_client.aio.models.generate_content.call_count == 3

    @pytest.mark.asyncio
    async def test_google_model_text_included_in_result(self, workspace):
        """When Google returns text alongside images, text and llm_content include it."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        resp = MagicMock()
        img_part = MagicMock()
        img_part.inline_data = MagicMock()
        img_part.inline_data.data = FAKE_IMAGE_DATA
        text_part = MagicMock()
        text_part.inline_data = None
        text_part.text = "Here is the generated cat image."
        resp.candidates = [MagicMock()]
        resp.candidates[0].content.parts = [img_part, text_part]

        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types"):
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(return_value=resp)
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="a cat")

        assert _rsuccess(result) is True
        assert "Here is the generated cat image." in _rtext(result)
        assert "sandbox://" in _rtext(result)
        llm_content = _rllm(result)
        assert isinstance(llm_content, str)
        assert "Here is the generated cat image." in llm_content
        assert "sandbox://" in llm_content

    @pytest.mark.asyncio
    async def test_google_aspect_ratio_computed(self, workspace):
        """Verify aspect ratio is computed from size input."""
        assert _compute_google_aspect_ratio(1024, 1024) == "1:1"
        assert _compute_google_aspect_ratio(1024, 1536) == "2:3"
        assert _compute_google_aspect_ratio(1536, 1024) == "3:2"
        assert _compute_google_aspect_ratio(800, 800) == "1:1"

    @pytest.mark.asyncio
    async def test_google_passes_image_size(self, workspace):
        """Verify image_size is passed to ImageConfig for all known sizes."""
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

            await tool._arun(prompt="test", size="1024x1024")

            mock_types.ImageConfig.assert_called_once_with(
                aspect_ratio="1:1", image_size="1K",
            )

    def test_google_image_size_tiers(self):
        """Verify image_size tier selection based on total pixels."""
        assert _compute_google_image_size(800, 800) == "1K"
        assert _compute_google_image_size(1024, 1024) == "1K"
        assert _compute_google_image_size(1024, 1536) == "2K"
        assert _compute_google_image_size(1536, 1024) == "2K"
        assert _compute_google_image_size(2048, 2048) == "2K"
        assert _compute_google_image_size(4096, 4096) == "4K"

    def test_google_aspect_ratio_closest_match(self):
        """Non-standard ratios should snap to the closest supported ratio."""
        # 1920x1080 = 16:9
        assert _compute_google_aspect_ratio(1920, 1080) == "16:9"
        # 500x1000 = 1:2, closest is 9:16 (0.5625)
        assert _compute_google_aspect_ratio(500, 1000) == "9:16"

    def test_google_aspect_ratio_all_supported(self):
        """All supported ratios should be matched exactly when given matching dimensions."""
        for w, h in _GOOGLE_SUPPORTED_RATIOS:
            # Direct match
            assert _compute_google_aspect_ratio(w, h) == f"{w}:{h}"
            # Scaled up
            assert _compute_google_aspect_ratio(w * 100, h * 100) == f"{w}:{h}"

    def test_google_aspect_ratio_extreme(self):
        """Extreme ratios should snap to the closest supported option."""
        # Very wide → 21:9
        assert _compute_google_aspect_ratio(2100, 100) == "21:9"
        # Very tall → 9:16
        assert _compute_google_aspect_ratio(100, 2000) == "9:16"

    def test_google_image_size_boundaries(self):
        """Boundary values at tier transitions."""
        # Exactly at 1K boundary (1024*1024 = 1048576)
        assert _compute_google_image_size(1024, 1024) == "1K"
        # Just above 1K
        assert _compute_google_image_size(1025, 1024) == "2K"
        # Exactly at 2K boundary (2048*2048 = 4194304)
        assert _compute_google_image_size(2048, 2048) == "2K"
        # Just above 2K
        assert _compute_google_image_size(2049, 2048) == "4K"

    @pytest.mark.asyncio
    async def test_google_nonstandard_size_end_to_end(self, workspace):
        """Non-standard size like 1920x1080 should compute 16:9 + 2K and pass to ImageConfig."""
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

            result = await tool._arun(prompt="test", size="1920x1080")

            mock_types.ImageConfig.assert_called_once_with(
                aspect_ratio="16:9", image_size="2K",
            )
            assert _rsuccess(result) is True
            assert "sandbox://" in _rtext(result)

    def test_parse_size(self):
        assert _parse_size("1024x1536") == (1024, 1536)
        assert _parse_size("800x800") == (800, 800)

    def test_input_schema_descriptions_clarify_provider_behavior(self):
        if hasattr(ImageGenerationInput, "model_json_schema"):
            schema = ImageGenerationInput.model_json_schema()
        else:
            schema = ImageGenerationInput.schema()

        size_desc = schema["properties"]["size"]["description"].lower()
        quality_desc = schema["properties"]["quality"]["description"].lower()

        assert "wxh" in size_desc
        assert "best-effort" in size_desc
        assert "openai" in size_desc
        assert "google" in size_desc
        assert "1k/2k/4k" in size_desc
        assert "openai only" in quality_desc
        assert "ignored by google" in quality_desc

    def test_parse_size_invalid(self):
        with pytest.raises((ValueError, IndexError)):
            _parse_size("abc")
        with pytest.raises((ValueError, IndexError)):
            _parse_size("1024")
        with pytest.raises((ValueError, IndexError)):
            _parse_size("")

    # --- Provider tests ---

    @pytest.mark.asyncio
    async def test_unsupported_provider_mistral(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="mistral", api_key="test-key", model="m",
        )
        result = await tool._arun(prompt="test")
        assert _rsuccess(result) is False
        assert "does not support image generation" in _rtext(result)

    @pytest.mark.asyncio
    async def test_unsupported_provider_anthropic(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="anthropic", api_key="test-key", model="m",
        )
        result = await tool._arun(prompt="test")
        assert _rsuccess(result) is False
        assert "does not support image generation" in _rtext(result)

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

    # --- Reference image tests ---

    @pytest.mark.asyncio
    async def test_reference_image_not_found(self, workspace):
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        result = await tool._arun(prompt="make it blue", reference_image="nonexistent.png")
        assert _rsuccess(result) is False
        assert "not found" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_unsupported_format(self, workspace):
        bmp_path = os.path.join(workspace, "photo.bmp")
        with open(bmp_path, "wb") as f:
            f.write(b"\x00" * 10)
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        result = await tool._arun(prompt="edit this", reference_image="photo.bmp")
        assert _rsuccess(result) is False
        assert "Unsupported image format" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_resolves_relative_path(self, workspace):
        """Relative paths should resolve against the workspace directory."""
        sub = os.path.join(workspace, "imgs")
        os.makedirs(sub)
        img_path = os.path.join(sub, "ref.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="edit", reference_image="imgs/ref.png")

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)

    @pytest.mark.asyncio
    async def test_openai_with_reference_image(self, workspace):
        """OpenAI should receive multimodal message with image data URI + text."""
        img_path = os.path.join(workspace, "ref.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            await tool._arun(prompt="make it blue", reference_image="ref.png")

            call_kwargs = mock_client.chat.completions.create.call_args[1]
            messages = call_kwargs["messages"]
            assert len(messages) == 1
            content = messages[0]["content"]
            # Should be multimodal list, not plain string
            assert isinstance(content, list)
            assert len(content) == 2
            assert content[0]["type"] == "image_url"
            assert content[0]["image_url"]["url"].startswith("data:image/png;base64,")
            assert content[1] == {"type": "text", "text": "make it blue"}

    @pytest.mark.asyncio
    async def test_google_with_reference_image(self, workspace):
        """Google should receive multimodal contents with image bytes + text."""
        img_path = os.path.join(workspace, "ref.jpg")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

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

            await tool._arun(prompt="add a hat", reference_image="ref.jpg")

            call_kwargs = mock_client.aio.models.generate_content.call_args[1]
            # contents should be the types.Content object, not a plain string
            assert call_kwargs["contents"] == mock_types.Content.return_value
            mock_types.Part.from_bytes.assert_called_once_with(
                data=FAKE_IMAGE_DATA, mime_type="image/jpeg",
            )
            mock_types.Part.from_text.assert_called_once_with(text="add a hat")
            mock_types.Content.assert_called_once()

    @pytest.mark.asyncio
    async def test_reference_image_path_traversal_dotdot(self, workspace):
        """../  traversal outside workspace should be rejected."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        result = await tool._arun(prompt="edit", reference_image="../../etc/passwd")
        assert _rsuccess(result) is False
        assert "Access denied" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_path_traversal_absolute(self, workspace):
        """Absolute paths outside workspace should be rejected."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        result = await tool._arun(prompt="edit", reference_image="/etc/passwd")
        assert _rsuccess(result) is False
        assert "Access denied" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_absolute_path_within_workspace(self, workspace):
        """Absolute path inside workspace should be accepted."""
        img_path = os.path.join(workspace, "photo.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="edit", reference_image=img_path)

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_dot_slash_relative(self, workspace):
        """./file.png style relative path should resolve correctly."""
        img_path = os.path.join(workspace, "input.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="edit", reference_image="./input.png")

        assert _rsuccess(result) is True
        assert "sandbox://" in _rtext(result)

    @pytest.mark.asyncio
    async def test_reference_image_with_multiple_n(self, workspace):
        """reference_image + n>1 should send the same multimodal content n times."""
        img_path = os.path.join(workspace, "ref.png")
        with open(img_path, "wb") as f:
            f.write(FAKE_IMAGE_DATA)

        tool = ImageGenerationTool(
            workspace=workspace, provider="openai", api_key="sk-test", model="gpt-4o",
        )
        with patch("src.tools.image_gen.openai.AsyncOpenAI") as mock_cls:
            mock_client = AsyncMock()
            mock_client.chat.completions.create = AsyncMock(return_value=_openai_response(FAKE_B64))
            mock_cls.return_value = mock_client

            result = await tool._arun(prompt="variations", reference_image="ref.png", n=2)

        assert _rsuccess(result) is True
        assert _rtext(result).count("sandbox://") == 2
        assert mock_client.chat.completions.create.call_count == 2
        # Both calls should use multimodal content
        for call in mock_client.chat.completions.create.call_args_list:
            content = call[1]["messages"][0]["content"]
            assert isinstance(content, list)

    def test_mime_types_mapping(self):
        """Verify all MIME type mappings are correct."""
        assert _MIME_TYPES == {
            ".png": "image/png",
            ".jpg": "image/jpeg",
            ".jpeg": "image/jpeg",
            ".gif": "image/gif",
            ".webp": "image/webp",
        }

    # --- _strip_data_uri_images tests ---

    def test_strip_data_uri_images_removes_data_uri_markdown(self):
        text = f"Here is your image: ![cat](data:image/png;base64,{FAKE_B64})"
        assert _strip_data_uri_images(text) == "Here is your image:"

    def test_strip_data_uri_images_preserves_non_data_uri(self):
        text = "![cat](https://example.com/cat.png) and some text"
        assert _strip_data_uri_images(text) == text

    def test_strip_data_uri_images_empty_after_strip(self):
        text = f"![img](data:image/jpeg;base64,{FAKE_B64})"
        assert _strip_data_uri_images(text) == ""

    # --- Google null/empty response tests ---

    @pytest.mark.asyncio
    async def test_google_no_image_in_response(self, workspace):
        """When Google returns parts with no inline_data, return fallback message."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        resp = MagicMock()
        text_part = MagicMock()
        text_part.inline_data = None
        text_part.text = "Image generation is blocked by policy."
        resp.candidates = [MagicMock()]
        resp.candidates[0].content.parts = [text_part]

        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types"):
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(return_value=resp)
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert _rsuccess(result) is False
        assert "No images were generated" in _rtext(result)
        assert "Image generation is blocked by policy." in _rtext(result)
        assert _rdata(result).get("model_output") == "Image generation is blocked by policy."
        assert _rdata(result).get("raw_output") == "Image generation is blocked by policy."
        assert _rllm(result) == "Image generation is blocked by policy."

    @pytest.mark.asyncio
    async def test_google_null_parts_returns_no_images(self, workspace):
        """When Google returns content.parts=None (policy rejection), return fallback."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        resp = MagicMock()
        resp.candidates = [MagicMock()]
        resp.candidates[0].content.parts = None

        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types"):
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(return_value=resp)
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert _rsuccess(result) is False
        assert "No images were generated" in _rtext(result)
        raw_output = _rdata(result).get("raw_output")
        assert isinstance(raw_output, str)
        assert raw_output
        assert _rllm(result) == raw_output

    @pytest.mark.asyncio
    async def test_google_empty_candidates_returns_no_images(self, workspace):
        """When Google returns candidates=[], return fallback."""
        tool = ImageGenerationTool(
            workspace=workspace, provider="google", api_key="google-key",
            model="gemini-2.0-flash",
        )
        resp = MagicMock()
        resp.candidates = []

        with patch("src.tools.image_gen.genai") as mock_genai, \
             patch("src.tools.image_gen.types"):
            mock_client = MagicMock()
            mock_client.aio.models.generate_content = AsyncMock(return_value=resp)
            mock_genai.Client.return_value = mock_client

            result = await tool._arun(prompt="test")

        assert _rsuccess(result) is False
        assert "No images were generated" in _rtext(result)
        raw_output = _rdata(result).get("raw_output")
        assert isinstance(raw_output, str)
        assert raw_output
        assert _rllm(result) == raw_output
