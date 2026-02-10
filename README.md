# Claude Chat

A multi-user AI chat platform where each conversation runs in an isolated Docker container with a Python/LangChain agent. Users bring their own API keys and can use multiple AI providers. The system supports MCP (Model Context Protocol) servers and a rich tool system.

## Architecture

```
Browser (Vue 3 + Element Plus)
    |
    | WebSocket (streaming) + REST (auth, CRUD)
    v
Rust Backend (Axum)
    |
    |-- SQLite (users, conversations, messages, settings)
    |-- Docker API (bollard) -> container lifecycle
    |
    | WebSocket (internal, per-container)
    v
Docker Container (Python 3.12 + LangChain)
    |
    |-- LLM Provider APIs (user's keys)
    |-- MCP Servers (stdio transport)
    |-- Built-in Tools (bash, file ops, web, code interpreter)
    +-- /workspace (mounted conversation directory)
```

## Features

- **Multi-provider AI**: OpenAI, Anthropic, Google Gemini, Mistral — users configure their own API keys
- **Isolated execution**: Each conversation runs in its own Docker container with resource limits
- **Rich tool system**: Bash, file read/write/edit, glob, grep, web fetch, code interpreter
- **MCP support**: Admin-defined MCP servers, user-selectable per conversation
- **Streaming responses**: Real-time token streaming via WebSocket
- **Auto-cleanup**: Idle containers are automatically stopped after a configurable timeout
- **JWT authentication**: Access tokens + refresh token rotation
- **Encrypted storage**: API keys encrypted with AES-256-GCM at rest

## Project Structure

```
claude-chat/
├── backend/                  # Rust (Axum) — REST API + WebSocket server
│   └── src/
│       ├── main.rs           # Server setup, routes, AppState
│       ├── config.rs         # Environment-based configuration
│       ├── crypto.rs         # AES-256-GCM encryption
│       ├── api/              # REST endpoint handlers
│       ├── auth/             # JWT, middleware, password hashing
│       ├── db/               # SQLite data access layer
│       ├── docker/           # Container management (bollard)
│       └── ws/               # WebSocket handlers (client + container)
├── agent/                    # Python (LangChain) — AI agent
│   └── src/
│       ├── main.py           # WebSocket client + agent lifecycle
│       ├── agent.py          # LangChain agent with streaming
│       ├── providers.py      # Multi-provider LLM factory
│       ├── tools/            # Built-in tools (bash, files, search, web, code)
│       ├── mcp/              # MCP client + server manager
│       └── prompts/          # Modular system prompt composition
├── frontend/                 # Vue 3 + TypeScript + Element Plus
│   └── src/
│       ├── views/            # Login, Register, Chat, Settings
│       ├── components/       # ChatMessage, ChatInput, ToolCallDisplay, etc.
│       ├── stores/           # Pinia stores (auth, chat, settings)
│       └── api/              # Axios client + WebSocket manager
├── docker/                   # Dockerfiles + docker-compose.yml
├── migrations/               # SQLite schema migrations
└── data/                     # Runtime data (gitignored)
```

## Prerequisites

