from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock

import pytest

from src.agent import AgentConfig, StreamEvent
from src.subagents import EXPLORE_SUBAGENT_PROMPT, SubagentRunner
from src.tools.explore import ExploreTool


class _FakeTool:
    def __init__(self, name: str, metadata: dict | None = None) -> None:
        self.name = name
        self.metadata = metadata or {}


def _config(**overrides) -> AgentConfig:
    base = {
        "conversation_id": "conv-test",
        "provider": "openai",
        "model": "gpt-4o",
        "api_key": "test-key",
    }
    base.update(overrides)
    return AgentConfig(base)


@pytest.mark.asyncio
async def test_explore_tool_requires_runner() -> None:
    tool = ExploreTool(runner=None)
    result = await tool._arun("x", "y")
    assert result["success"] is False
    assert "not configured" in result["error"]


@pytest.mark.asyncio
async def test_explore_tool_delegates_to_runner() -> None:
    runner = AsyncMock()
    runner.run_subagent.return_value = {"kind": "explore", "text": "ok", "success": True, "error": None, "data": {}, "meta": {}}
    tool = ExploreTool(runner=runner)
    result = await tool._arun("desc", "prompt")
    assert result["success"] is True
    runner.run_subagent.assert_awaited_once_with(
        subagent_type="explore",
        result_kind="explore",
        description="desc",
        prompt="prompt",
        event_sink=None,
    )


@pytest.mark.asyncio
async def test_explore_tool_forwards_event_sink_to_runner() -> None:
    runner = AsyncMock()
    runner.run_subagent.return_value = {"kind": "explore", "text": "ok", "success": True, "error": None, "data": {}, "meta": {}}
    tool = ExploreTool(runner=runner)
    sink = AsyncMock()
    tool.set_event_sink(sink)

    result = await tool._arun("desc", "prompt")
    assert result["success"] is True
    runner.run_subagent.assert_awaited_once_with(
        subagent_type="explore",
        result_kind="explore",
        description="desc",
        prompt="prompt",
        event_sink=sink,
    )


@pytest.mark.asyncio
async def test_explore_tool_falls_back_to_run_task_runner() -> None:
    class _LegacyRunner:
        async def run_task(self, **kwargs: Any) -> dict[str, Any]:
            assert kwargs["subagent_type"] == "explore"
            assert kwargs["description"] == "desc"
            assert kwargs["prompt"] == "prompt"
            assert kwargs["event_sink"] is None
            return {
                "kind": "task",
                "text": "ok",
                "success": True,
                "error": None,
                "data": {},
                "meta": {},
            }

    tool = ExploreTool(runner=_LegacyRunner())
    result = await tool._arun("desc", "prompt")
    assert result["success"] is True
    assert result["kind"] == "task"


@pytest.mark.asyncio
async def test_subagent_runner_rejects_unsupported_type() -> None:
    runner = SubagentRunner(
        parent_config=_config(subagent_provider="openai", subagent_model="gpt-4o"),
        base_tools=[],
    )
    result = await runner.run_subagent(subagent_type="plan", description="x", prompt="y")
    assert result["success"] is False
    assert "only subagent_type='explore'" in result["text"]


@pytest.mark.asyncio
async def test_subagent_runner_rejects_unsupported_type_with_result_kind() -> None:
    runner = SubagentRunner(
        parent_config=_config(subagent_provider="openai", subagent_model="gpt-4o"),
        base_tools=[],
    )
    result = await runner.run_subagent(
        subagent_type="plan",
        result_kind="explore",
        description="x",
        prompt="y",
    )
    assert result["success"] is False
    assert result["kind"] == "explore"


@pytest.mark.asyncio
async def test_subagent_runner_requires_configured_model() -> None:
    runner = SubagentRunner(
        parent_config=_config(provider="", model="", subagent_provider="", subagent_model=""),
        base_tools=[],
    )
    result = await runner.run_subagent(subagent_type="explore", description="x", prompt="y")
    assert result["success"] is False
    assert "not configured for this conversation" in result["text"]


@pytest.mark.asyncio
async def test_subagent_runner_rejects_nested_invocation() -> None:
    runner = SubagentRunner(
        parent_config=_config(subagent_provider="openai", subagent_model="gpt-4o"),
        base_tools=[],
    )
    runner._depth = 1
    result = await runner.run_subagent(subagent_type="explore", description="x", prompt="y")
    assert result["success"] is False
    assert "cannot invoke subagents" in result["text"]


@pytest.mark.asyncio
async def test_subagent_runner_returns_trace_blocks(monkeypatch: pytest.MonkeyPatch) -> None:
    class _FakeChatAgent:
        def __init__(self, _config: AgentConfig, tools: list[_FakeTool]) -> None:
            self.tools = tools

        async def handle_message(
            self,
            _prompt: str,
            deep_thinking: bool = False,
            thinking_budget: int | None = None,
        ):
            yield StreamEvent("assistant_delta", {"delta": "Investigating"})
            yield StreamEvent(
                "tool_call",
                {
                    "tool_call_id": "tc-1",
                    "tool_name": "read",
                    "tool_input": {"file_path": "README.md"},
                },
            )
            yield StreamEvent(
                "tool_result",
                {"tool_call_id": "tc-1", "result": "ok", "is_error": False},
            )
            yield StreamEvent("complete", {"content": "Done"})

    monkeypatch.setattr("src.subagents.ChatAgent", _FakeChatAgent)

    runner = SubagentRunner(
        parent_config=_config(subagent_provider="openai", subagent_model="gpt-4o"),
        base_tools=[
            _FakeTool("read", {"read_only": True}),
            _FakeTool("explore", {"read_only": False}),
        ],
    )
    sink = AsyncMock()
    result = await runner.run_subagent(
        subagent_type="explore",
        description="Inspect docs",
        prompt="Find architecture notes",
        event_sink=sink,
    )
    assert result["success"] is True
    assert result["kind"] == "explore"
    trace = result["data"]["trace"]
    assert isinstance(trace, list)
    assert len(trace) == 2
    assert trace[0]["type"] == "text"
    assert trace[1]["type"] == "tool_call"
    assert trace[1]["result"] == "ok"
    assert sink.await_count == 4
    emitted_types = [call.args[0].type for call in sink.await_args_list]
    assert emitted_types == ["assistant_delta", "tool_call", "tool_result", "complete"]


def test_agent_config_falls_back_to_main_model_for_subagent() -> None:
    cfg = _config()
    assert cfg.subagent_provider == "openai"
    assert cfg.subagent_model == "gpt-4o"
    assert cfg.subagent_api_key == "test-key"
    assert cfg.subagent_thinking_budget is None


def test_explore_prompt_is_read_only_and_no_bash_requirement() -> None:
    prompt = EXPLORE_SUBAGENT_PROMPT.lower()
    assert "read-only mode" in prompt
    assert "strictly prohibited" in prompt
    assert "glob" in prompt
    assert "grep" in prompt
    assert "read" in prompt
    assert "parallel tool calls" in prompt
    assert "bash" not in prompt
