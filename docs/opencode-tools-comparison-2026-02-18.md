# 当前项目 Agent 工具 vs OpenCode 工具对比报告

- 报告日期：2026-02-18
- 对比范围：内置工具能力 + 工具治理能力（权限、MCP、自定义工具、子代理调度）
- 结论口径：
  - “有/没有”按默认可用的第一方能力判断。
  - “可扩展能力”（比如通过 MCP 扩展出来）单独标注，不和内置能力混为一谈。
  - 涉及 OpenCode 的描述，以其官方文档和公开源码为准；源码与文档冲突时会明确标“版本差异或推断”。

## 一、结论先看（给业务和产品同学看的短版）

1. 你们这边在“多媒体与数据实验”上更强：内置了 `code_interpreter`（Python/JS 代码执行）和 `image_generation`（文生图/参考图编辑），OpenCode 内置工具里没有这两类等价能力。
2. OpenCode 在“治理和可控性”上更强：权限是细粒度、可交互授权（allow/ask/deny），还能按命令模式、路径模式、子代理维度控制。你们目前主要是“工作区边界限制 + 只读标签”，治理粒度明显更粗。
3. 双方共有的 bash/read/write/edit/glob/grep/web/task/MCP 能力，核心差别不是“能不能做”，而是“做的时候多可控、多稳、多可追踪”。OpenCode 在这一层做得更系统化；你们在实现上更直接、更轻，改造成本也更低。

---

## 二、我们有、OpenCode 没有（或默认不内置）

### 1) `code_interpreter`（Python/JavaScript 内置执行）

- 你们有：
  - 支持 `python` / `javascript` 两种语言执行。见 `agent/src/tools/code_interpreter.py:85`、`agent/src/tools/code_interpreter.py:95`。
  - 会扫描新生成的图像/音视频文件并自动返回 `sandbox:///` 引用。见 `agent/src/tools/code_interpreter.py:33`、`agent/src/tools/code_interpreter.py:132`。
- OpenCode：官方内置工具页未列出等价的“代码解释器”工具（其能力更偏 bash + 文件工具 + 任务调度）。

### 2) `image_generation`（内置图像生成/编辑）

- 你们有：
  - 工具名 `image_generation`。见 `agent/src/tools/image_gen.py:99`。
  - 支持 OpenAI 与 Google 两个图像 provider。见 `agent/src/tools/image_gen.py:132`、`agent/src/tools/image_gen.py:134`。
  - 支持 `reference_image` 作为编辑输入。见 `agent/src/tools/image_gen.py:90`、`agent/src/tools/image_gen.py:168`。
- OpenCode：官方内置工具页未列出图像生成/编辑类工具。

### 3) `read` 对音视频的直接媒体返回（当前实现形态）

- 你们有：`read` 遇到视频/音频文件会返回可访问引用和元数据（并做体积上限控制）。见 `agent/src/tools/file_ops.py:125`、`agent/src/tools/file_ops.py:18`。
- OpenCode `read` 侧重文本/目录、图片和 PDF（图片/PDF走附件返回），二进制文件会直接拒读（源码行为）。

---

## 三、OpenCode 有、我们没有

### 1) 目录树工具 `list`

- OpenCode 提供 `list`（目录树 + 过滤），并单独有 `list` 权限项。
- 你们当前没有等价内置工具（你们通常用 `glob` + `read` + `bash ls` 组合实现）。

### 2) `patch`（补丁应用）作为一等内置工具

- OpenCode 工具页有 `patch`，且归在 `edit` 权限下统一治理。
- 你们当前没有专门的 patch 工具；主要依赖 `edit`（字符串替换）与 `write`（整文件覆盖）。

### 3) `lsp`（实验性）工具

- OpenCode 有 `lsp`（definition/references/hover/call hierarchy 等），需实验开关。
- 你们当前没有对外暴露独立 LSP 工具。

### 4) `question` 工具（执行中向用户提问）

- OpenCode 有 `question`，支持在执行流中抛出结构化问题并收集用户选择。
- 你们当前没有等价工具（你们的交互主要在会话层，不是工具层）。

### 5) `skill` 工具（按需装载 SKILL.md）

- OpenCode 把“读取并注入技能说明”做成工具能力。
- 你们项目目前没有等价内置工具（虽然你们有 AGENTS.md 机制，但不是 agent 运行时的一般工具能力）。

### 6) Todo 工具链（`todowrite` / `todoread`）

- OpenCode 工具页提供 todo 读写，并支持对子代理默认禁用。
- 你们当前没有等价内置 todo 工具。

### 7) 更完整的工具治理体系（这块差距最大）

- OpenCode：
  - 权限动作 `allow / ask / deny`。
  - 规则支持通配、最后匹配生效、外部目录单独授权、doom-loop 防护、按 agent 覆盖。
  - `ask` 可一次性放行或会话内持久放行相似模式。
