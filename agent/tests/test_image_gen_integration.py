"""Integration tests for image generation — requires real API key.

Run:
    cd agent && PATH="$HOME/.local/bin:$PATH" uv run pytest tests/test_image_gen_integration.py -v -s
"""

from __future__ import annotations

import json

import httpx
import pytest

BASE_URL = "https://www.right.codes/gemini/v1"
API_KEY = "sk-7e9cdbb194a84369acdeae9373ed6227"
PROMPT = "A red circle on white background"

# Models with "image" in the name from the /models list
IMAGE_MODELS = [
    "gemini-3-pro-image-preview",
    "gemini-2.5-flash-image",
    "gemini-3-pro-preview",
]


@pytest.mark.asyncio
async def test_chat_completions_image():
    """Test image gen via chat/completions with image-capable models."""
    headers = {"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"}

    async with httpx.AsyncClient(timeout=120) as http:
        for model in IMAGE_MODELS:
            print(f"\n--- {model} via /chat/completions ---")
            r = await http.post(
                f"{BASE_URL}/chat/completions",
                headers=headers,
                json={
                    "model": model,
                    "messages": [{"role": "user", "content": f"Generate an image: {PROMPT}"}],
                    "max_tokens": 4096,
                },
            )
            print(f"  status: {r.status_code}")
            if r.status_code == 200:
                try:
                    data = r.json()
                    content = data["choices"][0]["message"].get("content", "")
                    print(f"  content length: {len(content)}")
                    print(f"  content preview: {content[:300]}")
                    # Check if there are any non-text parts
                    msg = data["choices"][0]["message"]
                    if "parts" in msg:
                        print(f"  parts: {json.dumps(msg['parts'])[:300]}")
                except Exception as e:
                    print(f"  parse error: {e}")
                    print(f"  raw: {r.text[:300]}")
            else:
                print(f"  body: {r.text[:300]}")


@pytest.mark.asyncio
async def test_images_generations():
    """Test image gen via /images/generations with image-capable models."""
    headers = {"Authorization": f"Bearer {API_KEY}", "Content-Type": "application/json"}

    async with httpx.AsyncClient(timeout=120) as http:
        for model in IMAGE_MODELS:
            print(f"\n--- {model} via /images/generations ---")
            r = await http.post(
                f"{BASE_URL}/images/generations",
                headers=headers,
                json={
                    "model": model,
                    "prompt": PROMPT,
                    "n": 1,
                    "size": "1024x1024",
                    "response_format": "b64_json",
                },
            )
            print(f"  status: {r.status_code}")
            if r.status_code == 200:
                try:
                    data = r.json()
                    if "data" in data and data["data"]:
                        item = data["data"][0]
                        if "b64_json" in item and item["b64_json"]:
                            print(f"  b64_json length: {len(item['b64_json'])}")
                        elif "url" in item and item["url"]:
                            print(f"  url: {item['url'][:200]}")
                        else:
                            print(f"  data[0] keys: {list(item.keys())}")
                    else:
                        print(f"  response: {json.dumps(data)[:300]}")
                except Exception as e:
                    print(f"  parse error: {e}")
                    print(f"  raw: {r.text[:300]}")
            else:
                print(f"  body: {r.text[:300]}")
