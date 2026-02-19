"""Image generation tool using multimodal model APIs."""

from __future__ import annotations

import asyncio
import base64
import hashlib
import json
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

# Regex to strip markdown images with data URI sources (e.g. ![alt](data:image/...))
_DATA_URI_IMAGE_RE = re.compile(r"!\[[^\]]*\]\(data:image/[^)]+\)")


def _strip_data_uri_images(text: str) -> str:
    """Remove markdown image syntax containing data URIs, keeping other text."""
    return _DATA_URI_IMAGE_RE.sub("", text).strip()

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


def _get_field(obj: Any, key: str) -> Any:
    """Read a field from dict-like objects and SDK objects."""
    if isinstance(obj, dict):
        return obj.get(key)
    if obj is None:
        return None

    # unittest.mock stores chained attributes under _mock_children.
    if type(obj).__module__.startswith("unittest.mock"):
        obj_dict = getattr(obj, "__dict__", None)
        if isinstance(obj_dict, dict) and key in obj_dict:
            return obj_dict[key]
        children = getattr(obj, "_mock_children", None)
        if isinstance(children, dict) and key in children:
            return children[key]
        return None

    # Prefer instance storage so dynamic __getattr__ implementations (e.g. mocks)
    # do not fabricate fields and cause recursive walks.
    obj_dict = getattr(obj, "__dict__", None)
    if isinstance(obj_dict, dict) and key in obj_dict:
        return obj_dict[key]

    # Fall back to declared class attributes/properties only.
    try:
        class_dict = vars(type(obj))
    except TypeError:
        class_dict = {}
    if key in class_dict:
        return getattr(obj, key, None)
    return None


def _extract_text_from_content(content: Any) -> list[str]:
    """Extract text-ish values from OpenAI message/content payloads."""
    texts: list[str] = []
    if content is None:
        return texts

    if isinstance(content, str):
        stripped = content.strip()
        if stripped:
            texts.append(stripped)
        return texts

    if isinstance(content, list):
        for item in content:
            texts.extend(_extract_text_from_content(item))
        return texts

    if isinstance(content, dict):
        for key in ("text", "content"):
            texts.extend(_extract_text_from_content(content.get(key)))
        image_url = content.get("image_url")
        if image_url is not None:
            texts.extend(_extract_text_from_content(_get_field(image_url, "url")))
        return texts

    for key in ("text", "content"):
        texts.extend(_extract_text_from_content(_get_field(content, key)))
    image_url = _get_field(content, "image_url")
    if image_url is not None:
        texts.extend(_extract_text_from_content(_get_field(image_url, "url")))
    return texts


def _extract_openai_response_text(response: Any) -> str:
    """Normalize OpenAI-compatible responses into a text blob."""
    if isinstance(response, str):
        return response.strip()

    texts: list[str] = []

    choices = _get_field(response, "choices")
    if isinstance(choices, list):
        for choice in choices:
            message = _get_field(choice, "message")
            if message is not None:
                texts.extend(_extract_text_from_content(_get_field(message, "content")))
            texts.extend(_extract_text_from_content(_get_field(choice, "content")))
            texts.extend(_extract_text_from_content(_get_field(choice, "text")))
    else:
        texts.extend(_extract_text_from_content(choices))

    for key in ("output_text", "content", "text"):
        texts.extend(_extract_text_from_content(_get_field(response, key)))

    seen: set[str] = set()
    unique_texts: list[str] = []
    for text in texts:
        if text not in seen:
            seen.add(text)
            unique_texts.append(text)
    return "\n".join(unique_texts)


def _serialize_provider_output(response: Any, fallback_text: str = "") -> str:
    """Serialize provider response into a raw string for error context."""
    fallback = fallback_text.strip()

    if response is None:
        return fallback
    if isinstance(response, str):
        return response.strip()

    if isinstance(response, (dict, list, tuple)):
        try:
            return json.dumps(response, ensure_ascii=False)
        except TypeError:
            return json.dumps(response, ensure_ascii=False, default=str)

    # MagicMock objects do not represent real provider payloads; prefer extracted text.
    if type(response).__module__.startswith("unittest.mock"):
        return fallback or repr(response).strip()

    for meth in ("model_dump_json", "to_json", "json"):
        fn = getattr(response, meth, None)
        if not callable(fn):
            continue
        try:
            rendered = fn()
        except Exception:
            continue
        if isinstance(rendered, str) and rendered.strip():
            return rendered.strip()

    for meth in ("model_dump", "to_dict", "dict"):
        fn = getattr(response, meth, None)
        if not callable(fn):
            continue
        try:
            value = fn()
        except Exception:
            continue
        if isinstance(value, (dict, list, tuple)):
            try:
                return json.dumps(value, ensure_ascii=False)
            except TypeError:
                return json.dumps(value, ensure_ascii=False, default=str)
        rendered = str(value).strip()
        if rendered:
            return rendered

    if fallback:
        return fallback
    return repr(response).strip()


