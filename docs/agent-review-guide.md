# Agent 端代码审查指南

> 技术栈：Python 3.12 · asyncio · LangChain · WebSocket · Pydantic v2 · pytest · httpx · tenacity · MCP
>
> 本文档为原则性审查指南，不绑定具体业务逻辑，聚焦技术栈的 best practice 和编码规范。

---

## 目录

1. [Python 通用规范](#1-python-通用规范)
2. [asyncio 异步编程](#2-asyncio-异步编程)
3. [LangChain Agent 模式](#3-langchain-agent-模式)
4. [WebSocket 通信](#4-websocket-通信)
5. [Pydantic 数据校验](#5-pydantic-数据校验)
6. [httpx 与网络请求](#6-httpx-与网络请求)
7. [安全性](#7-安全性)
8. [测试规范](#8-测试规范)

---

## 审查流程指引

### 审查优先级

Review 时建议按以下优先级关注：

1. **P0 — 必须修复**：安全漏洞（路径遍历、命令注入、敏感信息泄露）、数据丢失风险、资源泄漏（未关闭的连接/文件/子进程）
2. **P1 — 强烈建议**：异步竞态条件、错误处理缺失、类型安全问题、取消传播断裂、测试覆盖不足
3. **P2 — 建议改进**：性能优化、代码组织、命名规范、日志完善
4. **P3 — 可选优化**：代码风格偏好、注释补充、微小重构

### 审查清单使用方式

- 每个 PR 不需要逐条检查所有项目，根据改动范围选择相关章节
- 新增工具 → 重点看第 3、5、7 章
- Agent 循环变更 → 重点看第 2、3 章
- WebSocket 相关 → 重点看第 4 章
- 全栈功能 → 通读所有相关章节

---

## 1. Python 通用规范

### 类型安全

#### 检查项

- [ ] 模块顶部包含 `from __future__ import annotations` 以启用延迟类型求值
- [ ] 函数签名标注参数类型和返回类型，避免裸 `dict` / `list`（使用 `dict[str, Any]`、`list[str]`）
- [ ] 使用 `isinstance()` 进行类型检查，不使用 `type() ==`
- [ ] 联合类型使用 `X | Y` 语法（Python 3.10+），不使用 `Union[X, Y]`
- [ ] 可选类型使用 `str | None`，不使用 `Optional[str]`
- [ ] 回调函数类型使用 `Callable[[ArgType], ReturnType]` 标注
- [ ] `Any` 仅用于真正动态的外部数据（JSON payload），内部逻辑应使用具体类型
- [ ] 类型导入使用 `from typing import TYPE_CHECKING` 守卫避免循环导入

#### 正确示例

```python
from __future__ import annotations
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .agent import ChatAgent

def process_result(data: dict[str, Any], strict: bool = False) -> str | None:
    if isinstance(data, dict):
        return data.get("text")
    return None
```

#### 反模式

```python
# ❌ 裸 dict 无类型参数
def process(data: dict): ...

# ❌ 使用 type() 比较
if type(obj) == str: ...

# ❌ 旧式 Optional
from typing import Optional, Union
def foo(x: Optional[Union[str, int]]): ...
```

### 资源管理

#### 检查项

- [ ] 文件操作使用 `with` 语句（上下文管理器），确保异常时也能正确关闭
- [ ] 子进程使用 `process.kill()` + `process.wait()` 配对清理，避免僵尸进程
- [ ] 子进程超时终止使用进程组 kill（`os.killpg`），确保子进程的子进程也被清理
- [ ] 临时文件使用 `tempfile` 模块创建，不手动拼接 `/tmp` 路径
- [ ] 异步上下文管理器使用 `async with`，不手动调用 `__aenter__` / `__aexit__`
- [ ] 大文件读取使用分块读取或流式处理，不一次性 `read()` 全部内容
- [ ] 输出截断设置合理上限（如 50K 字符），防止内存溢出

#### 正确示例

```python
# 文件操作
with open(path, "r", encoding="utf-8") as f:
    content = f.read()

# 子进程清理
import os, signal
try:
    process = await asyncio.create_subprocess_shell(cmd, start_new_session=True)
    await asyncio.wait_for(process.wait(), timeout=30)
except asyncio.TimeoutError:
    os.killpg(os.getpgid(process.pid), signal.SIGKILL)
    await process.wait()
```

#### 反模式

```python
# ❌ 未使用 with，异常时文件句柄泄漏
f = open(path)
content = f.read()
f.close()

# ❌ 只 kill 主进程，子进程成为孤儿
process.kill()
# 缺少 process.wait()

# ❌ 一次性读取大文件
content = open(huge_file).read()  # 可能 OOM
```

### 错误处理

#### 检查项

- [ ] 捕获具体异常类型，不使用裸 `except:` 或 `except Exception`（除非是顶层兜底）
- [ ] 异常链使用 `raise NewError(...) from original_error` 保留原始堆栈
- [ ] 不使用 `assert` 做运行时校验（`-O` 模式会跳过 assert）
- [ ] 错误信息包含足够上下文（操作名、关键参数值），便于排查
- [ ] 顶层入口点设置兜底异常处理，防止未捕获异常导致静默退出
- [ ] 不吞没异常（catch 后至少 log 或 re-raise）

#### 正确示例

```python
# 具体异常 + 异常链
try:
    data = json.loads(raw)
except json.JSONDecodeError as e:
    raise ValueError(f"Invalid config format: {raw[:100]}") from e

# 顶层兜底
async def main():
    try:
        await run_agent()
    except Exception:
        logger.exception("Agent crashed")
        sys.exit(1)
```

#### 反模式

```python
# ❌ 裸 except 吞没所有异常
try:
    do_something()
except:
    pass

# ❌ 丢失异常链
except SomeError:
    raise NewError("failed")  # 缺少 from e

# ❌ assert 做运行时校验
assert user_id is not None, "user_id required"  # -O 模式下跳过
```

### 编码风格

#### 检查项

- [ ] 常量使用大写蛇形命名（`MAX_RETRY_COUNT`），定义在模块顶部
- [ ] 无 magic number / magic string，使用命名常量替代
- [ ] 函数保持单一职责，超过 50 行考虑拆分
- [ ] 早返回（early return）减少嵌套层级
- [ ] 可变默认参数使用 `None` + 函数内初始化，不使用 `[]` 或 `{}`
- [ ] 字符串格式化使用 f-string，不使用 `%` 或 `.format()`
- [ ] 列表/字典/集合推导式优先于 `map()` / `filter()`（可读性更好）
- [ ] 模块导入顺序：标准库 → 第三方库 → 本地模块，各组之间空行分隔

#### 正确示例

```python
# 可变默认参数
def process(items: list[str] | None = None) -> list[str]:
    items = items or []
    return [item.strip() for item in items]

# 早返回
def validate(data: dict[str, Any]) -> str | None:
    if "name" not in data:
        return None
    if not isinstance(data["name"], str):
        return None
    return data["name"].strip()
```

#### 反模式

```python
# ❌ 可变默认参数（所有调用共享同一个 list）
def process(items: list[str] = []):
    items.append("new")
    return items

# ❌ magic number
if retry_count > 3: ...
await asyncio.sleep(0.5)

# ❌ 深层嵌套
def handle(data):
    if data:
        if data.get("type"):
            if data["type"] == "message":
                # 实际逻辑在第 4 层缩进
```

### 日志规范

#### 检查项

- [ ] 使用 `logging` 模块，不使用 `print()` 输出调试信息
- [ ] 日志级别正确使用：`DEBUG`（开发调试）、`INFO`（关键流程节点）、`WARNING`（可恢复异常）、`ERROR`（不可恢复错误）
- [ ] 日志使用 `%s` 占位符（`logger.info("msg: %s", var)`），不使用 f-string（避免未输出时的格式化开销）
- [ ] 异常日志使用 `logger.exception()` 或 `logger.error(..., exc_info=True)` 保留堆栈
- [ ] 敏感信息（API key、token、密码）不出现在任何级别的日志中
- [ ] 高频操作（如每个 streaming chunk）不记录 INFO 级别日志（使用 DEBUG）
- [ ] 日志包含足够上下文（conversation_id、tool_name、provider），便于问题定位

#### 正确示例

```python
import logging

logger = logging.getLogger(__name__)

# 关键流程节点
logger.info("Agent initialized: provider=%s, model=%s, tools=%d", provider, model, len(tools))

# 异常保留堆栈
try:
    await connect()
except ConnectionError:
    logger.exception("WebSocket connection failed")

# 高频操作用 DEBUG
logger.debug("Received chunk: %d bytes", len(chunk))
```

#### 反模式

```python
# ❌ 使用 print
print(f"Connected to {url}")

# ❌ f-string 日志（即使日志级别不输出也会格式化）
logger.debug(f"Processing {len(items)} items with config {config}")

# ❌ 异常不保留堆栈
except Exception as e:
    logger.error(str(e))  # 丢失堆栈信息

# ❌ 高频操作用 INFO
async for chunk in stream:
    logger.info("Chunk received")  # 每秒可能数百条
```

---

## 2. asyncio 异步编程

### 事件循环与阻塞

#### 检查项

- [ ] 异步代码路径中不调用同步阻塞 I/O（`open()`、`os.path.*`、`subprocess.Popen`、`time.sleep()`）
- [ ] 必须调用同步 I/O 时使用 `asyncio.to_thread()` 或 `loop.run_in_executor()` 包装
- [ ] 使用 `asyncio.sleep()` 替代 `time.sleep()`
- [ ] 子进程使用 `asyncio.create_subprocess_shell()` / `asyncio.create_subprocess_exec()`，不使用 `subprocess.Popen`
- [ ] CPU 密集型计算使用 `run_in_executor(ProcessPoolExecutor())` 避免阻塞事件循环
- [ ] 不在异步函数中使用 `requests` 库（使用 `httpx` 的异步客户端）

#### 正确示例

```python
# 异步子进程
process = await asyncio.create_subprocess_shell(
    cmd,
    stdout=asyncio.subprocess.PIPE,
    stderr=asyncio.subprocess.PIPE,
    start_new_session=True,
)
stdout, stderr = await process.communicate()

# 同步 I/O 包装
content = await asyncio.to_thread(Path(file_path).read_text, encoding="utf-8")

# 异步 sleep
await asyncio.sleep(RETRY_DELAY_SECONDS)
```

#### 反模式

```python
# ❌ 异步函数中使用同步 I/O
async def read_config():
    with open("config.json") as f:  # 阻塞事件循环
        return json.load(f)

# ❌ 使用 requests（同步库）
async def fetch_data():
    return requests.get(url)  # 阻塞

# ❌ time.sleep 阻塞事件循环
async def retry():
    time.sleep(1)  # 应使用 await asyncio.sleep(1)
```

### Task 管理

#### 检查项

- [ ] 每个 `asyncio.create_task()` 都有对应的 `await` 或取消路径
- [ ] Task 取消使用 `task.cancel()` + `await task`（包裹在 `try/except asyncio.CancelledError` 中）
- [ ] 多任务并发使用 `asyncio.gather()` 或 `asyncio.wait()`，不手动管理 task 列表
- [ ] `asyncio.wait()` 明确指定 `return_when` 参数（`FIRST_COMPLETED` / `ALL_COMPLETED`）
- [ ] 后台 Task 的异常不被静默吞没（使用 `task.add_done_callback()` 或 `await` 检查）
- [ ] `finally` 块中清理所有创建的 Task
- [ ] 不在 `__del__` 中执行异步清理（使用 `async with` 或显式 `shutdown()` 方法）

#### 正确示例

```python
# Task 创建 + 清理
task = asyncio.create_task(long_running_operation())
try:
    result = await asyncio.wait_for(task, timeout=30)
except asyncio.TimeoutError:
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass

# 多任务并发 + 异常处理
results = await asyncio.gather(
    fetch_a(), fetch_b(), fetch_c(),
    return_exceptions=True,
)
for result in results:
    if isinstance(result, Exception):
        logger.error("Task failed: %s", result)
```

#### 反模式

```python
# ❌ 创建 Task 后不 await（异常静默丢失）
asyncio.create_task(background_job())
# 如果 background_job 抛异常，无人知晓

# ❌ cancel 后不 await（Task 可能仍在运行）
task.cancel()
# 缺少 await task

# ❌ asyncio.wait 不指定 return_when
done, pending = await asyncio.wait(tasks)  # 默认 ALL_COMPLETED，但不明确
```

### 取消传播

#### 检查项

- [ ] `asyncio.CancelledError` 不被意外吞没（catch 后应 re-raise 或执行清理后 re-raise）
- [ ] 长循环在每次迭代开始检查取消标志
- [ ] 工具执行之间检查取消状态，避免取消后继续执行后续工具
- [ ] 取消标志使用简单 `bool`（asyncio 单线程，无需锁），但确保检查点之间无 `await` 导致状态不一致
- [ ] 协作式取消：设置标志 → 检查标志 → 清理 → 退出，不强制 kill 协程

#### 正确示例

```python
# 协作式取消
async def agent_loop(self):
    while not self._cancelled:
        result = await self._call_llm()
        if self._cancelled:
            break
        for tool_call in result.tool_calls:
            if self._cancelled:
                break
            await self._execute_tool(tool_call)

# CancelledError 正确传播
async def run_with_cleanup():
    try:
        await long_operation()
    except asyncio.CancelledError:
        await cleanup_resources()
        raise  # 必须 re-raise
```

#### 反模式

```python
# ❌ 吞没 CancelledError
try:
    await operation()
except asyncio.CancelledError:
    pass  # 取消被吞没，调用方无法感知

# ❌ 循环中不检查取消
async def process_all(items):
    for item in items:  # 即使被取消也会处理完所有 item
        await process(item)
```

### 共享状态与并发安全

#### 检查项

- [ ] asyncio 单线程模型下，简单标志位（`bool`、`int`）无需锁
- [ ] 需要原子性的多步操作使用 `asyncio.Lock`
- [ ] Lock 在 `finally` 块中释放（或使用 `async with lock:`）
- [ ] 不在持有 Lock 期间执行耗时 I/O（会阻塞其他协程获取锁）
- [ ] 队列通信使用 `asyncio.Queue`，不使用 `list` + 轮询
- [ ] 不使用 `threading.Lock`（在 asyncio 中会阻塞事件循环）

#### 正确示例

```python
# asyncio.Lock 保护多步操作
lock = asyncio.Lock()

async def safe_increment():
    async with lock:
        value = await read_counter()
        await write_counter(value + 1)

# asyncio.Queue 替代 list + 轮询
queue: asyncio.Queue[StreamEvent] = asyncio.Queue()

async def producer():
    await queue.put(event)

async def consumer():
    event = await queue.get()  # 自动等待，不浪费 CPU
```

#### 反模式

```python
# ❌ threading.Lock 阻塞事件循环
import threading
lock = threading.Lock()
async def bad():
    with lock:  # 阻塞整个事件循环
        await do_something()

# ❌ list + 轮询
events = []
async def poll():
    while True:
        if events:
            event = events.pop(0)
            await handle(event)
        await asyncio.sleep(0.01)  # 浪费 CPU
```

---

## 3. LangChain Agent 模式

### 工具定义

#### 检查项

- [ ] 每个工具继承 `BaseTool`，设置 `name`、`description`、`args_schema`
- [ ] `args_schema` 使用 Pydantic `BaseModel`，每个字段提供 `Field(description=...)` — 描述直接影响 LLM 调用准确性
- [ ] 工具同时实现 `_run()` 和 `_arun()`，异步工具在 `_arun()` 中实现核心逻辑
- [ ] 工具内部异常不向外抛出，统一返回结构化错误结果（如 `{success: False, error: "..."}`)
- [ ] 工具返回值使用统一的结果 schema，不返回裸字符串或任意 dict
- [ ] 文件系统操作的工具必须校验路径在允许的工作目录内（防止路径遍历）
- [ ] 工具的 `description` 清晰描述功能、参数含义和使用场景，避免模糊描述
- [ ] 只读工具和写入工具有明确的元数据标记，便于权限控制

#### 正确示例

```python
class SearchInput(BaseModel):
    pattern: str = Field(description="正则表达式搜索模式")
    path: str = Field(default=".", description="搜索起始目录，相对于工作区根目录")
    max_results: int = Field(default=100, ge=1, le=1000, description="最大返回结果数")

class SearchTool(BaseTool):
    name = "search"
    description = "在工作区内按正则表达式搜索文件内容"
    args_schema: type[BaseModel] = SearchInput

    def _run(self, pattern: str, path: str = ".", max_results: int = 100) -> dict:
        try:
            results = do_search(pattern, path, max_results)
            return {"success": True, "text": format_results(results), "data": {"count": len(results)}}
        except Exception as e:
            return {"success": False, "error": str(e)}
```

#### 反模式

```python
# ❌ 无 args_schema，LLM 无法获知参数格式
class MyTool(BaseTool):
    name = "my_tool"
    description = "does something"
    # 缺少 args_schema

# ❌ 工具抛出异常（会中断 agent 循环）
def _run(self, query: str) -> str:
    result = external_api(query)  # 可能抛异常
    return result  # 裸字符串返回

# ❌ 模糊的 description
description = "A useful tool"  # LLM 不知道何时使用
```

### 流式处理

#### 检查项

- [ ] 使用 `llm.astream()` 进行流式输出，不使用 `ainvoke()` 后一次性返回
- [ ] 流式 chunk 类型检查（`isinstance(chunk, AIMessageChunk)`），跳过非预期类型
- [ ] 文本 delta 在 yield 前进行清理/转义（防止特殊字符破坏 JSON 序列化）
- [ ] 流式事件使用统一的事件类型封装（如 `StreamEvent(type, data)`），不直接 yield 裸数据
- [ ] 流式处理中的异常不导致整个流中断，应 yield 错误事件后正常结束
- [ ] 流式 chunk 的 `tool_call_chunks` 按 `index` 正确累积，处理分片 JSON args
- [ ] thinking/reasoning delta 通过 provider 抽象层提取，不硬编码特定 provider 的格式

#### 正确示例

```python
# 流式处理 + 类型检查
accumulated = None
async for chunk in llm.astream(messages):
    if not isinstance(chunk, AIMessageChunk):
        continue
    accumulated = chunk if accumulated is None else accumulated + chunk

    text_delta = extract_text_delta(chunk)
    if text_delta:
        yield StreamEvent(type="text_delta", data={"content": sanitize(text_delta)})

# 工具调用 chunk 累积
tool_calls: dict[int, dict] = {}
for tc_chunk in chunk.tool_call_chunks:
    idx = tc_chunk["index"]
    if idx not in tool_calls:
        tool_calls[idx] = {"name": "", "args": ""}
    tool_calls[idx]["name"] += tc_chunk.get("name", "")
    tool_calls[idx]["args"] += tc_chunk.get("args", "")
```

#### 反模式

```python
# ❌ 一次性调用，无流式输出
result = await llm.ainvoke(messages)

# ❌ 不检查 chunk 类型
async for chunk in llm.astream(messages):
    yield chunk.content  # chunk 可能不是 AIMessageChunk

# ❌ 硬编码 provider 格式
if chunk.response_metadata.get("type") == "thinking":  # 只适用于特定 provider
```

### Agent 循环

#### 检查项

- [ ] Agent 循环设置最大迭代次数上限，防止无限循环
- [ ] 每次迭代开始检查取消标志
- [ ] 工具执行之间检查取消标志
- [ ] 无工具调用时正常结束循环（yield complete 事件）
- [ ] 过滤无效工具调用（如 `name` 为空的 ghost tool call）
- [ ] 工具执行结果作为 `ToolMessage` 追加到历史，`tool_call_id` 正确匹配
- [ ] 循环结束时 yield 完整的 `AIMessage`（包含 `usage_metadata`）到历史
- [ ] 支持运行时事件流（工具执行期间的中间事件转发）

#### 正确示例

```python
async def agent_loop(self):
    iteration = 0
    while iteration < MAX_ITERATIONS:
        if self._cancelled:
            break
        iteration += 1

        # 调用 LLM
        ai_message = await self._stream_llm_response()
        self.messages.append(ai_message)

        tool_calls = [tc for tc in ai_message.tool_calls if tc.get("name")]
        if not tool_calls:
            yield StreamEvent(type="complete", data={})
            return

        # 执行工具
        for tc in tool_calls:
            if self._cancelled:
                break
            result = await self._execute_tool(tc)
            self.messages.append(ToolMessage(content=result, tool_call_id=tc["id"]))
```

#### 反模式

```python
# ❌ 无迭代上限
while True:
    result = await call_llm()
    if not result.tool_calls:
        break
    # LLM 可能无限循环调用工具

# ❌ 不过滤 ghost tool call
for tc in ai_message.tool_calls:  # 可能包含 name="" 的无效调用
    await execute(tc)

# ❌ tool_call_id 不匹配
self.messages.append(ToolMessage(content=result, tool_call_id="wrong_id"))
```

### 消息历史管理

#### 检查项

- [ ] 历史重建正确处理多种消息格式（legacy flat format / structured parts format）
- [ ] `AIMessage` 和 `ToolMessage` 严格配对（每个 tool_call 对应一个 ToolMessage）
- [ ] 历史中的 thinking/reasoning block 根据 provider 要求保留或移除
- [ ] 历史 normalize 移除 provider 特定的元数据（如 response ID），避免重放时冲突
- [ ] 空内容 block（空字符串 text、空 thinking）在 normalize 时清理
- [ ] 历史截断（edit/regenerate）正确处理消息边界，不破坏 AI-Tool 消息配对

#### 正确示例

```python
# 历史 normalize
def normalize_history(messages: list[BaseMessage], provider: str) -> list[BaseMessage]:
    normalized = []
    for msg in messages:
        if isinstance(msg, AIMessage) and isinstance(msg.content, list):
            # 移除空 block
            content = [b for b in msg.content if not is_empty_block(b)]
            # 移除 provider 特定 ID
            msg = msg.copy(update={"content": content})
            if hasattr(msg, "id") and is_provider_generated_id(msg.id):
                msg.id = None
        normalized.append(msg)
    return normalized
```

#### 反模式

```python
# ❌ AI-Tool 消息不配对
messages.append(AIMessage(content="", tool_calls=[tc1, tc2]))
messages.append(ToolMessage(content=result1, tool_call_id=tc1["id"]))
# 缺少 tc2 对应的 ToolMessage → LLM 报错

# ❌ 截断破坏配对
messages = messages[:index]  # 可能截断到 AIMessage 和 ToolMessage 之间
```

### Provider 抽象

#### 检查项

- [ ] Provider 特定逻辑通过策略模式（Strategy Pattern）封装，不在 agent 循环中 `if/elif` 判断 provider
- [ ] 新增 provider 只需添加新的策略类，不修改 agent 核心逻辑
- [ ] Provider 能力（feature flags）与运行时行为（参数构建、内容提取）分离为两层
- [ ] LLM 初始化通过工厂函数统一创建，不直接实例化特定 provider 的类
- [ ] Provider 特定的参数（thinking budget、reasoning effort）通过抽象方法构建

#### 正确示例

```python
# 策略模式
class ProviderContract(ABC):
    @abstractmethod
    def build_thinking_kwargs(self, budget: int) -> dict: ...

    @abstractmethod
    def extract_text_delta(self, chunk) -> str | None: ...

class AnthropicContract(ProviderContract):
    def build_thinking_kwargs(self, budget: int) -> dict:
        return {"thinking": {"type": "enabled", "budget_tokens": budget}}

# 工厂函数
def create_llm(provider: str, model: str, api_key: str, **kwargs):
    return init_chat_model(model, model_provider=provider, api_key=api_key, **kwargs)
```

#### 反模式

```python
# ❌ agent 循环中硬编码 provider 逻辑
if provider == "anthropic":
    kwargs["thinking"] = {"type": "enabled"}
elif provider == "openai":
    kwargs["reasoning"] = {"effort": "high"}
# 每加一个 provider 都要改 agent 核心代码
```

### Subagent 委托

#### 检查项

- [ ] Subagent 只能使用只读工具（通过 `read_only` 元数据过滤），不能执行写入操作
- [ ] 嵌套 subagent 调用有深度限制（通常最大深度 1），防止递归爆炸
- [ ] 深度计数器使用 `asyncio.Lock` 保护，确保并发安全
- [ ] Subagent 使用独立的 provider/model/api_key 配置，不共享主 agent 的凭证
- [ ] Subagent 的运行时事件通过 event sink 回调转发给主 agent，不阻塞主循环
- [ ] Subagent 执行结果收集为结构化 trace（text、thinking、tool_call、tool_result block），不返回裸文本
- [ ] Subagent 工具列表排除自身（`explore`、`task`），防止递归调用

#### 正确示例

```python
# 深度限制 + 锁保护
class SubagentRunner:
    def __init__(self):
        self._depth = 0
        self._depth_lock = asyncio.Lock()

    async def run_subagent(self, query: str) -> dict:
        async with self._depth_lock:
            if self._depth >= MAX_DEPTH:
                return make_error("Subagent nesting depth exceeded")
            self._depth += 1
        try:
            # 只使用只读工具
            tools = [t for t in all_tools if tool_is_read_only(t)]
            agent = ChatAgent(tools=tools, ...)
            trace = await self._collect_trace(agent, query)
            return make_success(text=format_trace(trace))
        finally:
            async with self._depth_lock:
                self._depth -= 1
```

#### 反模式

```python
# ❌ subagent 可使用写入工具
subagent_tools = all_tools  # 应过滤为只读

# ❌ 无深度限制
async def run_subagent(self, query):
    # subagent 内部又调用 explore → 无限递归

# ❌ 不过滤自身工具
tools = [t for t in all_tools if tool_is_read_only(t)]
# 忘记排除 explore/task 工具本身
```

---

## 4. WebSocket 通信

### 连接生命周期

#### 检查项

- [ ] 连接建立后发送就绪信号，等待初始化消息
- [ ] 连接参数（URL、token）不在日志中明文输出
- [ ] 设置合理的消息大小上限（`max_size`），防止恶意大消息导致 OOM
- [ ] 连接关闭时清理所有关联资源（运行中的 Task、MCP 连接、子进程）
- [ ] 使用 `async with` 管理连接生命周期

#### 正确示例

```python
async with websockets.connect(url, max_size=MAX_MESSAGE_BYTES) as ws:
    await ws.send(json.dumps({"type": "ready"}))
    try:
        async for raw in ws:
            await handle_message(json.loads(raw))
    finally:
        await cleanup_all_resources()
```

#### 反模式

```python
# ❌ 无消息大小限制
ws = await websockets.connect(url)  # 默认可能接受超大消息

# ❌ token 出现在日志中
logger.info(f"Connecting to {url}?token={token}")

# ❌ 不清理资源
async for raw in ws:
    await handle(raw)
# 连接断开后，运行中的 Task 成为孤儿
```

### 消息处理

#### 检查项

- [ ] JSON 解析失败记录警告日志但不中断连接
- [ ] 未知消息类型记录警告日志但不中断连接
- [ ] 消息分发使用字典映射或 match/case，不使用长 if/elif 链
- [ ] 发送消息前检查连接状态，断开时不尝试发送
- [ ] 错误消息使用结构化格式（`{type: "error", code: "...", message: "..."}`），不发送裸字符串

#### 正确示例

```python
HANDLERS = {
    "init": handle_init,
    "user_message": handle_user_message,
    "cancel": handle_cancel,
}

async def dispatch(raw: str):
    try:
        msg = json.loads(raw)
    except json.JSONDecodeError:
        logger.warning("Invalid JSON received: %s", raw[:200])
        return

    handler = HANDLERS.get(msg.get("type"))
    if handler:
        await handler(msg)
    else:
        logger.warning("Unknown message type: %s", msg.get("type"))
```

#### 反模式

```python
# ❌ JSON 解析失败中断连接
msg = json.loads(raw)  # 异常未捕获 → 连接断开

# ❌ 长 if/elif 链
if msg["type"] == "init": ...
elif msg["type"] == "user_message": ...
elif msg["type"] == "cancel": ...
# 每加一种消息都要改分发逻辑
```

### 重连策略

#### 检查项

- [ ] 使用指数退避重连（初始延迟 → 倍增 → 上限封顶）
- [ ] 设置最大重连次数，超过后放弃并报错退出
- [ ] 只对可恢复的错误重连（`ConnectionClosedError`、`ConnectionRefusedError`），不对逻辑错误重连
- [ ] 重连成功后重置退避延迟
- [ ] 使用成熟的重试库（如 `tenacity`），不手写重试逻辑

#### 正确示例

```python
from tenacity import retry, stop_after_attempt, wait_exponential, retry_if_exception_type

@retry(
    stop=stop_after_attempt(MAX_RECONNECT_ATTEMPTS),
    wait=wait_exponential(multiplier=1, min=1, max=30),
    retry=retry_if_exception_type((ConnectionClosedError, ConnectionRefusedError)),
)
async def connect_with_retry():
    async with websockets.connect(url) as ws:
        await session.run(ws)
```

#### 反模式

```python
# ❌ 手写重试 + 无上限
while True:
    try:
        await connect()
    except Exception:  # 所有异常都重试
        await asyncio.sleep(1)  # 固定延迟，无退避

# ❌ 重连不重置状态
# 重连后继续使用旧的 agent 实例，历史状态可能不一致
```

### 优雅关闭

#### 检查项

- [ ] 注册 `SIGTERM` / `SIGINT` 信号处理器触发关闭流程
- [ ] 关闭流程：设置关闭标志 → 取消当前 Task → 清理外部连接（MCP 等）→ 关闭 WebSocket
- [ ] 信号处理器中不执行耗时操作（只设置标志或调用 `loop.call_soon_threadsafe`）
- [ ] 消息循环检查关闭标志，收到信号后不再处理新消息
- [ ] `finally` 块确保清理逻辑一定执行

#### 正确示例

```python
async def run(self, ws):
    try:
        async for raw in ws:
            if self._shutdown:
                break
            await self._handle_message(raw)
    finally:
        if self._current_task:
            self._current_task.cancel()
            try:
                await self._current_task
            except asyncio.CancelledError:
                pass
        await self._mcp_manager.shutdown()
```

#### 反模式

```python
# ❌ 信号处理器中执行异步操作
def handle_signal(sig, frame):
    await cleanup()  # 信号处理器中不能 await

# ❌ 不检查关闭标志
async for raw in ws:
    await handle(raw)  # 收到 SIGTERM 后仍在处理消息
```

---

## 5. Pydantic 数据校验

### Schema 设计

#### 检查项

- [ ] 工具输入 schema 继承 `BaseModel`，每个字段使用 `Field(description=...)` 描述
- [ ] 数值字段设置合理边界（`ge=0`、`le=1000`、`gt=0`）
- [ ] 可选字段使用 `Field(default=...)` 提供合理默认值
- [ ] 枚举类型使用 `Literal["a", "b", "c"]` 约束，不使用裸 `str`
- [ ] 复杂嵌套结构拆分为多个 Model，不在一个 Model 中定义过深的嵌套
- [ ] Model 的 `description` 或 docstring 描述整体用途

#### 正确示例

```python
from pydantic import BaseModel, Field
from typing import Literal

class SearchInput(BaseModel):
    """在工作区内搜索文件内容"""
    pattern: str = Field(description="正则表达式搜索模式")
    path: str = Field(default=".", description="搜索起始目录")
    max_results: int = Field(default=100, ge=1, le=1000, description="最大返回结果数")
    case_sensitive: bool = Field(default=True, description="是否区分大小写")
    output_mode: Literal["content", "files", "count"] = Field(
        default="files", description="输出模式"
    )
```

#### 反模式

```python
# ❌ 无 description，LLM 不知道参数含义
class SearchInput(BaseModel):
    pattern: str
    path: str = "."
    max_results: int = 100

# ❌ 无边界约束
max_results: int = Field(default=100)  # 用户可传 999999999

# ❌ 裸 str 替代枚举
mode: str = "files"  # LLM 可能传入任意字符串
```

### 校验边界

#### 检查项

- [ ] 外部数据（WebSocket 消息、API 响应）使用 Pydantic 校验后再使用
- [ ] 内部数据传递可使用 dataclass 或 plain dict（不必所有内部数据都用 Pydantic）
- [ ] `model_validate()` 替代直接构造（处理外部数据时）
- [ ] 校验错误返回有意义的错误信息，不暴露内部实现细节
- [ ] 不使用 `model_validate(obj, strict=False)` 处理安全敏感数据（宽松模式可能接受意外类型）

#### 正确示例

```python
# 外部数据校验
try:
    config = AgentConfig.model_validate(init_data)
except ValidationError as e:
    logger.error("Invalid init data: %s", e.errors())
    await send_error("Invalid configuration")
    return

# 内部数据用 dataclass（轻量）
@dataclass
class StreamEvent:
    type: str
    data: dict[str, Any]
```

#### 反模式

```python
# ❌ 外部数据不校验
config = AgentConfig(**init_data)  # 缺少字段时报 TypeError，错误信息不友好

# ❌ 所有内部数据都用 Pydantic（过度校验）
class InternalCounter(BaseModel):  # 简单计数器不需要 Pydantic
    value: int = 0
```

### 常见陷阱

#### 检查项

- [ ] 不使用 `assert` 做 Pydantic validator 中的校验（`-O` 模式跳过）
- [ ] `@field_validator` 和 `@model_validator` 必须返回值（忘记 return 会导致字段变 `None`）
- [ ] `@field_validator(mode='before')` 中只能访问已校验的字段（字段按定义顺序校验，后定义的字段此时尚未校验）
- [ ] Pydantic v2 中 `Config` 类改为 `model_config = ConfigDict(...)`
- [ ] `model_dump()` 替代 v1 的 `.dict()`，`model_validate()` 替代 `.parse_obj()`
- [ ] 可变默认值使用 `Field(default_factory=list)` 而非 `Field(default=[])`

#### 正确示例

```python
from pydantic import field_validator, ConfigDict

class ToolInput(BaseModel):
    model_config = ConfigDict(strict=True)

    tags: list[str] = Field(default_factory=list)

    @field_validator("tags")
    @classmethod
    def validate_tags(cls, v: list[str]) -> list[str]:
        if len(v) > 10:
            raise ValueError("Too many tags (max 10)")
        return v  # 必须 return
```

#### 反模式

```python
# ❌ validator 忘记 return
@field_validator("name")
@classmethod
def clean_name(cls, v):
    v.strip()  # 忘记 return v → 字段变 None

# ❌ 可变默认值（虽然 Pydantic v2 会深拷贝，但不够显式，且与 dataclass 行为不一致）
tags: list[str] = Field(default=[])  # 应使用 default_factory=list

# ❌ v1 API
data = model.dict()  # v2 应使用 model.model_dump()
```

---

## 6. httpx 与网络请求

### 检查项

- [ ] 使用 `httpx.AsyncClient` 进行异步 HTTP 请求，不使用 `requests`
- [ ] 设置合理的超时（`timeout=httpx.Timeout(connect=5, read=30)`），不使用无限超时
- [ ] 响应体大小检查（通过 `Content-Length` header 或流式读取 + 累计计数），防止下载超大文件导致 OOM
- [ ] HTML 响应转 Markdown 时使用 `html2text` 等库，不手写正则解析 HTML
- [ ] 请求异常（`httpx.TimeoutException`、`httpx.HTTPStatusError`）捕获并返回结构化错误
- [ ] `AsyncClient` 使用 `async with` 管理生命周期，或在 shutdown 时显式 `await client.aclose()`
- [ ] 不在请求 URL 中拼接未编码的用户输入（使用 `httpx.URL` 或 `urllib.parse.quote`）
- [ ] 敏感 header（API key、Authorization）不记录到日志中
- [ ] SSE（Server-Sent Events）流式响应使用 `httpx-sse` 等库解析，不手写解析逻辑

#### 正确示例

```python
async with httpx.AsyncClient(timeout=httpx.Timeout(connect=5, read=30)) as client:
    try:
        response = await client.get(url, follow_redirects=True)
        response.raise_for_status()

        # 大小检查
        content_length = int(response.headers.get("content-length", 0))
        if content_length > MAX_RESPONSE_BYTES:
            return make_error(f"Response too large: {content_length} bytes")

        return make_success(text=response.text)
    except httpx.TimeoutException:
        return make_error("Request timed out")
    except httpx.HTTPStatusError as e:
        return make_error(f"HTTP {e.response.status_code}")
```

#### 反模式

```python
# ❌ 无超时
response = await client.get(url)  # 可能永远挂起

# ❌ 不检查响应大小
content = response.text  # 可能是 GB 级响应

# ❌ 使用 requests（同步库）
import requests
response = requests.get(url)  # 阻塞事件循环

# ❌ URL 拼接未编码
url = f"https://api.example.com/search?q={user_input}"  # 注入风险
```

---

## 7. 安全性

### 路径遍历防护

#### 检查项

- [ ] 所有文件操作的路径参数经过沙箱校验（resolve 后检查 `is_relative_to(workspace)`）
- [ ] 使用 `Path.resolve()` 解析符号链接后再校验，防止 symlink 绕过
- [ ] 路径校验失败返回明确错误，不静默忽略
- [ ] 新增的文件操作工具必须复用已有的路径校验函数，不自行实现

#### 正确示例

```python
from pathlib import Path

def resolve_safe_path(user_path: str, workspace: str) -> Path:
    """解析路径并确保在工作区内"""
    base = Path(workspace).resolve()
    target = (base / user_path).resolve()
    if not target.is_relative_to(base):
        raise ValueError(f"Path escapes workspace: {user_path}")
    return target
```

#### 反模式

```python
# ❌ 不 resolve，symlink 可绕过
target = Path(workspace) / user_path
if ".." not in user_path:  # 不够：symlink 可指向外部
    return target

# ❌ 每个工具自行实现路径校验
# tool_a.py: 手写校验逻辑 A
# tool_b.py: 手写校验逻辑 B（可能有遗漏）
```

### 子进程安全

#### 检查项

- [ ] 子进程设置超时，超时后强制终止
- [ ] 使用 `start_new_session=True` 创建新进程组，确保 kill 时子进程的子进程也被清理
- [ ] 子进程输出截断到合理大小（防止恶意命令输出 GB 级数据）
- [ ] 子进程在 Docker 容器内运行时，确认容器有资源限制（CPU、内存）
- [ ] 不将用户输入直接拼接到 shell 命令中（如果必须使用 `shell=True`，确保在沙箱环境中）

#### 正确示例

```python
process = await asyncio.create_subprocess_shell(
    command,
    stdout=asyncio.subprocess.PIPE,
    stderr=asyncio.subprocess.PIPE,
    start_new_session=True,  # 新进程组
)
try:
    stdout, stderr = await asyncio.wait_for(
        process.communicate(), timeout=EXECUTION_TIMEOUT
    )
    # 截断输出
    stdout = stdout[:MAX_OUTPUT_BYTES]
except asyncio.TimeoutError:
    os.killpg(os.getpgid(process.pid), signal.SIGKILL)
    await process.wait()
```

#### 反模式

```python
# ❌ 无超时
stdout, stderr = await process.communicate()  # 可能永远挂起

# ❌ 只 kill 主进程
process.kill()  # 子进程的子进程成为孤儿

# ❌ 不截断输出
output = stdout.decode()  # 可能是 GB 级数据
```

### 敏感信息保护

#### 检查项

- [ ] API key 不出现在日志输出中（包括 debug 级别）
- [ ] 错误消息不包含内部路径、堆栈跟踪或 API key
- [ ] WebSocket 发送的错误消息使用结构化格式，不包含原始异常信息
- [ ] 环境变量中的敏感值不在启动日志中打印
- [ ] 工具执行结果中的内部字段（如 `llm_content`）在发送到前端前移除

#### 正确示例

```python
# 错误消息不泄露内部信息
async def send_error(ws, code: str, message: str):
    await ws.send(json.dumps({
        "type": "error",
        "code": code,
        "message": message,  # 用户友好的消息
    }))

# 日志中隐藏敏感值
logger.info("Connecting to provider: %s, model: %s", provider, model)
# 不记录 api_key
```

#### 反模式

```python
# ❌ 日志泄露 API key
logger.debug(f"Config: {config}")  # config 包含 api_key

# ❌ 原始异常发送给前端
except Exception as e:
    await ws.send(json.dumps({"error": str(e)}))  # 可能包含内部路径

# ❌ 内部字段泄露
await ws.send(json.dumps(tool_result))  # 包含 llm_content（base64 数据）
```

### MCP 安全

#### 检查项

- [ ] MCP 服务器配置中的环境变量正确解析，不将 JSON 字符串作为裸值传递
- [ ] MCP 工具的只读标记正确设置，子 agent 只能使用只读工具
- [ ] MCP 服务器启动失败时正确清理已启动的服务器，不留下孤儿进程
- [ ] MCP 工具名称冲突时有明确的处理策略（前缀区分或拒绝加载）
- [ ] 仅支持受信任的 MCP transport（如 `stdio`），不盲目支持所有 transport

#### 正确示例

```python
# MCP 服务器清理
async def setup_mcp_servers(configs):
    started = []
    try:
        for config in configs:
            server = await start_mcp_server(config)
            started.append(server)
        return started
    except Exception:
        # 启动失败时清理已启动的服务器
        for server in started:
            await server.shutdown()
        raise
```

#### 反模式

```python
# ❌ 启动失败不清理
servers = []
for config in configs:
    servers.append(await start_mcp_server(config))  # 第 3 个失败时，前 2 个泄漏

# ❌ 子 agent 可使用写入工具
subagent_tools = all_tools  # 应过滤为只读工具
```

---

## 8. 测试规范

### 异步测试

#### 检查项

- [ ] `pyproject.toml` 中配置 `asyncio_mode = "auto"`，无需手动标记 `@pytest.mark.asyncio`
- [ ] 异步 fixture 使用 `async def` 定义
- [ ] 测试中的 `await` 不遗漏（遗漏会导致 coroutine 未执行但测试通过）
- [ ] 异步生成器测试使用 `async for` 收集结果，不使用 `list()`

#### 正确示例

```python
# pyproject.toml
# [tool.pytest.ini_options]
# asyncio_mode = "auto"

async def test_stream_events():
    events = []
    async for event in agent.handle_message("hello"):
        events.append(event)
    assert any(e.type == "complete" for e in events)
```

#### 反模式

```python
# ❌ 遗漏 await（测试通过但逻辑未执行）
async def test_something():
    agent.process("input")  # 缺少 await，coroutine 被丢弃
    assert True  # 永远通过

# ❌ 同步收集异步生成器
events = list(agent.handle_message("hello"))  # TypeError
```

### Mock 策略

#### 检查项

- [ ] 异步方法使用 `AsyncMock`，同步方法使用 `MagicMock`（混用会导致 `coroutine was never awaited` 警告）
- [ ] `@patch` 在导入位置 patch，不在定义位置（`patch("module_a.func")` 而非 `patch("module_b.func")`）
- [ ] Mock LLM 的 `astream` 返回异步生成器，yield `AIMessageChunk` 对象（不是裸字符串）
- [ ] Mock 的 `.bind_tools()` 和 `.bind()` 返回 self（保持链式调用）
- [ ] 多轮对话测试使用 `side_effect` 返回不同的异步生成器序列
- [ ] 使用 Fake 对象替代复杂的 Mock 链（如 WebSocket fake、Agent fake）

#### 正确示例

```python
# Mock LLM streaming
async def fake_stream(*args, **kwargs):
    yield AIMessageChunk(content="Hello ")
    yield AIMessageChunk(content="world")

mock_llm = AsyncMock()
mock_llm.astream = fake_stream
mock_llm.bind_tools.return_value = mock_llm
mock_llm.bind.return_value = mock_llm

# 多轮对话
streams = [fake_tool_call_stream, fake_text_stream]
mock_llm.astream = AsyncMock(side_effect=streams)

# Fake WebSocket
class FakeWebSocket:
    def __init__(self, responses: list[str]):
        self._responses = iter(responses)
        self.sent: list[str] = []

    async def send(self, data: str):
        self.sent.append(data)

    async def recv(self) -> str:
        return next(self._responses)
```

#### 反模式

```python
# ❌ 同步 Mock 用于异步方法
mock_llm = MagicMock()
mock_llm.astream = MagicMock()  # 返回 MagicMock 而非 coroutine

# ❌ 在定义位置 patch
@patch("langchain.chat_models.init_chat_model")  # 应 patch 导入位置
def test_create_model(): ...

# ❌ bind_tools 不返回 self
mock_llm.bind_tools.return_value = None  # agent 调用 bind_tools().astream() 会报错
```

### Fixture 设计

#### 检查项

- [ ] 共享 fixture 定义在 `conftest.py` 中，不在测试文件中重复定义
- [ ] Fixture 保持原子性（一个 fixture 只做一件事）
- [ ] 工厂 fixture 使用 `**overrides` 模式，允许测试自定义部分字段
- [ ] 文件系统 fixture 使用 `tmp_path`（pytest 内置），不手动创建临时目录
- [ ] Fixture 的 teardown 逻辑放在 `yield` 之后（generator fixture 模式）

#### 正确示例

```python
# conftest.py
@pytest.fixture
def workspace(tmp_path):
    """提供临时工作目录"""
    return tmp_path

@pytest.fixture
def make_config():
    """工厂 fixture，支持自定义覆盖"""
    def _make(**overrides):
        defaults = {
            "provider": "openai",
            "model": "gpt-4",
            "api_key": "test-key",
        }
        defaults.update(overrides)
        return AgentConfig(**defaults)
    return _make

# Generator fixture with teardown
@pytest.fixture
async def mcp_manager():
    manager = McpManager()
    yield manager
    await manager.shutdown()
```

#### 反模式

```python
# ❌ 每个测试文件重复定义 fixture
# test_a.py 和 test_b.py 都定义了相同的 workspace fixture

# ❌ 手动创建临时目录
import tempfile
workspace = tempfile.mkdtemp()  # 测试结束后不会自动清理

# ❌ fixture 做太多事
@pytest.fixture
def everything():
    workspace = create_workspace()
    config = create_config()
    agent = create_agent(config)
    return workspace, config, agent  # 应拆分为 3 个 fixture
```

### 测试组织与覆盖

#### 检查项

- [ ] 测试按类组织（`TestBashTool`、`TestAgentLoop`），相关测试归入同一个类
- [ ] 覆盖正常路径、错误路径和边界情况
- [ ] 竞态条件有专门测试（如快速取消、并发消息）
- [ ] 集成测试验证完整的 agent 循环（LLM → 工具调用 → 工具结果 → LLM 回复）
- [ ] 测试遵循 Arrange-Act-Assert 模式
- [ ] 测试名称描述行为而非实现（`test_returns_error_on_timeout` 而非 `test_timeout_branch`）
- [ ] 不测试私有方法的实现细节（除非是关键内部逻辑的单元测试）
- [ ] 测试辅助函数提取到共享模块（如 `result_helpers.py`），不在测试文件中重复

#### 正确示例

```python
class TestBashTool:
    def test_returns_output_on_success(self, workspace):
        tool = BashTool(workspace=str(workspace))
        result = tool._run(command="echo hello")
        assert result["success"] is True
        assert "hello" in result["text"]

    def test_returns_error_on_timeout(self, workspace):
        tool = BashTool(workspace=str(workspace))
        result = tool._run(command="sleep 999", timeout=1)
        assert result["success"] is False
        assert "timeout" in result["error"].lower()

    def test_truncates_large_output(self, workspace):
        tool = BashTool(workspace=str(workspace))
        result = tool._run(command="yes | head -100000")
        assert len(result["text"]) <= MAX_OUTPUT_SIZE
```

#### 反模式

```python
# ❌ 测试名称不描述行为
def test_bash_1(): ...
def test_bash_2(): ...

# ❌ 只测正常路径
def test_read_file(workspace):
    # 只测了文件存在的情况
    # 缺少：文件不存在、路径遍历、二进制文件、超大文件

# ❌ 过度 mock 导致测试无意义
def test_agent():
    # mock 了 LLM、所有工具、WebSocket → 测试只验证了 mock 的行为
```

