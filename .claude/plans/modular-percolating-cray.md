# Fix: Deep Thinking 流式输出 + cold-start 参数传递

## Context

用户开启 deep thinking 后发现两个问题：
1. thinking 内容很简短 — 因为 `deep_thinking` 标志在冷启动路径丢失，agent 从未收到该参数，没有启用 Anthropic extended thinking API
2. thinking 内容一次性显示 — 前端 `<details>` 默认折叠，流式内容在折叠状态下累积，展开时已全部到位

正文消息的流式输出已确认是真正的逐 token 流式（WebSocket → Vue reactive mutation → DOM re-render），无伪装。

## 已完成的修复（需验证）

### Backend: cold-start `deep_thinking` 传递

`pending_messages` 机制已实现，代码已就位：

- `backend/src/ws/mod.rs` — `WsState` 新增 `pending_messages: RwLock<HashMap<String, String>>`，含 `set_pending_message()` / `take_pending_message()`
- `backend/src/ws/client.rs` — `send_to_container` 失败时，将完整消息（含 `deep_thinking`）存入 pending
- `backend/src/ws/container.rs` — 容器 "ready" 时优先取 pending message，保留所有字段；fallback 才从 DB 重建

### Backend: `thinking_delta` 转发

`container.rs:197` 的 match arm 已包含 `"thinking_delta"`。

## 待实施的变更

### Step 1: 前端 — thinking 块流式展开

`frontend/src/components/ChatMessage.vue` line 26

```html
<!-- 改前 -->
<details>

<!-- 改后 -->
<details :open="!!streamingBlocks">
```

原理：`streamingBlocks` prop 仅传给正在流式输出的 ChatMessage 实例（`Chat.vue:69`），历史消息不传此 prop（值为 `undefined`）。所以：
- 流式消息：`!!streamingBlocks` = `true` → 展开，thinking 内容实时可见
- 历史消息：`!!undefined` = `false` → 折叠
- 流式结束后：streaming ChatMessage 从 DOM 移除，完成的消息作为历史消息渲染 → 折叠

### Step 2: 清理调试日志

`backend/src/ws/container.rs` — 移除 3 行调试日志：
- line ~89: `tracing::debug!("Container msg for ...")`
- line ~198: `tracing::debug!("Forwarding ...")`
- line ~262: `tracing::debug!("Unhandled container msg type: ...")`

`backend/src/ws/client.rs` — 移除 1 行：
- `tracing::debug!("user_message: deep_thinking=...")`

### Step 3: 清理 agent 调试代码

`agent/src/agent.py` — 如果还有 `import logging` / `logger` / debug 日志行，移除。
`agent/src/main.py` — 如果 log level 被改为 DEBUG，恢复为 INFO。

（注：用户可能已手动恢复这些文件，需先检查当前状态）

### Step 4: 重建 & 重启

1. `cd backend && cargo build` — 重建后端
2. 重启后端进程
3. `cd agent && docker build ...` — 仅在 agent 代码有变更时重建镜像

## 验证

1. `cd backend && cargo test` — 编译通过，83 个测试无回归
2. 在前端新建对话（触发冷启动路径），开启 deep thinking，发送消息
3. 确认：thinking 块在流式输出时展开显示，内容逐步增长
4. 确认：流式结束后 thinking 块自动折叠
5. 确认：正文内容仍然逐 token 流式输出
6. 查看 agent 容器日志，确认 API 请求包含 `thinking: {"type": "enabled", "budget_tokens": 10000}`
