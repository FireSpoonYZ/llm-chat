# Fix: 切换 Provider 后 ImageGenerationTool 仍使用旧 Provider

## Context

用户在聊天窗口通过 cascader 从 anthropic 切换到 openai provider 后，image_generation tool 仍然报错 `Provider 'anthropic' does not support image generation`。

**根因**: 容器只在首次连接时通过 `init` 消息初始化一次（`container.rs:95-184`）。之后在 UI 切换 provider/model 只更新了数据库（REST API `PUT /api/conversations/{id}`），但已运行的容器内的 agent 仍持有旧的 provider 配置。`ImageGenerationTool.provider` 在 init 时设置后不会再变。

## 修复方案

当 `update_conversation` 检测到 `provider` 或 `model_name` 发生变化时，停止当前运行的容器。下次发消息时容器会自动重启，并用新的 provider/model 重新初始化。

### 修改文件

1. **`backend/src/api/conversations.rs`** — `update_conversation` handler
   - 比较 `existing.provider` / `existing.model_name` 与新值
   - 如果 provider 或 model 变了，调用 `docker_manager.stop_container(&id)` 停止容器
   - 同时调用 `ws_state.remove_container(&id)` 清理 WS 状态

2. **`backend/tests/` 或现有测试** — 补充测试
   - 验证 provider 变更时容器被停止的逻辑

### 关键代码路径

- `backend/src/api/conversations.rs:108-147` — update handler，需要在此添加容器停止逻辑
- `backend/src/docker/manager.rs:175` — `stop_container` 方法已存在
- `backend/src/ws/mod.rs` — `WsState::remove_container` 已存在
- `backend/src/auth/middleware.rs:12-17` — `AppState` 已包含 `ws_state` 和 `docker_manager`

## 验证

1. `cd backend && cargo test` — 确保编译通过、现有测试不受影响
2. 手动测试：创建对话 → 用 anthropic provider 发消息 → 在 toolbar 切换到 openai → 发消息使用 image_generation tool → 应该正常工作
