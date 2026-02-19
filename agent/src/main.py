"""Main entry point for the Claude Chat agent."""

from __future__ import annotations

import asyncio
import base64
import json
import logging
import os
import signal
import sys

import websockets
from tenacity import (
    retry,
    retry_if_exception_type,
    stop_after_attempt,
    wait_exponential,
)

from .agent import AgentConfig, ChatAgent, _build_multimodal_content
from .mcp.manager import McpManager
from .prompts.assembler import assemble_system_prompt
from .subagents import SubagentRunner
from .tools import create_all_tools, create_explore_tool
from .tools.capabilities import annotate_mcp_tools

logger = logging.getLogger("claude-chat-agent")

BACKEND_WS_URL = os.environ.get(
    "BACKEND_WS_URL", "ws://host.docker.internal:3001/internal/ws"
)
CONTAINER_TOKEN = os.environ.get("CONTAINER_TOKEN", "")
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
        elif msg_type == "question_answer":
            await self._handle_question_answer(msg)
        elif msg_type == "truncate_history":
            self._handle_truncate_history(msg)
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
        tools = create_all_tools(
            provider=config.provider,
            api_key=config.api_key,
            endpoint_url=config.endpoint_url,
            model=config.model,
            image_provider=config.image_provider,
            image_model=config.image_model,
            image_api_key=config.image_api_key,
            image_endpoint_url=config.image_endpoint_url,
        ) if config.tools_enabled else []

        # Set up MCP servers and add their tools
        if config.mcp_servers:
            mcp_tools = await self.mcp_manager.setup_from_config(config.mcp_servers)
            annotate_mcp_tools(mcp_tools, mcp_servers=config.mcp_servers)
            tools.extend(mcp_tools)
            logger.info("Added %d MCP tools from %d servers",
                        len(mcp_tools), len(self.mcp_manager.connected_servers))

        # Register explore delegation tool after all other tools are available.
        if tools:
            subagent_runner = SubagentRunner(
                parent_config=config,
                base_tools=tools,
                mcp_servers=config.mcp_servers or None,
            )
            tools.append(create_explore_tool(subagent_runner))

        # Assemble final system prompt with tool descriptions
        if tools:
            tool_names = [t.name for t in tools]
            config.system_prompt = assemble_system_prompt(
                tool_names,
                mcp_servers=config.mcp_servers or None,
                base_prompt=config.system_prompt,
            )

        self.agent = ChatAgent(config, tools=tools)

    async def _handle_user_message(self, msg: dict) -> None:
        """Process a user message through the agent."""
        content = msg.get("content", "")
        if not content:
            return

        if self.agent is None:
            await self._send_error("not_initialized", "Agent not initialized")
            return

        deep_thinking = msg.get("deep_thinking", False)
        thinking_budget = msg.get("thinking_budget")  # None = use default
        subagent_thinking_budget = msg.get("subagent_thinking_budget")

        # Build multimodal content if attachments are present
        attachments = msg.get("attachments", [])
        processed_attachments = []
        for att in attachments:
            path = att.get("path", "")
            full_path = os.path.join("/workspace", path.lstrip("/"))
            if os.path.isfile(full_path):
                with open(full_path, "rb") as f:
                    data = base64.b64encode(f.read()).decode()
                processed_attachments.append({"path": path, "data": data})
        final_content = _build_multimodal_content(content, processed_attachments)

        # Cancel any running generation
        if self._current_task and not self._current_task.done():
            self.agent.cancel()
            self._current_task.cancel()
            try:
                await self._current_task
            except asyncio.CancelledError:
                pass

        self._current_task = asyncio.create_task(
            self._run_agent(
                final_content,
                deep_thinking,
                thinking_budget,
                subagent_thinking_budget,
            )
        )

    async def _run_agent(
        self,
        content: str | list,
        deep_thinking: bool = False,
        thinking_budget: int | None = None,
        subagent_thinking_budget: int | None = None,
    ) -> None:
        """Stream agent response back through WebSocket."""
        if self.agent is None:
            raise RuntimeError("Agent not initialized before _run_agent")
        self.agent.config.deep_thinking = bool(deep_thinking)
        if thinking_budget is not None:
            self.agent.config.thinking_budget = thinking_budget
        if subagent_thinking_budget is not None:
            self.agent.config.subagent_thinking_budget = subagent_thinking_budget
        try:
            async for event in self.agent.handle_message(content, deep_thinking, thinking_budget):
                await self.ws.send(event.to_json())
        except websockets.ConnectionClosed:
            logger.warning("WebSocket closed during agent run")
        except asyncio.CancelledError:
            logger.info("Agent run cancelled")

    async def _handle_question_answer(self, msg: dict[str, object]) -> None:
        """Deliver questionnaire answers to pending question tools."""
        if self.agent is None:
            await self._send_error("not_initialized", "Agent not initialized")
            return

        questionnaire_id = str(msg.get("questionnaire_id") or "").strip()
        if not questionnaire_id:
            await self._send_error("invalid_question_answer", "Missing questionnaire_id")
            return

        raw_answers = msg.get("answers")
        if not isinstance(raw_answers, list):
            await self._send_error("invalid_question_answer", "answers must be a list")
            return

        answers: list[dict[str, object]] = [
            item for item in raw_answers if isinstance(item, dict)
        ]
        accepted = self.agent.submit_question_answer(questionnaire_id, answers)
        if not accepted:
            await self._send_error(
                "question_not_pending",
                f"No pending questionnaire found for id '{questionnaire_id}'",
            )

    def _handle_truncate_history(self, msg: dict) -> None:
        """Truncate the agent's in-memory history for regenerate/edit."""
        if self.agent is None:
            logger.warning("truncate_history received before init")
            return
        keep_turns = msg.get("keep_turns", 0)
        old_len = len(self.agent.messages)
        self.agent.truncate_history(keep_turns)
        logger.info(
            "Truncated history: keep_turns=%d, messages %d -> %d",
            keep_turns, old_len, len(self.agent.messages),
        )

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

    @retry(
        retry=retry_if_exception_type(
            (websockets.ConnectionClosedError, ConnectionRefusedError)
        ),
        stop=stop_after_attempt(MAX_RECONNECT_ATTEMPTS),
        wait=wait_exponential(multiplier=1, min=1, max=30),
        reraise=True,
    )
    async def _connect_with_retry() -> None:
        await session.run()

    try:
        await _connect_with_retry()
    except (websockets.ConnectionClosedError, ConnectionRefusedError):
        logger.error("Max reconnection attempts reached, exiting")
        sys.exit(1)
    except asyncio.CancelledError:
        logger.info("Shutting down")


if __name__ == "__main__":
    asyncio.run(main())