- 你们：
  - 主要是工作区路径防逃逸 + 读写工具区分 + 子代理 read-only 过滤。
  - 没有同等级别的交互式授权策略。

### 8) MCP 远程能力与 OAuth 管理

- OpenCode 支持 local/remote MCP、header、timeout、OAuth 自动流、`opencode mcp auth/list/logout`。
- 你们当前 MCP 管理器只支持 `stdio` 本地启动，不支持 remote/OAuth。

### 9) 写后自动格式化链路（Formatter）

- OpenCode 对写入/编辑后可自动触发语言格式化器，并可按项目配置启停。
- 你们当前没有内置“写后自动格式化”工具链。

---

## 四、双方都有的工具：能力差异（重点）

## 1) `bash`

- 你们：
  - 默认超时 30 秒；输出总长度 50k 字符，stdout/stderr 各 25k。见 `agent/src/tools/bash.py:25`、`agent/src/tools/bash.py:17`。
  - 执行模型直接：命令执行 -> 结果返回，未做交互式权限批准。
- OpenCode：
  - 默认超时更长（源码里是 2 分钟默认），且工具调用要求带“命令意图描述”。
  - 会先做权限评估/询问（bash、external_directory），再执行。
- 影响：
  - 你们执行快，但在高风险命令上的防误触弱。
  - OpenCode 约束更多，安全和可审计性更强。

## 2) `read`

- 你们：
  - `offset` 实参是 0 起始语义（实现中按行号做阈值判断），默认 `limit=2000`。见 `agent/src/tools/file_ops.py:29`、`agent/src/tools/file_ops.py:33`。
  - 支持图片 base64、音视频链接化返回。
- OpenCode：
  - `offset` 明确 1-indexed，目录也可直接读；并有读权限审查、外部目录检查、二进制拒读。
  - 图片/PDF 走附件通道，文本有字节和单行长度上限。
- 影响：
  - 你们在多媒体回传体验更直接。
  - OpenCode 在“超大文本防爆”和“目录场景”更成熟。

## 3) `write`

- 你们：
  - 语义简单：创建父目录 + 覆盖写入。见 `agent/src/tools/file_ops.py:215`。
- OpenCode：
  - 写入前后串联了权限确认、文件时间一致性检查、diff 记录、LSP 诊断反馈。
- 影响：
  - 你们实现轻、快，适合简单场景。
  - OpenCode 更像“带护栏的写入流水线”，更适合多人/高风险仓库。

## 4) `edit`

- 你们：
  - 精确替换模式，默认要求唯一匹配；多匹配需 `replace_all=true`。见 `agent/src/tools/file_ops.py:263`。
- OpenCode：
  - 在精确替换之外，还做了多种“纠错/归一化匹配”策略（行修剪、缩进弹性、上下文锚点等），并输出 diff + LSP 诊断。
- 影响：
  - 你们行为可预测、简单。
  - OpenCode 在“模型给的 oldString 不够精确”时更抗失败。

## 5) `glob`

- 你们：
  - 最多 1000 条，按字母序排序。见 `agent/src/tools/search.py:101`、`agent/src/tools/search.py:118`。
  - 支持花括号展开。
- OpenCode：
  - 最多 100 条（源码），结果按修改时间排序；底层依赖 ripgrep 文件枚举。
  - 并受权限系统和 `.gitignore/.ignore` 规则影响。
- 影响：
  - 你们更适合“全量盘点”。
  - OpenCode更倾向“最新变更优先、上下文更干净”。

## 6) `grep`

- 你们：
  - Python 正则实现，支持 `context` 前后文行返回，这是一个实用优势。见 `agent/src/tools/search.py:143`。
- OpenCode：
  - 用 ripgrep，速度和忽略规则管理更强，且纳入权限体系。
- 影响：
  - 你们在“带上下文追问题”上手感好。
  - OpenCode 在大仓库检索性能和一致性上更强。

## 7) `web_fetch` / `webfetch`

- 你们：
  - 默认 30 秒，HTML 转文本，按字符长度截断。见 `agent/src/tools/web.py:51`、`agent/src/tools/web.py:26`。
- OpenCode：
  - 支持返回格式 `text/markdown/html`，超时可配（上限 120 秒），响应体有 5MB 保护，且含权限确认。
- 影响：
  - 你们更简洁。
  - OpenCode 对“网页形态差异 + 大响应保护”更完整。

## 8) `web_search` / `websearch`

- 你们：
  - 内置 `web_search`，直接打 Exa MCP 端点，25 秒超时。见 `agent/src/tools/web.py:14`、`agent/src/tools/web.py:114`。
- OpenCode：
  - 同样走 Exa，但可用性受 provider/环境变量门控（OpenCode provider 或 `OPENCODE_ENABLE_EXA`）。
- 影响：
  - 你们默认可用路径更直。
  - OpenCode发布面更可控，便于按部署策略开关。

## 9) `task`

