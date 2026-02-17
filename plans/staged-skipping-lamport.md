# 对话默认 thinking_budget 设置为 128K

## Context

当前新建对话的 `thinking_budget` 在数据库中为 `NULL`，前端显示 "default" 占位符，agent 端在收到 `None` 时 fallback 到 128000。用户希望新建对话时显式默认为 128000（128K tokens），使 UI 直接展示该值。

## 修改方案

只需修改后端创建对话时的默认值，一行改动：

### `backend/src/api/conversations.rs` (line 93)

```rust
// before
req.thinking_budget,

// after
req.thinking_budget.or(Some(128000)),
```

这样新建的对话会在数据库中存储 `128000`，前端 UI 会直接显示该数值而非 "default"，agent 也会收到明确的 budget 值。

## 验证

1. `cd backend && cargo test` — 确保编译和现有测试通过
2. 手动验证：创建新对话后检查 API 返回的 `thinking_budget` 字段为 `128000`