- **Rust** (1.82+) with cargo
- **Python** (3.12+) with [uv](https://docs.astral.sh/uv/)
- **Node.js** (20+) with [pnpm](https://pnpm.io/)
- **Docker** with Docker Compose

## Quick Start

### 1. Clone and configure

```bash
cp .env.example .env
# Edit .env — set JWT_SECRET and ENCRYPTION_KEY to secure random values
```

### 2. Run with Docker Compose

```bash
cd docker
docker compose up --build
```

This builds all three services:
- **backend** on port 3000 (API) and 3001 (internal WS)
- **frontend** on port 80
- **agent image** built as `claude-chat-agent:latest`

Open http://localhost in your browser.

### 3. First-time setup

1. Register a user account
2. Go to Settings and configure at least one AI provider (e.g., add your OpenAI API key)
3. Create a new conversation and start chatting

## Development

### Backend

```bash
cd backend
cp ../.env.example ../.env  # if not done already
cargo run
```

The backend runs on `http://localhost:3000` (API) and `http://localhost:3001` (internal WS).

### Frontend

```bash
cd frontend
pnpm install
pnpm dev
```

The dev server runs on `http://localhost:5173` with hot reload.

### Agent

```bash
cd agent
uv sync
uv run python -m src.main
```

The agent connects to the backend's internal WebSocket endpoint (normally started by the backend inside a Docker container).

## Testing

### Backend (54 tests)

```bash
cd backend
cargo test
```

| Module | Tests | Coverage |
|--------|-------|----------|
| `db::users` | 5 | CRUD + lookup by id/username/email |
| `db::conversations` | 6 | CRUD + list + cross-user isolation |
| `db::messages` | 3 | Create + pagination + count |
| `db::providers` | 6 | Upsert + default provider + CRUD |
| `db::mcp_servers` | 6 | CRUD + enabled filter + conversation associations |
| `db::refresh_tokens` | 5 | CRUD + delete by hash + user cleanup |
| `auth` | 4 | JWT round-trip + admin flag + wrong secret |
| `auth::password` | 3 | Hash/verify + wrong password + salt uniqueness |
| `crypto` | 3 | AES-GCM round-trip + wrong key + invalid key length |
| `docker::registry` | 6 | Register/unregister + idle detection + touch |
| `ws` | 8 | Client/container add/remove + send + multi-client |

### Agent (146 tests)

```bash
cd agent
uv run pytest
```

| Module | Tests | Coverage |
|--------|-------|----------|
| `test_agent` | 27 | Config parsing, message history, streaming, tool execution |
| `test_main` | 13 | Session lifecycle, message dispatch, cancellation |
| `test_providers` | 18 | All 4 providers, custom endpoints, temperature, streaming |
| `test_tools` | 47 | All 8 tools: bash, read, write, edit, glob, grep, web, code |
| `test_mcp` | 26 | MCP client, LangChain bridge, manager, config parsing |
| `test_prompts` | 15 | Base prompt, tool descriptions, MCP instructions, assembly |

### Frontend

```bash
cd frontend
npx vue-tsc --noEmit   # Type checking
```

## API Reference

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/auth/register` | Register new user |
| POST | `/api/auth/login` | Login (returns JWT + refresh token) |
| POST | `/api/auth/refresh` | Refresh access token |
| POST | `/api/auth/logout` | Invalidate refresh token |

### Users

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/users/me` | Get current user profile |
| GET | `/api/users/me/providers` | List configured providers |
| POST | `/api/users/me/providers` | Add/update a provider |
| DELETE | `/api/users/me/providers/:provider` | Remove a provider |

### Conversations

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/conversations` | List conversations |
| POST | `/api/conversations` | Create conversation |
| GET | `/api/conversations/:id` | Get conversation |
| PUT | `/api/conversations/:id` | Update conversation |
| DELETE | `/api/conversations/:id` | Delete conversation |
| GET | `/api/conversations/:id/messages` | Get messages (paginated) |
| GET | `/api/conversations/:id/mcp-servers` | Get enabled MCP servers |
| PUT | `/api/conversations/:id/mcp-servers` | Set enabled MCP servers |

### Admin

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/admin/mcp-servers` | List all MCP servers |
| POST | `/api/admin/mcp-servers` | Create MCP server |
| PUT | `/api/admin/mcp-servers/:id` | Update MCP server |
| DELETE | `/api/admin/mcp-servers/:id` | Delete MCP server |
| GET | `/api/admin/containers` | List running containers |

### WebSocket

Connect to `ws://host/api/ws?token={jwt}` for real-time chat streaming.

## Built-in Tools

The AI agent has access to these tools inside each conversation container:

| Tool | Description |
|------|-------------|
| **Bash** | Execute shell commands with configurable timeout |
| **Read** | Read file contents with line numbers, offset, and limit |
| **Write** | Create or overwrite files (auto-creates directories) |
| **Edit** | Find-and-replace editing with occurrence validation |
| **Glob** | Find files by glob pattern |
| **Grep** | Search file contents with regex, context lines, glob filter |
| **WebFetch** | Fetch URL content with HTML-to-text conversion |
| **CodeInterpreter** | Execute Python or JavaScript code |

MCP tools are dynamically registered from enabled MCP servers.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `JWT_SECRET` | Secret for signing JWT tokens | (required) |
| `ENCRYPTION_KEY` | 32-byte hex key for AES-256-GCM | (required) |
| `DATABASE_URL` | SQLite connection string | `sqlite:data/claude-chat.db?mode=rwc` |
| `HOST` | Backend bind address | `0.0.0.0` |
| `PORT` | Backend API port | `3000` |
| `INTERNAL_WS_PORT` | Internal WebSocket port for containers | `3001` |
| `CONTAINER_IMAGE` | Docker image for agent containers | `claude-chat-agent:latest` |
| `CONTAINER_IDLE_TIMEOUT` | Seconds before idle containers are stopped | `600` |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Backend | Rust, Axum 0.8, SQLite (sqlx), bollard |
| Agent | Python 3.12, LangChain 0.3, websockets, MCP SDK |
| Frontend | Vue 3, TypeScript, Element Plus, Pinia, Vite |
| Auth | JWT (jsonwebtoken), Argon2, AES-256-GCM |
| Deployment | Docker Compose |
