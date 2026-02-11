# Claude.ai System Prompt — 外部工具/功能需求分析

本文档分析 Claude.ai 风格 system prompt 中引用的所有外部工具和功能需求，
用于评估当前项目的实现差距和未来开发方向。

## 1. 已实现的工具

以下工具在当前项目中已有实现（`agent/src/tools/`）：

| 工具 | Prompt 中的描述 | 实现状态 |
|------|----------------|---------|
| bash | 执行 shell 命令 | 已实现 |
| read | 读取文件内容 | 已实现 |
| write | 写入文件 | 已实现 |
| edit | 编辑文件（精确替换） | 已实现 |
| glob | 文件模式匹配搜索 | 已实现 |
| grep | 文件内容搜索 | 已实现 |
| web_fetch | 获取网页内容 | 已实现 |
| code_interpreter | Python 代码执行 | 已实现 |

## 2. Prompt 中引用但未实现的功能

### 2.1 Web 搜索（Web Search）
- 描述：Prompt 提到"use web search tools if available"用于获取最新信息
- 当前状态：仅有 `web_fetch`（获取指定 URL），无搜索引擎集成
- 实现建议：集成搜索 API（如 Brave Search、SerpAPI、Tavily）

### 2.2 版本控制集成（Version Control）
- 描述：Prompt 提到"use version control when making significant changes"
- 当前状态：可通过 `bash` 工具执行 git 命令，但无专用 git 工具
- 实现建议：可考虑添加专用 git 工具以提供更安全的操作（如自动 stash、冲突检测）

### 2.3 文件备份（Backup）
- 描述：Prompt 提到"Create backups"
- 当前状态：无自动备份机制
- 实现建议：在 write/edit 操作前自动创建 `.bak` 文件或利用 git

## 3. Prompt 中引用的行为能力（非工具）

以下是 prompt 中描述的行为准则，不需要额外工具但影响 agent 行为：

### 3.1 安全审查
- 拒绝恶意软件、武器等有害内容的请求
- 保护用户隐私
- 安全漏洞的负责任披露
- 当前实现：依赖 LLM 自身的安全对齐

### 3.2 代码质量保证
- 代码正确性验证（通过运行代码）
- 遵循项目现有代码规范
- 安全编码实践
- 当前实现：依赖 LLM 判断 + code_interpreter 工具

### 3.3 信息来源引用
- 引用网页信息来源
- 当前实现：依赖 LLM 在 web_fetch 结果中提取来源

## 4. MCP 扩展点

通过 MCP（Model Context Protocol）服务器，以下功能可以按需扩展：

| 功能 | MCP 服务器示例 |
|------|--------------|
| 数据库查询 | PostgreSQL MCP、SQLite MCP |
| API 调用 | REST API MCP |
| 图像生成 | DALL-E MCP、Stable Diffusion MCP |
| 文档检索 | RAG MCP |
| 浏览器自动化 | Puppeteer MCP、Playwright MCP |

## 5. 优先级建议

1. **高优先级**：Web 搜索 — 显著提升信息获取能力
2. **中优先级**：Git 专用工具 — 提升代码操作安全性
3. **低优先级**：自动备份 — 可通过 git 间接实现
