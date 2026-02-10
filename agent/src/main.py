"""Main entry point for the Claude Chat agent."""

import asyncio
import json
import os
import sys

import websockets


BACKEND_WS_URL = os.environ.get("BACKEND_WS_URL", "ws://host.docker.internal:3001/internal/ws")
CONTAINER_TOKEN = os.environ.get("CONTAINER_TOKEN", "")


async def main():
    url = f"{BACKEND_WS_URL}?token={CONTAINER_TOKEN}"
    print(f"Connecting to backend: {BACKEND_WS_URL}", flush=True)

    async with websockets.connect(url) as ws:
        # Send ready signal
        await ws.send(json.dumps({"type": "ready"}))
        print("Agent ready, waiting for messages...", flush=True)

        async for raw in ws:
            msg = json.loads(raw)
            msg_type = msg.get("type")

            if msg_type == "init":
                print(f"Initialized for conversation {msg.get('conversation_id')}", flush=True)
            elif msg_type == "user_message":
                # Echo for now - will be replaced with LangChain agent
                content = msg.get("content", "")
                await ws.send(json.dumps({
                    "type": "assistant_delta",
                    "delta": f"Echo: {content}",
                }))
                await ws.send(json.dumps({
                    "type": "complete",
                    "content": f"Echo: {content}",
                    "token_usage": {"prompt": 0, "completion": 0},
                }))
            elif msg_type == "cancel":
                print("Cancel received", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
