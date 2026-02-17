"""Image generation tool using multimodal model APIs."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import math
import os
import re
import time
from typing import Any, Optional, Type

import openai
from google import genai
from google.genai import types
from langchain_core.tools import BaseTool
from pydantic import BaseModel, Field

from ._paths import resolve_workspace_path
from .result_schema import make_tool_error, make_tool_success

# Regex to extract data URIs from chat completion content
_DATA_URI_RE = re.compile(r"data:image/(\w+);base64,([A-Za-z0-9+/=]+)")

# Google supported aspect ratios
_GOOGLE_SUPPORTED_RATIOS = [
    (1, 1), (2, 3), (3, 2), (3, 4), (4, 3),
    (4, 5), (5, 4), (9, 16), (16, 9), (21, 9),
]

# Supported reference image MIME types
_MIME_TYPES = {
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".gif": "image/gif",
    ".webp": "image/webp",
}


def _parse_size(size: str) -> tuple[int, int]:
    """Parse 'WxH' string into (width, height)."""
    parts = size.lower().split("x")
    return int(parts[0]), int(parts[1])


def _compute_google_aspect_ratio(width: int, height: int) -> str:
    """Find the closest supported Google aspect ratio for the given dimensions."""
    g = math.gcd(width, height)
    simplified = (width // g, height // g)
    # Check exact match first
    for w, h in _GOOGLE_SUPPORTED_RATIOS:
        if simplified == (w, h):
            return f"{w}:{h}"
    # Fall back to closest ratio by value
    target = width / height
    best = min(_GOOGLE_SUPPORTED_RATIOS, key=lambda r: abs(r[0] / r[1] - target))
    return f"{best[0]}:{best[1]}"


def _compute_google_image_size(width: int, height: int) -> str:
    """Pick the smallest Google resolution tier that covers the requested pixels."""
    total = width * height
    if total <= 1024 * 1024:
        return "1K"
    if total <= 2048 * 2048:
        return "2K"
    return "4K"


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
    reference_image: Optional[str] = Field(
        default=None,
        description="Path to a reference image file to edit/modify. When provided, the prompt describes how to modify this image.",
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

    def _run(self, **kwargs) -> dict[str, Any]:
        raise NotImplementedError("Use async _arun for image generation.")

    async def _arun(
        self,
        prompt: str,
        size: str = "1024x1024",
        quality: str = "auto",
        n: int = 1,
        reference_image: Optional[str] = None,
    ) -> dict[str, Any]:
        if not self.model:
            return make_tool_error(kind=self.name, error="No model specified for image generation")

        try:
            ref_data = None
            if reference_image:
                ref_data = self._load_reference_image(reference_image)

            provider = self.provider.lower()
            if provider == "openai":
                images = await self._generate_openai(prompt, size, quality, n, ref_data)
            elif provider == "google":
                images = await self._generate_google(prompt, size, n, ref_data)
            else:
                return make_tool_error(
                    kind=self.name,
                    error=(
                        f"Provider '{self.provider}' does not support image generation. "
                        "Supported: openai, google"
                    ),
                )

            if not images:
                return make_tool_error(
                    kind=self.name,
                    error="No images were generated. The model may not support image generation",
                )

            text, media = self._save_images(images)
            return make_tool_success(
                kind=self.name,
                text=text,
                data={
                    "prompt": prompt,
                    "size": size,
                    "quality": quality,
                    "requested": n,
                    "provider": provider,
                    "media": media,
                },
                meta={"image_count": len(media)},
            )
        except Exception as exc:
            return make_tool_error(kind=self.name, error=f"image generation failed: {exc}")

    def _load_reference_image(self, path: str) -> tuple[bytes, str]:
        """Load a reference image from the workspace and return (bytes, mime_type)."""
        resolved = resolve_workspace_path(path, self.workspace)
        if not resolved.is_file():
            raise ValueError(f"Reference image not found: {path}")
        ext = resolved.suffix.lower()
        mime_type = _MIME_TYPES.get(ext)
        if not mime_type:
            raise ValueError(f"Unsupported image format '{ext}'. Supported: {', '.join(_MIME_TYPES)}")
        return resolved.read_bytes(), mime_type

    async def _generate_openai(
        self, prompt: str, size: str, quality: str, n: int,
        ref_data: Optional[tuple[bytes, str]] = None,
    ) -> list[tuple[bytes, str]]:
        kwargs = {"api_key": self.api_key}
        if self.endpoint_url:
            kwargs["base_url"] = self.endpoint_url
        client = openai.AsyncOpenAI(**kwargs)

        # Append size/quality hints to prompt (best-effort for Chat Completions API)
        size_hint = f" Output the image at {size} resolution." if size != "1024x1024" else ""
        quality_hint = f" Use {quality} quality." if quality not in ("auto", "") else ""
        effective_prompt = prompt + size_hint + quality_hint

        if ref_data:
            image_bytes, mime_type = ref_data
            b64 = base64.b64encode(image_bytes).decode()
            message_content: str | list[dict[str, Any]] = [
                {"type": "image_url", "image_url": {"url": f"data:{mime_type};base64,{b64}"}},
                {"type": "text", "text": effective_prompt},
            ]
        else:
            message_content = effective_prompt

        async def _single_call() -> list[tuple[bytes, str]]:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[{"role": "user", "content": message_content}],
            )
            content = response.choices[0].message.content or ""
            images: list[tuple[bytes, str]] = []
            for match in _DATA_URI_RE.finditer(content):
                fmt, b64 = match.group(1), match.group(2)
                ext = f".{fmt}" if fmt != "jpeg" else ".jpg"
                images.append((base64.b64decode(b64), ext))
            return images

        if n <= 1:
            return await _single_call()

        results = await asyncio.gather(*[_single_call() for _ in range(n)])
        return [img for batch in results for img in batch]

    async def _generate_google(
        self, prompt: str, size: str, n: int,
        ref_data: Optional[tuple[bytes, str]] = None,
    ) -> list[tuple[bytes, str]]:
        kwargs = {"api_key": self.api_key}
        if self.endpoint_url:
            kwargs["http_options"] = {"base_url": self.endpoint_url}
        client = genai.Client(**kwargs)

        width, height = _parse_size(size)
        aspect_ratio = _compute_google_aspect_ratio(width, height)
        image_size = _compute_google_image_size(width, height)

        if ref_data:
            image_bytes, mime_type = ref_data
            contents: str | types.Content = types.Content(
                role="user",
                parts=[
                    types.Part.from_bytes(data=image_bytes, mime_type=mime_type),
                    types.Part.from_text(text=prompt),
                ],
            )
        else:
            contents = prompt

        async def _single_call() -> list[tuple[bytes, str]]:
            response = await client.aio.models.generate_content(
                model=self.model,
                contents=contents,
                config=types.GenerateContentConfig(
                    response_modalities=["IMAGE"],
                    image_config=types.ImageConfig(
                        aspect_ratio=aspect_ratio,
                        image_size=image_size,
                    ),
                ),
            )
            images: list[tuple[bytes, str]] = []
            candidates = response.candidates or []
            if not candidates or not candidates[0].content:
                return images
            parts = candidates[0].content.parts or []
            for part in parts:
                if part.inline_data:
                    images.append((part.inline_data.data, ".png"))
            return images

        if n <= 1:
            return await _single_call()

        results = await asyncio.gather(*[_single_call() for _ in range(n)])
        return [img for batch in results for img in batch]

    def _save_images(self, images: list[tuple[bytes, str]]) -> tuple[str, list[dict[str, Any]]]:
        out_dir = os.path.join(self.workspace, "generated_images")
        os.makedirs(out_dir, exist_ok=True)

        markdown: list[str] = []
        media: list[dict[str, Any]] = []
        for i, (data, ext) in enumerate(images):
            ts = int(time.time() * 1000)
            h = hashlib.md5(data).hexdigest()[:8]
            filename = f"{ts}_{h}_{i}{ext}"
            filepath = os.path.join(out_dir, filename)
            with open(filepath, "wb") as f:
                f.write(data)
            sandbox_url = f"sandbox:///generated_images/{filename}"
            markdown.append(f"![Generated Image]({sandbox_url})")
            media.append(
                {
                    "type": "image",
                    "name": filename,
                    "url": sandbox_url,
                    "size": len(data),
                }
            )

        return "\n\n".join(markdown), media
