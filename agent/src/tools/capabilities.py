from __future__ import annotations

import json
from typing import Any

from langchain_core.tools import BaseTool

READ_ONLY_BUILTINS = {"read", "list", "glob", "grep", "web_fetch", "web_search"}


def _as_bool(value: Any) -> bool | None:
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered in {"1", "true", "yes", "y"}:
            return True
        if lowered in {"0", "false", "no", "n"}:
            return False
    return None


def _tool_metadata(tool: BaseTool) -> dict[str, Any]:
    meta = getattr(tool, "metadata", None)
    if isinstance(meta, dict):
        return dict(meta)
    return {}


def set_tool_capabilities(
    tool: BaseTool,
    *,
    source: str,
    read_only: bool,
    mcp_server: str | None = None,
) -> None:
    meta = _tool_metadata(tool)
    meta["tool_source"] = source
    meta["read_only"] = bool(read_only)
    if mcp_server:
        meta["mcp_server"] = mcp_server
    tool.metadata = meta


def tool_is_read_only(tool: BaseTool) -> bool:
    meta = _tool_metadata(tool)
    val = _as_bool(meta.get("read_only"))
    return bool(val) if val is not None else False


def annotate_builtin_tools(tools: list[BaseTool]) -> None:
    for tool in tools:
        set_tool_capabilities(
            tool,
            source="builtin",
            read_only=tool.name in READ_ONLY_BUILTINS,
        )


def parse_mcp_read_only_overrides(
    mcp_servers: list[dict[str, Any]],
) -> dict[str, dict[str, bool]]:
    parsed: dict[str, dict[str, bool]] = {}
    for server in mcp_servers:
        name = str(server.get("name", "")).strip()
        if not name:
            continue
        raw = server.get("read_only_overrides")
        value: Any = raw
        if isinstance(raw, str):
            try:
                value = json.loads(raw)
            except json.JSONDecodeError:
                continue
        if not isinstance(value, dict):
            continue
        out: dict[str, bool] = {}
        for k, v in value.items():
            if not isinstance(k, str):
                continue
            b = _as_bool(v)
            if b is None:
                continue
            out[k] = b
        parsed[name] = out
    return parsed


def _extract_read_only_from_metadata(tool: BaseTool) -> bool | None:
    meta = _tool_metadata(tool)
    for key in ("read_only", "readOnly", "readonly", "readOnlyHint"):
        val = _as_bool(meta.get(key))
        if val is not None:
            return val
    annotations = meta.get("annotations")
    if isinstance(annotations, dict):
        for key in ("readOnlyHint", "read_only", "readOnly"):
            val = _as_bool(annotations.get(key))
            if val is not None:
                return val
    return None


def _extract_mcp_server_name(
    tool: BaseTool,
    known_servers: set[str],
) -> str | None:
    meta = _tool_metadata(tool)
    for key in ("mcp_server", "server_name", "server", "mcpServer"):
        value = meta.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()

    annotations = meta.get("annotations")
    if isinstance(annotations, dict):
        for key in ("mcp_server", "server_name", "server", "mcpServer"):
            value = annotations.get(key)
            if isinstance(value, str) and value.strip():
                return value.strip()

    mcp_meta = meta.get("mcp")
    if isinstance(mcp_meta, dict):
        for key in ("server", "name", "server_name", "mcp_server"):
            value = mcp_meta.get(key)
            if isinstance(value, str) and value.strip():
                return value.strip()

    tool_name = str(getattr(tool, "name", "")).strip()
    if not tool_name:
        return None

    for server in known_servers:
        if (
            tool_name.startswith(f"{server}.")
            or tool_name.startswith(f"{server}__")
            or tool_name.startswith(f"{server}:")
            or tool_name.startswith(f"{server}/")
        ):
            return server
    return None


def _tool_name_candidates(tool_name: str, server_name: str | None) -> list[str]:
    candidates = [tool_name]
    if server_name:
        prefixes = [
            f"{server_name}.",
            f"{server_name}__",
            f"{server_name}:",
            f"{server_name}/",
        ]
        for prefix in prefixes:
            if tool_name.startswith(prefix):
                short = tool_name[len(prefix):]
                if short:
                    candidates.append(short)
    return candidates


def _unique_preserve_order(items: list[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for item in items:
        if item not in seen:
            seen.add(item)
            out.append(item)
    return out


def _build_global_unique_override_index(
    overrides: dict[str, dict[str, bool]],
) -> dict[str, bool]:
    counts: dict[str, int] = {}
    values: dict[str, bool] = {}
    for server_map in overrides.values():
        for key, value in server_map.items():
            counts[key] = counts.get(key, 0) + 1
            values[key] = value
    return {k: values[k] for k, count in counts.items() if count == 1}


def annotate_mcp_tools(
    tools: list[BaseTool],
    *,
    mcp_servers: list[dict[str, Any]],
) -> None:
    overrides = parse_mcp_read_only_overrides(mcp_servers)
    global_unique_overrides = _build_global_unique_override_index(overrides)
    known_servers = {name for name in overrides} | {
        str(s.get("name", "")).strip() for s in mcp_servers if s.get("name")
    }

    for tool in tools:
        tool_name = str(getattr(tool, "name", "")).strip()
        server_name = _extract_mcp_server_name(tool, known_servers)

        read_only = _extract_read_only_from_metadata(tool)
        applied_override = False

        if server_name and server_name in overrides:
            server_map = overrides[server_name]
            for candidate in _tool_name_candidates(tool_name, server_name):
                if candidate in server_map:
                    read_only = server_map[candidate]
                    applied_override = True
                    break

        if not applied_override and global_unique_overrides:
            fallback_candidates = _tool_name_candidates(tool_name, server_name)
            for known_server in known_servers:
                fallback_candidates.extend(_tool_name_candidates(tool_name, known_server))
            for candidate in _unique_preserve_order(fallback_candidates):
                if candidate in global_unique_overrides:
                    read_only = global_unique_overrides[candidate]
                    break

        set_tool_capabilities(
            tool,
            source="mcp",
            read_only=bool(read_only) if read_only is not None else False,
            mcp_server=server_name,
        )
