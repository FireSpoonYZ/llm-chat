"""Main entry point for the Claude Chat agent."""

from __future__ import annotations

import asyncio
import json
import logging
import os
import signal
import sys

import websockets

from .agent import AgentConfig, ChatAgent
from .mcp.manager import McpManager
from .tools import create_all_tools

logger = logging.getLogger("claude-chat-agent")

BACKEND_WS_URL = os.environ.get(
    "BACKEND_WS_URL", "ws://host.docker.internal:3001/internal/ws"
)
CONTAINER_TOKEN = os.environ.get("CONTAINER_TOKEN", "")
RECONNECT_DELAY = 3
MAX_RECONNECT_ATTEMPTS = 5


class AgentSession:
    """Manages the WebSocket connection and agent lifecycle."""

    def __init__(self, ws_url: str, token: str) -> None:
        self.ws_url = ws_url
        self.token = token
        self.agent: ChatAgent | None = None
        self.mcp_manager: McpManager = McpManager()
        self._current_task: asyncio.Task | None = None
        self._shutdown = False

    async def run(self) -> None:
        """Connect to backend and process messages."""
        url = f"{self.ws_url}?token={self.token}"
        logger.info("Connecting to backend: %s", self.ws_url)

        try:
            async with websockets.connect(url) as ws:
                self.ws = ws
                await ws.send(json.dumps({"type": "ready"}))
                logger.info("Agent ready, waiting for messages...")

                async for raw in ws:
                    if self._shutdown:
                        break
                    await self._handle_message(raw)
        finally:
            await self.mcp_manager.shutdown()

    async def _handle_message(self, raw: str | bytes) -> None:
        """Dispatch an incoming WebSocket message."""
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            logger.warning("Invalid JSON received: %s", raw[:200])
            return

        msg_type = msg.get("type", "")

        if msg_type == "init":
            await self._handle_init(msg)
        elif msg_type == "user_message":
            await self._handle_user_message(msg)
        elif msg_type == "cancel":
            self._handle_cancel()
        else:
            logger.warning("Unknown message type: %s", msg_type)

    async def _handle_init(self, msg: dict) -> None:
        """Initialize the agent with config from backend."""
        config = AgentConfig(msg)
        logger.info(
            "Initialized for conversation %s (provider=%s, model=%s)",
            config.conversation_id,
            config.provider,
            config.model,
        )
        tools = create_all_tools() if config.tools_enabled else []

        # Set up MCP servers and add their tools
        if config.mcp_servers:
            mcp_tools = await self.mcp_manager.setup_from_config(config.mcp_servers)
            tools.extend(mcp_tools)
            logger.info("Added %d MCP tools from %d servers",
                        len(mcp_tools), len(self.mcp_manager.connected_servers))

        self.agent = ChatAgent(config, tools=tools)

    async def _handle_user_message(self, msg: dict) -> None:
        """Process a user message through the agent."""
        content = msg.get("content", "")
        if not content:
            return

        if self.agent is None:
            await self._send_error("not_initialized", "Agent not initialized")
            return

        # Cancel any running generation
        if self._current_task and not self._current_task.done():
            self.agent.cancel()
            self._current_task.cancel()
            try:
                await self._current_task
            except asyncio.CancelledError:
                pass

        self._current_task = asyncio.create_task(
            self._run_agent(content)
        )

    async def _run_agent(self, content: str) -> None:
        """Stream agent response back through WebSocket."""
        assert self.agent is not None
        try:
            async for event in self.agent.handle_message(content):
                await self.ws.send(event.to_json())
        except websockets.ConnectionClosed:
            logger.warning("WebSocket closed during agent run")
        except asyncio.CancelledError:
            logger.info("Agent run cancelled")

    def _handle_cancel(self) -> None:
        """Cancel the current generation."""
        logger.info("Cancel received")
        if self.agent:
            self.agent.cancel()
        if self._current_task and not self._current_task.done():
            self._current_task.cancel()

    async def _send_error(self, code: str, message: str) -> None:
        """Send an error message to the backend."""
        await self.ws.send(json.dumps({
            "type": "error",
            "code": code,
            "message": message,
        }))

    def shutdown(self) -> None:
        """Signal graceful shutdown."""
        self._shutdown = True
        self._handle_cancel()
        # MCP cleanup happens in the async context


async def main() -> None:
    """Entry point with reconnection logic."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        stream=sys.stdout,
    )

    session = AgentSession(BACKEND_WS_URL, CONTAINER_TOKEN)

    loop = asyncio.get_running_loop()
    if sys.platform != "win32":
        for sig in (signal.SIGTERM, signal.SIGINT):
            loop.add_signal_handler(sig, session.shutdown)

    attempts = 0
    while attempts < MAX_RECONNECT_ATTEMPTS:
        try:
            await session.run()
            break  # Clean exit
        except websockets.ConnectionClosedError as exc:
            attempts += 1
            logger.warning(
                "Connection closed (attempt %d/%d): %s",
                attempts, MAX_RECONNECT_ATTEMPTS, exc,
            )
            if attempts < MAX_RECONNECT_ATTEMPTS:
                await asyncio.sleep(RECONNECT_DELAY)
        except ConnectionRefusedError:
            attempts += 1
            logger.warning(
                "Connection refused (attempt %d/%d)",
                attempts, MAX_RECONNECT_ATTEMPTS,
            )
            if attempts < MAX_RECONNECT_ATTEMPTS:
                await asyncio.sleep(RECONNECT_DELAY)
        except asyncio.CancelledError:
            logger.info("Shutting down")
            break

    if attempts >= MAX_RECONNECT_ATTEMPTS:
        logger.error("Max reconnection attempts reached, exiting")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
