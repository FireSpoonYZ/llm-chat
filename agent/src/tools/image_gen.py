"""Image generation tool using multimodal model APIs."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import os
import re
import time
from typing import Optional, Type

import openai
from google import genai
from google.genai import types
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

# Regex to extract data URIs from chat completion content
_DATA_URI_RE = re.compile(r"data:image/(\w+);base64,([A-Za-z0-9+/=]+)")

# Google size → aspect_ratio mapping
_GOOGLE_ASPECT_RATIOS = {
    "1024x1024": "1:1",
    "1024x1536": "9:16",
    "1536x1024": "16:9",
}


class ImageGenerationInput(BaseModel):
    """Input schema for the ImageGenerationTool."""

    prompt: str = Field(..., description="A detailed description of the image to generate.")
    size: str = Field(
        default="1024x1024",
        description="Image size. Options: 1024x1024, 1024x1536, 1536x1024.",
    )
    quality: str = Field(
        default="auto",
        description="Image quality. Options: low, medium, high, auto.",
    )
    n: int = Field(
        default=1,
        description="Number of images to generate (1-4).",
        ge=1,
        le=4,
    )


class ImageGenerationTool(BaseTool):
    """Generate images using multimodal AI models (OpenAI, Google)."""

    name: str = "image_generation"
    description: str = (
        "Generate images from text descriptions using the conversation's AI model. "
        "Returns sandbox:// URLs for the generated images. "
        "Supports OpenAI and Google providers."
    )
    args_schema: Type[BaseModel] = ImageGenerationInput
    workspace: str = "/workspace"
    provider: str = "openai"
    api_key: str = ""
    endpoint_url: Optional[str] = None
    model: str = ""

    def _run(self, **kwargs) -> str:
        raise NotImplementedError("Use async _arun for image generation.")

    async def _arun(
        self,
        prompt: str,
        size: str = "1024x1024",
        quality: str = "auto",
        n: int = 1,
    ) -> str:
        if not self.model:
            raise ValueError("No model specified for image generation.")
        provider = self.provider.lower()
        if provider == "openai":
            return await self._generate_openai(prompt, size, quality, n)
        elif provider == "google":
            return await self._generate_google(prompt, size, n)
        else:
            raise ValueError(f"Provider '{self.provider}' does not support image generation. Supported: openai, google.")

    async def _generate_openai(
        self, prompt: str, size: str, quality: str, n: int
    ) -> str:
        kwargs = {"api_key": self.api_key}
        if self.endpoint_url:
            kwargs["base_url"] = self.endpoint_url
        client = openai.AsyncOpenAI(**kwargs)

        async def _single_call() -> list[tuple[bytes, str]]:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[{"role": "user", "content": prompt}],
            )
            content = response.choices[0].message.content or ""
            images = []
            for match in _DATA_URI_RE.finditer(content):
                fmt, b64 = match.group(1), match.group(2)
                ext = f".{fmt}" if fmt != "jpeg" else ".jpg"
                images.append((base64.b64decode(b64), ext))
            return images

        if n <= 1:
            all_images = await _single_call()
        else:
            results = await asyncio.gather(*[_single_call() for _ in range(n)])
            all_images = [img for batch in results for img in batch]

        if not all_images:
            return "No images were generated. The model may not support image generation."

        return self._save_images(all_images)

    async def _generate_google(self, prompt: str, size: str, n: int) -> str:
        kwargs = {"api_key": self.api_key}
        if self.endpoint_url:
            kwargs["http_options"] = {"base_url": self.endpoint_url}
        client = genai.Client(**kwargs)

        aspect_ratio = _GOOGLE_ASPECT_RATIOS.get(size, "1:1")

        async def _single_call() -> list[tuple[bytes, str]]:
            response = await client.aio.models.generate_content(
                model=self.model,
                contents=prompt,
                config=types.GenerateContentConfig(
                    response_modalities=["IMAGE"],
                    image_config=types.ImageConfig(
                        aspect_ratio=aspect_ratio,
                    ),
                ),
            )
            images = []
            for part in response.candidates[0].content.parts:
                if part.inline_data:
                    images.append((part.inline_data.data, ".png"))
            return images

        if n <= 1:
            all_images = await _single_call()
        else:
            results = await asyncio.gather(*[_single_call() for _ in range(n)])
            all_images = [img for batch in results for img in batch]

        return self._save_images(all_images)

    def _save_images(self, images: list[tuple[bytes, str]]) -> str:
        out_dir = os.path.join(self.workspace, "generated_images")
        os.makedirs(out_dir, exist_ok=True)

        results = []
        for i, (data, ext) in enumerate(images):
            ts = int(time.time() * 1000)
            h = hashlib.md5(data).hexdigest()[:8]
            filename = f"{ts}_{h}_{i}{ext}"
            filepath = os.path.join(out_dir, filename)
            with open(filepath, "wb") as f:
                f.write(data)
            sandbox_url = f"sandbox:///generated_images/{filename}"
            results.append(f"![Generated Image]({sandbox_url})")

        return "\n\n".join(results)