- 你们：
  - 只支持 `explore` 子代理类型，明确 read-only，禁止嵌套子代理。见 `agent/src/subagents.py:81`、`agent/src/subagents.py:99`。
  - 主代理会流式回传 `task_trace_delta`（可观测性不错）。见 `agent/src/agent.py:813`。
- OpenCode：
  - `task` 能调度更通用的子代理体系，支持 `task_id` 恢复会话，且可通过 `permission.task` 控制可调用子代理集合。
  - 子代理中默认禁用 todo 工具（可配置放开）。
- 影响：
  - 你们“探索子代理”定位清晰、成本低。
  - OpenCode“编排能力”更强，更适合复杂多工流。

## 10) MCP

- 你们：
  - 只支持 `stdio`，非 stdio 会被跳过。见 `agent/src/mcp/manager.py:44`。
  - 有 read-only 元数据和覆盖机制（对你们的 explore 子代理很实用）。见 `agent/src/tools/capabilities.py:62`。
- OpenCode：
  - 支持 local + remote + OAuth + CLI 管理认证，且支持服务器级开关、超时、header。
- 影响：
  - 你们实现简单稳定，但扩展面窄。
  - OpenCode更偏平台化接入。

---

## 五、工具治理能力：两边“方法论”差异

### 你们当前项目

- 核心机制是“执行边界 + 工具约束”：
  - 路径必须在工作区内（防目录逃逸）。见 `agent/src/tools/_paths.py:22`。
  - MCP 工具有 read-only 标注和覆盖机制。
  - `task` 子代理可硬限制为只读工具集。
- 优点：实现直观、心智负担低。
- 短板：细粒度授权与审计能力不足（比如“允许 git status 但拒绝 git push”这种级别）。

### OpenCode

- 核心机制是“声明式权限 + 运行时交互授权”：
  - 规则级 allow/ask/deny。
  - 命令/路径模式匹配。
  - 外部目录、重复调用防护、agent 级覆盖。
- 优点：适合多人协作与高风险仓库。
- 代价：配置复杂度更高。

---

## 六、重点建议（按投入产出比排序）

1. 先补“权限层”而不是先补新工具。
   - 最小可行版：给 `bash`、`write/edit`、`web_fetch` 增加 allow/ask/deny。
   - 这样能最快缩小和 OpenCode 在“可控性”上的核心差距。

2. 给 `write/edit` 加“写后检查”链路。
   - 不一定一步到位做 LSP 全量，先做最小诊断回传也有价值。

3. 增加 `list` 工具。
   - 这会显著降低模型在目录探索时对 bash 的依赖，提升可解释性。

4. 评估是否引入“远程 MCP + OAuth”。
   - 如果你们目标是平台化生态接入，这项是战略能力；如果只是单仓工程助手，可后置。

---

## 七、证据与引用

### 当前项目（本地源码）

- 工具清单与装配：`agent/src/tools/__init__.py:33`
- Bash：`agent/src/tools/bash.py:25`
- 文件读写改：`agent/src/tools/file_ops.py:25`
- 搜索工具：`agent/src/tools/search.py:48`
- Web 工具：`agent/src/tools/web.py:22`
- Code Interpreter：`agent/src/tools/code_interpreter.py:81`
- 图像生成：`agent/src/tools/image_gen.py:72`
- Task 工具：`agent/src/tools/task.py:24`
- 子代理策略：`agent/src/subagents.py:72`
- MCP 管理器：`agent/src/mcp/manager.py:26`
- 工具能力标注：`agent/src/tools/capabilities.py:8`
- 路径安全：`agent/src/tools/_paths.py:8`

### OpenCode 官方文档

- Tools: https://opencode.ai/docs/tools
- Permissions: https://opencode.ai/docs/permissions
- MCP servers: https://opencode.ai/docs/mcp-servers
- Custom tools: https://opencode.ai/docs/custom-tools
- Formatters: https://opencode.ai/docs/formatters
- Agents: https://opencode.ai/docs/agents
- LSP servers: https://opencode.ai/docs/lsp-servers

### OpenCode 公开源码（dev 分支快照）

- Bash: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/bash.ts
- Read: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/read.ts
- Write: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/write.ts
- Edit: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/edit.ts
- Glob: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/glob.ts
- WebFetch: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/webfetch.ts
- WebSearch: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/websearch.ts
- Task: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/task.ts
- List(Ls): https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/ls.ts
- LSP: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/lsp.ts
- Question: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/question.ts
- Skill: https://raw.githubusercontent.com/sst/opencode/dev/packages/opencode/src/tool/skill.ts

---

## 八、说明（避免误读）

- OpenCode 文档页更新时间为 2026-02-17；本报告检索日期为 2026-02-18。
- 个别工具（如 `patch`/`todoread`）在文档与 dev 源码目录命名上存在轻微不一致的迹象，报告中已优先采用“文档公开能力 + 可抓取源码证据”共同判断。
