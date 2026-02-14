# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Multi-user AI chat platform where each conversation runs in an isolated Docker container with a Python/LangChain agent. Users bring their own API keys (OpenAI, Anthropic, Google Gemini, Mistral). Supports MCP servers and built-in tools (bash, file ops, search, web fetch, code interpreter).

## Development Guidelines

1. **TDD（测试驱动开发）**: 所有新功能开发和代码修改都必须遵循 TDD 模式。新增功能前先写测试，修改代码后同步更新对应的单元测试和集成测试。确保测试覆盖核心逻辑和边界情况，提交代码前所有测试必须通过。
2. **优先使用成熟的第三方库**: 实现功能时优先选用社区广泛使用、维护活跃的开源库，避免手写已有成熟方案的逻辑。引入新依赖前确认其流行度、维护状态和许可证兼容性。

## Commands

### Backend (Rust/Axum)
```
cd backend && cargo build
cd backend && cargo test
cd backend && cargo run
```

### Agent (Python/LangChain)
```
cd agent && uv sync --dev
cd agent && uv run pytest tests/ -v          # all tests
cd agent && uv run pytest tests/test_foo.py -v  # single file
cd agent && uv run pytest tests/test_foo.py::test_bar -v  # single test
```
Note: `uv` is at `~/.local/bin/uv`

### Frontend (Vue 3/TypeScript)
```
cd frontend && pnpm install
cd frontend && pnpm dev          # Vite dev server on :8080, proxies /api to :8081
cd frontend && pnpm build        # vue-tsc --noEmit + vite build
cd frontend && pnpm test         # vitest --run
cd frontend && npx vue-tsc --noEmit  # type check only
```

### Full Stack (Docker)
```
docker compose up --build
```

## Architecture

```
Browser (Vue 3 + Element Plus + Pinia)
  │
  │ REST (/api/*) + WebSocket (/api/ws?token=jwt)
  ▼
Nginx (port 80) ── reverse proxy
  │
  │ /api/* → backend:3000    /internal/* → backend:3001
  ▼
Rust Backend (Axum) ── two ports:
  ├─ :3000  Main API (REST + client WebSocket)
  ├─ :3001  Internal WebSocket (container-facing)
  ├─ SQLite (sqlx, migrations in /migrations/)
  └─ Docker API (bollard) → container lifecycle
       │
       │ Internal WS (ws://backend:3001/internal/ws?token=container_token)
       ▼
Docker Container (Python 3.12 + LangChain)
  ├─ Connects back to backend on startup, sends "ready"
  ├─ Receives "init" (provider config, API key, model, MCP servers)
  ├─ Receives "user_message", streams back deltas/tool calls/results
  ├─ /workspace mounted from data/conversations/{conversation_id}
  └─ Resource limits: 512MB RAM, 1 CPU
```

### WebSocket Message Flow
- **Client → Backend**: `join_conversation`, `user_message`, `edit_message`, `regenerate`, `cancel`, `ping`
- **Backend → Container**: `init`, `user_message`, `truncate_history`, `cancel`
- **Container → Backend**: `ready`, `complete`, `error`, plus pass-through: `assistant_delta`, `thinking_delta`, `tool_call`, `tool_result`

### Container Lifecycle
Each conversation gets its own Docker container (image: `claude-chat-agent:latest`). Idle containers auto-stop after configurable timeout (default 600s). Graceful shutdown stops all containers.

## Database

SQLite with sqlx. Schema in `migrations/20240101000000_initial_schema.sql`.
Tables: `users`, `user_providers` (encrypted API keys), `conversations`, `messages`, `mcp_servers`, `conversation_mcp_servers`, `refresh_tokens`, `user_presets`.

## Key Directories

- `backend/src/` — Axum handlers, WebSocket logic, Docker management, auth (JWT + argon2), encryption (AES-GCM)
- `agent/src/` — LangChain agent, tools (`tools/`), prompt assembly (`prompts/`), WebSocket client
- `frontend/src/` — Vue 3 SPA, stores (Pinia), views, components
- `docker/` — Dockerfiles + nginx.conf
- `data/` — Runtime: SQLite DB + per-conversation workspace dirs (gitignored)

## Agent Internals

- Tools defined in `agent/src/tools/`: bash, file_ops (read/write/edit), search (glob/grep), web (fetch), code_interpreter
- Prompt system in `agent/src/prompts/`: modular assembly (base + tools + behaviors + mcp), with presets in `prompts/presets/`
- pytest config: `asyncio_mode = auto` (in pyproject.toml)

## Environment

Copy `.env.example` to `.env`. Key vars: `JWT_SECRET`, `ENCRYPTION_KEY`, `DATABASE_URL`, `HOST`, `PORT`, `INTERNAL_WS_PORT`, `CONTAINER_IMAGE`, `CONTAINER_IDLE_TIMEOUT`, `FILESERVER_URL`.