def _extract_images_from_data_uris(content: str) -> list[tuple[bytes, str]]:
    """Extract image bytes from text containing data:image/...;base64 payloads."""
    images: list[tuple[bytes, str]] = []
    for match in _DATA_URI_RE.finditer(content):
        fmt, b64 = match.group(1), match.group(2)
        ext = f".{fmt}" if fmt != "jpeg" else ".jpg"
        images.append((base64.b64decode(b64), ext))
    return images


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
        description=(
            "Desired image size in 'WxH' format (positive integers), e.g. 1024x1024 or 1920x1080. "
            "OpenAI treats size as a best-effort hint. "
            "Google maps to the closest supported aspect ratio and 1K/2K/4K tier."
        ),
    )
    quality: str = Field(
        default="auto",
        description=(
            "Image quality for OpenAI only. Options: low, medium, high, auto. "
            "Ignored by Google provider."
        ),
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
        "Returns sandbox:// URLs for the generated images and multimodal tool context. "
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
                images, model_text, raw_output = await self._generate_openai(
                    prompt, size, quality, n, ref_data,
                )
            elif provider == "google":
                images, model_text, raw_output = await self._generate_google(
                    prompt, size, n, ref_data,
                )
            else:
                return make_tool_error(
                    kind=self.name,
                    error=(
                        f"Provider '{self.provider}' does not support image generation. "
                        "Supported: openai, google"
                    ),
                )

            if not images:
                model_text = model_text.strip()
                raw_output = raw_output.strip()
                if not raw_output:
                    raw_output = model_text
                if not raw_output:
                    raw_output = "<empty provider response>"

                rendered = (
                    "No images were generated. The model may not support image generation.\n\n"
                    f"Model output:\n{raw_output}"
                )
                return make_tool_error(
                    kind=self.name,
                    error="No images were generated. The model may not support image generation",
                    text=rendered,
                    data={
                        "prompt": prompt,
                        "size": size,
                        "quality": quality,
                        "requested": n,
                        "provider": provider,
                        "model_output": model_text,
                        "raw_output": raw_output,
                    },
                    llm_content=raw_output,
                )

            text, media = self._save_images(images)
            clean_model_text = _strip_data_uri_images(model_text)
            display_text = f"{clean_model_text}\n\n{text}" if clean_model_text else text
            return make_tool_success(
                kind=self.name,
                text=display_text,
                data={
                    "prompt": prompt,
                    "size": size,
                    "quality": quality,
                    "requested": n,
                    "provider": provider,
                    "media": media,
                },
                meta={"image_count": len(media)},
                llm_content=display_text,
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
    ) -> tuple[list[tuple[bytes, str]], str, str]:
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

        async def _single_call() -> tuple[list[tuple[bytes, str]], str, str]:
            response = await client.chat.completions.create(
                model=self.model,
                messages=[{"role": "user", "content": message_content}],
            )
            content = _extract_openai_response_text(response)
            raw_output = _serialize_provider_output(response, fallback_text=content)
            images = _extract_images_from_data_uris(content)
            return images, content, raw_output

        if n <= 1:
            return await _single_call()

        results = await asyncio.gather(*[_single_call() for _ in range(n)])
        all_images: list[tuple[bytes, str]] = []
        model_texts: list[str] = []
        raw_outputs: list[str] = []
        for i, (batch_images, batch_text, batch_raw_output) in enumerate(results, start=1):
            all_images.extend(batch_images)
            cleaned = batch_text.strip()
            if cleaned:
                model_texts.append(f"[output {i}]\n{cleaned}" if n > 1 else cleaned)
            cleaned_raw = batch_raw_output.strip()
            if cleaned_raw:
                raw_outputs.append(f"[output {i}]\n{cleaned_raw}" if n > 1 else cleaned_raw)
        return all_images, "\n\n".join(model_texts), "\n\n".join(raw_outputs)

    async def _generate_google(
        self, prompt: str, size: str, n: int,
        ref_data: Optional[tuple[bytes, str]] = None,
    ) -> tuple[list[tuple[bytes, str]], str, str]:
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

        async def _single_call() -> tuple[list[tuple[bytes, str]], str, str]:
            response = await client.aio.models.generate_content(
                model=self.model,
                contents=contents,
                config=types.GenerateContentConfig(
                    response_modalities=["TEXT", "IMAGE"],
                    image_config=types.ImageConfig(
                        aspect_ratio=aspect_ratio,
                        image_size=image_size,
                    ),
                ),
            )
            images: list[tuple[bytes, str]] = []
            text_parts: list[str] = []
            candidates = response.candidates or []
            if not candidates or not candidates[0].content:
                raw_output = _serialize_provider_output(response)
                return images, "", raw_output
            parts = candidates[0].content.parts or []
            for part in parts:
                if part.inline_data:
                    images.append((part.inline_data.data, ".png"))
                text = getattr(part, "text", None)
                if isinstance(text, str) and text.strip():
                    text_parts.append(text.strip())
            model_text = "\n\n".join(text_parts)
            raw_output = _serialize_provider_output(response, fallback_text=model_text)
            return images, model_text, raw_output

        if n <= 1:
            return await _single_call()

        results = await asyncio.gather(*[_single_call() for _ in range(n)])
        all_images: list[tuple[bytes, str]] = []
        model_texts: list[str] = []
        raw_outputs: list[str] = []
        for i, (batch_images, batch_text, batch_raw_output) in enumerate(results, start=1):
            all_images.extend(batch_images)
            cleaned = batch_text.strip()
            if cleaned:
                model_texts.append(f"[output {i}]\n{cleaned}" if n > 1 else cleaned)
            cleaned_raw = batch_raw_output.strip()
            if cleaned_raw:
                raw_outputs.append(f"[output {i}]\n{cleaned_raw}" if n > 1 else cleaned_raw)
        return all_images, "\n\n".join(model_texts), "\n\n".join(raw_outputs)

    def _save_images(
        self, images: list[tuple[bytes, str]]
    ) -> tuple[str, list[dict[str, Any]]]:
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

        text = "\n\n".join(markdown)
        return text, media
