# 前端代码审查指南

> 技术栈：Vue 3 · TypeScript · Pinia · Vite · Element Plus · Axios · Vitest
>
> 本文档为原则性审查指南，不绑定具体业务逻辑，聚焦技术栈的 best practice 和编码规范。

---

## 目录

1. [代码质量与通用规范](#1-代码质量与通用规范)
2. [TypeScript 类型安全](#2-typescript-类型安全)
3. [Vue 组件规范](#3-vue-组件规范)
4. [Composables 与逻辑复用](#4-composables-与逻辑复用)
5. [Pinia Store 规范](#5-pinia-store-规范)
6. [Vue Router 规范](#6-vue-router-规范)
7. [API 层规范](#7-api-层规范)
8. [WebSocket 规范](#8-websocket-规范)
9. [国际化 (i18n)](#9-国际化-i18n)
10. [性能优化](#10-性能优化)
11. [安全性](#11-安全性)
12. [可访问性 (a11y)](#12-可访问性-a11y)
13. [测试规范](#13-测试规范)
14. [CSS / 样式规范](#14-css--样式规范)

---

## 审查流程指引

### 审查优先级

Review 时建议按以下优先级关注：

1. **P0 — 必须修复**：安全漏洞、数据丢失风险、类型错误（`any`/`as` 滥用）、内存泄漏（未清理的监听/定时器）
2. **P1 — 强烈建议**：竞态条件、错误处理缺失、测试覆盖不足、可访问性缺陷
3. **P2 — 建议改进**：性能优化、代码组织、命名规范、CSS 变量使用
4. **P3 — 可选优化**：代码风格偏好、注释补充、微小重构

### 审查清单使用方式

- 每个 PR 不需要逐条检查所有项目，根据改动范围选择相关章节
- 新增组件 → 重点看第 3、4、12、14 章
- Store 变更 → 重点看第 5 章
- API 相关 → 重点看第 7、11 章
- 路由变更 → 重点看第 6 章
- 全栈功能 → 通读所有相关章节

---

## 1. 代码质量与通用规范

### 检查项

- [ ] 函数保持单一职责，一个函数只做一件事
- [ ] 常量使用 `const` + 大写蛇形命名（`MAX_RETRY_COUNT`），定义在使用作用域的顶部
- [ ] 高频查找使用 `Set` 或 `Map` 替代数组 `.includes()` / `.find()`
- [ ] 无依赖的并行异步操作使用 `Promise.all` 或 `Promise.allSettled`，而非顺序 `await`
- [ ] 错误处理使用 `try/catch` 并通过 UI 反馈（如 `ElMessage.error`）通知用户，而非仅 `console.error`
- [ ] 无 magic number / magic string，使用命名常量替代
- [ ] 辅助函数提取到模块顶层或独立 utils 文件，而非嵌套在闭包内部
- [ ] 早返回（early return）减少嵌套层级

### 正确示例

```typescript
// 并行无依赖请求
const [users, settings] = await Promise.all([
  fetchUsers(),
  fetchSettings(),
])

// 命名常量替代 magic number
const IDLE_TIMEOUT_MS = 600_000
const MAX_PENDING_EVENTS = 256
```

### 反模式

```typescript
// ❌ 顺序 await 无依赖请求
const users = await fetchUsers()
const settings = await fetchSettings()

// ❌ magic number
if (retryCount > 3) { ... }
setTimeout(callback, 600000)

// ❌ 只 console.error 不通知用户
catch (e) { console.error(e) }
```

---

## 2. TypeScript 类型安全

### 检查项

- [ ] 领域类型集中定义在 `types/` 目录下，而非散落在各文件中
- [ ] 类型导入使用 `import type { ... }`，避免运行时导入空模块
- [ ] 联合类型使用 discriminated union 模式（通过共有字段如 `type` 区分）
- [ ] 类型守卫使用 `is` 谓词函数，而非 `as` 强制断言
- [ ] API 响应通过泛型标注返回类型（`client.get<T>()`）
- [ ] 禁止使用 `any`（测试文件中 mock 对象除外），使用 `unknown` + 逐字段校验
- [ ] 外部数据（API 响应、WS 消息、用户输入）视为 `unknown`，经类型守卫后再使用
- [ ] 可空字段使用 `T | null`（JSON 中 null 是合法值），可选字段使用 `?:`
- [ ] 避免 `as` 类型断言，优先使用类型收窄（`typeof`、`in`、discriminated union）
- [ ] `Record<string, unknown>` 用于开放式 JSON 对象，而非 `object` 或 `any`

### 正确示例

```typescript
// discriminated union
type ContentBlock =
  | { type: 'text'; content: string }
  | { type: 'tool_call'; id: string; name: string }

// 类型守卫
function isToolCall(block: ContentBlock): block is ContentBlock & { type: 'tool_call' } {
  return block.type === 'tool_call'
}

// 可空 vs 可选
interface Conversation {
  title: string           // 必填
  model_name: string | null  // 必填但可为 null
  description?: string       // 可选（可能不存在）
}
```

### 反模式

```typescript
// ❌ as 强制断言
const data = response as UserData

// ❌ any
function handle(payload: any) { ... }

// ❌ 类型散落各处
// component-a.vue 中定义 interface User
// component-b.vue 中重复定义 interface User
```

---

## 3. Vue 组件规范

### 检查项

- [ ] 所有组件使用 `<script setup lang="ts">` 语法，不使用 Options API
- [ ] `defineProps` 使用类型参数语法 `defineProps<{ ... }>()`，而非运行时声明
- [ ] `defineEmits` 使用 tuple 语法 `defineEmits<{ event: [arg: Type] }>()`
- [ ] 组件名使用多词 PascalCase（`ChatInput`），避免与 HTML 元素冲突
- [ ] `v-for` 必须提供唯一且稳定的 `:key`（使用 ID，不用 index）
- [ ] `v-if` 和 `v-for` 不在同一元素上，用 `<template v-for>` 包裹或 `computed` 预过滤
- [ ] 模板引用使用 `ref<HTMLElement | null>(null)` 或 `ref<InstanceType<typeof Comp> | null>(null)` 类型标注
- [ ] 生命周期配对：`onMounted` 中建立的连接/监听必须在 `onUnmounted` 中清理
- [ ] 复杂模板表达式提取为 `computed` 属性
- [ ] 组件通过 `emit` 向上通信，不直接修改 props
- [ ] `v-model` 使用 `update:propName` emit 模式
- [ ] Element Plus 组件通过 auto-import 使用；`ElMessage`、`ElMessageBox` 等命令式 API 需手动 import
- [ ] Icon 从 `@element-plus/icons-vue` 显式导入
- [ ] `<script setup>` 内代码按以下顺序组织：imports → 类型定义 → props/emits → refs/reactive → computed → watch → 函数/方法 → 生命周期钩子
- [ ] `watch` 优先于 `watchEffect`（显式声明依赖更易维护），除非需要自动追踪多个依赖
- [ ] `watch` 的异步副作用使用 `onCleanup` 参数取消过期操作
- [ ] `watch` 的 `flush` 选项根据场景选择：`'pre'`（默认，DOM 更新前）、`'post'`（DOM 更新后，适合需要访问更新后 DOM 的场景）、`'sync'`（同步，慎用）
- [ ] 全局错误使用 `app.config.errorHandler` 统一捕获；组件树错误使用 `onErrorCaptured` 拦截
- [ ] 多属性元素每行一个属性，提高可读性和 diff 友好度

### 正确示例

```vue
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

const props = defineProps<{
  modelValue: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  submit: [content: string]
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const trimmed = computed(() => props.modelValue.trim())

let timer: ReturnType<typeof setInterval> | null = null
onMounted(() => { timer = setInterval(heartbeat, 30000) })
onUnmounted(() => { if (timer) clearInterval(timer) })
</script>
```

```typescript
// watch 带 cleanup 取消过期请求
watch(selectedId, async (id, _, onCleanup) => {
  const controller = new AbortController()
  onCleanup(() => controller.abort())
  const data = await fetchDetail(id, { signal: controller.signal })
  detail.value = data
})

// SFC 代码组织顺序
// 1. imports
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import type { User } from '@/types'
// 2. props / emits
const props = defineProps<{ userId: string }>()
const emit = defineEmits<{ update: [user: User] }>()
// 3. refs / reactive
const user = ref<User | null>(null)
// 4. computed
const displayName = computed(() => user.value?.name ?? 'Unknown')
// 5. watch
watch(() => props.userId, (id) => loadUser(id))
// 6. functions
async function loadUser(id: string) { ... }
// 7. lifecycle
onMounted(() => loadUser(props.userId))
```

### 反模式

```vue
<!-- ❌ v-if 和 v-for 同一元素 -->
<li v-for="item in list" v-if="item.active" :key="item.id">

<!-- ❌ 运行时 props 声明 -->
<script setup>
defineProps({ title: String, count: Number })
</script>

<!-- ❌ index 作为 key -->
<div v-for="(item, index) in list" :key="index">

<!-- ❌ 忘记清理 -->
<script setup>
onMounted(() => window.addEventListener('resize', handler))
// 缺少 onUnmounted 清理
</script>
```

---

## 4. Composables 与逻辑复用

### 检查项

- [ ] 可复用的有状态逻辑提取为 composable 函数，命名以 `use` 开头（`useCountdown`、`useDebounce`）
- [ ] Composable 文件放在 `composables/` 目录下，一个文件一个 composable
- [ ] Composable 接收参数时支持 `ref` 和普通值（使用 `toValue()` 或 `unref()` 兼容）
- [ ] Composable 返回值使用具名对象（`return { count, increment }`），不返回数组（除非只有 2 个值且语义明确）
- [ ] Composable 内部的副作用（事件监听、定时器）在 `onUnmounted` / `onScopeDispose` 中清理
- [ ] 不将简单的无状态工具函数包装为 composable（纯函数放 `utils/`）
- [ ] Composable 之间可以组合调用，但避免循环依赖
- [ ] `provide` / `inject` 使用 `InjectionKey<T>` 确保类型安全

### 正确示例

```typescript
// composables/useDebounce.ts
import { onScopeDispose, ref, watch, type Ref } from 'vue'

export function useDebounce<T>(source: Ref<T>, delay = 300) {
  const debounced = ref(source.value) as Ref<T>
  let timer: ReturnType<typeof setTimeout>

  watch(source, (val) => {
    clearTimeout(timer)
    timer = setTimeout(() => { debounced.value = val }, delay)
  })

  onScopeDispose(() => clearTimeout(timer))

  return debounced
}

// 类型安全的 provide/inject
import type { InjectionKey } from 'vue'

export const ThemeKey: InjectionKey<Ref<'light' | 'dark'>> = Symbol('theme')

// 提供方
provide(ThemeKey, theme)
// 注入方
const theme = inject(ThemeKey) // 自动推断为 Ref<'light' | 'dark'> | undefined
```

### 反模式

```typescript
// ❌ 无状态函数包装为 composable
export function useFormatDate(date: Date) {
  return date.toLocaleDateString() // 纯函数，应放 utils/
}

// ❌ 返回未命名数组（超过 2 个值时语义不清）
return [count, increment, decrement, reset]

// ❌ inject 无类型
const theme = inject('theme') // any 类型
```

---

## 5. Pinia Store 规范

### 检查项

- [ ] 使用 setup function 语法 `defineStore('name', () => { ... })`
- [ ] 响应式状态使用 `ref()`，派生状态使用 `computed()`
- [ ] 非响应式私有状态（WebSocket 实例、计数器、定时器 ID）使用 plain `let`，不放入 `ref()`
- [ ] 异步操作定义为 `async function`，而非箭头函数
- [ ] 处理异步竞态条件（如请求 ID 递增 + 返回前校验）
- [ ] 并发重复请求使用单例 Promise 去重模式
- [ ] `return` 对象只暴露必要的 state / computed / action，内部辅助函数不暴露
- [ ] 定时器在清理函数中正确 `clearTimeout` / `clearInterval`
- [ ] Store 内部不直接操作 DOM
- [ ] 使用 `storeToRefs()` 解构 state/getters，action 可直接解构

### 正确示例

```typescript
export const useDataStore = defineStore('data', () => {
  // 响应式状态
  const items = ref<Item[]>([])
  const loading = ref(false)
  const currentDetail = ref<ItemDetail | null>(null)

  // 派生状态
  const activeItems = computed(() => items.value.filter(i => i.active))

  // 非响应式私有状态
  let lastRequestId = 0

  // 单例 Promise 去重
  let loadPromise: Promise<void> | null = null
  async function loadItems() {
    if (loadPromise) return loadPromise
    loadPromise = (async () => {
      loading.value = true
      try { items.value = await api.fetchItems() }
      finally { loading.value = false; loadPromise = null }
    })()
    return loadPromise
  }

  // 竞态处理
  async function selectItem(id: string) {
    const requestId = ++lastRequestId
    const detail = await api.fetchDetail(id)
    if (requestId !== lastRequestId) return // 被更新的请求取代
    currentDetail.value = detail
  }

  return { items, loading, activeItems, currentDetail, loadItems, selectItem }
})
```

### 反模式

```typescript
// ❌ WebSocket 实例放入 ref（不必要的响应式代理）
const ws = ref<WebSocket | null>(null)

// ❌ 直接解构 store 破坏响应式
const { count, name } = useMyStore() // 失去响应式
// ✅ const { count, name } = storeToRefs(useMyStore())

// ❌ 暴露内部辅助函数
return { items, loadItems, _internalHelper } // _internalHelper 不应暴露

// ❌ 不处理竞态
async function select(id: string) {
  currentDetail.value = await api.fetchDetail(id) // 快速切换会显示旧数据
}
```

---

## 6. Vue Router 规范

### 检查项

- [ ] 路由组件使用动态 `import()` 懒加载，不同步导入
- [ ] 需要认证的路由通过 `meta` 字段标记（如 `meta: { requiresAuth: true }`），全局 `beforeEach` 守卫统一拦截
- [ ] 导航守卫中的异步操作（如 session 校验）正确 `await`，避免闪烁
- [ ] 路由参数通过 `useRoute().params` 获取并校验类型，不直接信任
- [ ] 404 / 未匹配路由有兜底页面（`path: '/:pathMatch(.*)*'`）
- [ ] 路由切换时清理前一页面的副作用（通过组件 `onUnmounted` 或路由守卫 `onBeforeRouteLeave`）
- [ ] 编程式导航使用 `router.push()` / `router.replace()`，不直接操作 `window.location`
- [ ] 路由路径使用 kebab-case（`/shared-chat/:token`），不用 camelCase

### 正确示例

```typescript
// 路由定义
const routes = [
  {
    path: '/login',
    component: () => import('./views/Login.vue'),
  },
  {
    path: '/chat',
    component: () => import('./views/Chat.vue'),
    meta: { requiresAuth: true },
  },
  {
    path: '/:pathMatch(.*)*',
    component: () => import('./views/NotFound.vue'),
  },
]

// 全局守卫
router.beforeEach(async (to) => {
  if (to.meta.requiresAuth) {
    await authStore.ensureSession()
    if (!authStore.isAuthenticated) return '/login'
  }
})
```

### 反模式

```typescript
// ❌ 每个路由单独写认证逻辑
{ path: '/chat', beforeEnter: async () => { await checkAuth() } }
{ path: '/settings', beforeEnter: async () => { await checkAuth() } }
// 应统一在全局 beforeEach 中处理

// ❌ 直接操作 location
window.location.href = '/login' // 会导致整页刷新

// ❌ 路由参数未校验
const id = route.params.id // 可能是 undefined 或数组
```

---

## 7. API 层规范

### 检查项

- [ ] 所有 HTTP 请求通过统一的 axios client 单例发起，不直接 `import axios`
- [ ] API 函数返回解包后的 `data`（`const { data } = await client.get<T>(...); return data`），不返回 `AxiosResponse`
- [ ] 响应类型通过泛型标注（`client.get<User[]>('/users')`）
- [ ] 路径参数使用 `encodeURIComponent()` 编码
- [ ] API 模块按资源分文件（`auth.ts`、`users.ts`、`conversations.ts`）
- [ ] 401 认证刷新通过拦截器统一处理，业务代码无需关心
- [ ] Token 刷新使用单例 Promise 去重，防止并发刷新
- [ ] 刷新请求本身不经过拦截器（使用裸 axios 或标记跳过）
- [ ] 文件上传使用 `FormData` + 正确的 `Content-Type`
- [ ] 函数签名使用具体类型，不用 `any`

### 正确示例

```typescript
// api/client.ts — 统一 client
const client = axios.create({
  baseURL: '/api',
  withCredentials: true,
})

client.interceptors.response.use(
  (res) => res,
  async (error) => {
    if (error.response?.status === 401 && !error.config._retry) {
      error.config._retry = true
      const ok = await refreshSession()
      if (ok) return client(error.config)
    }
    return Promise.reject(error)
  }
)

// api/users.ts — 类型化 API 函数
export async function getUser(id: string): Promise<User> {
  const { data } = await client.get<User>(`/users/${encodeURIComponent(id)}`)
  return data
}
```

### 反模式

```typescript
// ❌ 组件中直接 import axios
import axios from 'axios'
const { data } = await axios.get('/api/users')

// ❌ 返回整个 AxiosResponse
export async function getUsers() {
  return client.get<User[]>('/users') // 调用方需要 .data
}

// ❌ 多处重复实现 token 刷新
```

---

## 8. WebSocket 规范

### 检查项

- [ ] WebSocket 连接通过封装的 Manager 类管理，不直接使用原生 `WebSocket`
- [ ] 实现指数退避重连（初始延迟 → 倍增 → 上限封顶）
- [ ] 实现心跳机制（定时 ping，超时未收到 pong 则强制关闭触发重连）
- [ ] 监听浏览器生命周期事件（`visibilitychange` 恢复、`online` 重连）
- [ ] `disconnect` 时清理所有事件监听器和定时器
- [ ] 消息处理使用事件分发模式（`ws.on('type', handler)`），不在一个大 switch 中处理
- [ ] `send()` 返回值需检查，返回 `false` 时需回退乐观更新
- [ ] 预期断连场景（如容器重启）设置宽限窗口，避免误报错误
- [ ] 组件不直接监听 WS 事件，通过 store 中转

### 正确示例

```typescript
class WebSocketManager {
  private reconnectDelay = 1000
  private maxReconnectDelay = 30000
  private heartbeatInterval = 25000
  private heartbeatTimeout = 10000

  connect(url: string) {
    this.ws = new WebSocket(url)
    this.ws.onclose = () => this.scheduleReconnect()
    this.startHeartbeat()
    this.addLifecycleListeners()
  }

  disconnect() {
    this.intentionalClose = true
    this.stopHeartbeat()
    this.removeLifecycleListeners()
    this.ws?.close()
  }

  send(data: object): boolean {
    if (this.ws?.readyState !== WebSocket.OPEN) return false
    this.ws.send(JSON.stringify(data))
    return true
  }

  private scheduleReconnect() {
    if (this.intentionalClose) return
    setTimeout(() => this.connect(this.url), this.reconnectDelay)
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay)
  }
}
```

### 反模式

```typescript
// ❌ 直接使用原生 WebSocket
const ws = new WebSocket('ws://...')

// ❌ 忽略 send 返回值
ws.send(message) // 连接未就绪时静默丢失

// ❌ 组件直接监听 WS
onMounted(() => ws.on('message', handler)) // 应通过 store 中转
```

---

## 9. 国际化 (i18n)

### 检查项

- [ ] 所有用户可见文本通过翻译函数（如 `t('key')`）获取，不硬编码
- [ ] 翻译 key 按模块分层命名（`chat.send`、`settings.provider.title`）
- [ ] 带动态参数的文本使用占位符插值（`t('error.failed', { name })`），不用字符串拼接
- [ ] 新增文本同时提供所有支持的语言翻译
- [ ] 翻译字典使用 `as const` 断言确保类型安全
- [ ] `document.documentElement.lang` 随语言切换更新
- [ ] 日期、数字等格式化考虑 locale 差异

### 正确示例

```typescript
// 参数插值
t('file.uploadFailed', { name: file.name })
// 对应翻译: "上传 {name} 失败" / "Failed to upload {name}"

// key 分层
const messages = {
  en: {
    chat: { send: 'Send', cancel: 'Cancel' },
    settings: { title: 'Settings', save: 'Save' },
  },
  'zh-CN': {
    chat: { send: '发送', cancel: '取消' },
    settings: { title: '设置', save: '保存' },
  },
} as const
```

### 反模式

```typescript
// ❌ 硬编码文本
ElMessage.error('操作失败')

// ❌ 字符串拼接替代插值
ElMessage.error('上传 ' + file.name + ' 失败')

// ❌ 只加一种语言
'zh-CN': { newFeature: '新功能' }
// 缺少 en 翻译
```

---

## 10. 性能优化

### 检查项

- [ ] 路由组件使用懒加载 `() => import('./views/Foo.vue')`
- [ ] Element Plus 通过 `unplugin-vue-components` + `ElementPlusResolver` 按需加载，不全量导入
- [ ] 重型组件使用 `defineAsyncComponent` 延迟加载
- [ ] Vite 构建配置 `manualChunks` 分离大型第三方库（UI 库、markdown 渲染等）
- [ ] 大列表（100+ 项）使用虚拟滚动
- [ ] 大型不可变数据使用 `shallowRef()` / `shallowReactive()` 避免深层响应式开销
- [ ] `computed` 用于缓存派生计算，不在模板中重复计算
- [ ] `v-for` 中不使用 `v-if`，用 `computed` 预过滤
- [ ] 高频更新的非 UI 状态（如 WS 内部计数器）不放入响应式系统
- [ ] `watch` 中的昂贵操作加 `debounce` 或 `throttle`
- [ ] 流式内容更新直接修改数组元素属性，而非替换整个数组
- [ ] 纯静态内容使用 `v-once`，条件静态子树使用 `v-memo`
- [ ] 缓冲区/队列设置上限防止内存泄漏
- [ ] 优先使用 ES module 版本的依赖（如 `lodash-es` 而非 `lodash`）

### 正确示例

```typescript
// 路由懒加载
const routes = [
  { path: '/settings', component: () => import('./views/Settings.vue') },
]

// shallowRef 避免深层响应式
const largeDataset = shallowRef<DataPoint[]>([])

// computed 预过滤替代 v-for + v-if
const activeItems = computed(() => items.value.filter(i => i.active))
```

### 反模式

```typescript
// ❌ 同步导入路由组件
import Settings from './views/Settings.vue'
const routes = [{ path: '/settings', component: Settings }]

// ❌ 全量导入 Element Plus
import ElementPlus from 'element-plus'
app.use(ElementPlus)

// ❌ 无上限缓冲区
const buffer: Event[] = [] // 可能无限增长
```

---

## 11. 安全性

### 检查项

- [ ] Markdown / 富文本渲染必须经过 DOMPurify 消毒后再插入 DOM
- [ ] `v-html` 仅用于已消毒的内容，禁止直接渲染用户输入
- [ ] 动态 `:href` 绑定防范 `javascript:` 协议注入（使用 URL 白名单或 sanitize-url 库）
- [ ] 动态 `:style` 绑定不接受用户输入（防止 clickjacking）
- [ ] 不使用用户输入作为组件 `template` 字符串
- [ ] API Key 等敏感信息只在后端存储，前端只传输不持久化（不存 localStorage / sessionStorage）
- [ ] Cookie 认证正确配置 `withCredentials: true`
- [ ] 路由守卫正确拦截未认证访问
- [ ] `URL.createObjectURL` 创建的 Blob URL 及时 `revokeObjectURL` 释放
- [ ] 定期审计第三方依赖（`npm audit` / `pnpm audit`）
- [ ] 不在前端代码中硬编码密钥、凭证或内部 URL

### 正确示例

```typescript
// Markdown 消毒
import DOMPurify from 'dompurify'
import MarkdownIt from 'markdown-it'

const md = new MarkdownIt()
const safeHtml = DOMPurify.sanitize(md.render(userContent))

// Blob URL 及时释放
const url = URL.createObjectURL(blob)
downloadLink.href = url
downloadLink.click()
URL.revokeObjectURL(url)
```

### 反模式

```html
<!-- ❌ 未消毒的 v-html -->
<div v-html="userMessage.content"></div>

<!-- ❌ 动态 href 未校验 -->
<a :href="userProvidedUrl">Link</a>
```

```typescript
// ❌ 敏感信息存前端
localStorage.setItem('apiKey', key)

// ❌ Blob URL 泄漏
const url = URL.createObjectURL(blob) // 从未 revoke
```

---

## 12. 可访问性 (a11y)

### 检查项

- [ ] 图标按钮必须提供 `aria-label` 描述功能
- [ ] 装饰性元素标记 `aria-hidden="true"`
- [ ] 表单控件关联 `<label>`（`for` / `id` 匹配，或嵌套 `<label>` 包裹）
- [ ] 不使用 `placeholder` 作为唯一标签
- [ ] 交互元素使用语义化标签（`<button>`、`<a>`），不用 `<div @click>`
- [ ] 键盘导航可达所有交互元素（Tab 顺序合理，Enter/Space 触发操作）
- [ ] 标题层级顺序正确（`h1` → `h2` → `h3`），不跳级
- [ ] 使用语义化 landmark 元素（`<main>`、`<nav>`、`<header>`、`<footer>`）
- [ ] 颜色对比度满足 WCAG AA 标准（正文 4.5:1，大文本 3:1）
- [ ] `document.documentElement.lang` 设置正确的语言代码
- [ ] 路由切换后焦点合理重置
- [ ] 表单内非提交按钮设置 `type="button"`
- [ ] 视觉隐藏内容使用 CSS clip 方案，不用 `display: none`（屏幕阅读器需要读取）
- [ ] `disabled` 状态正确传递给原生/组件属性
- [ ] 测试辅助属性使用 `data-testid`，不依赖 CSS 类名定位

### 正确示例

```html
<!-- 图标按钮 -->
<button aria-label="发送消息" type="button">
  <SendIcon aria-hidden="true" />
</button>

<!-- 表单关联 -->
<label for="username">用户名</label>
<input id="username" type="text" autocomplete="username" />

<!-- 视觉隐藏但屏幕阅读器可读 -->
<span class="sr-only">当前页码</span>
```

```css
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
```

### 反模式

```html
<!-- ❌ div 模拟按钮 -->
<div @click="handleClick" class="btn">点击</div>

<!-- ❌ 图标按钮无 aria-label -->
<button><DeleteIcon /></button>

<!-- ❌ placeholder 当 label -->
<input placeholder="请输入用户名" />
```

---

## 13. 测试规范

### 检查项

- [ ] 测试文件放在 `__tests__/` 对应子目录下（`stores/`、`views/`、`components/`）
- [ ] 外部依赖通过 `vi.mock()` 模块级 mock，不在测试内部 mock
- [ ] 每个 `describe` 块的 `beforeEach` 中重置状态：`setActivePinia(createPinia())`、`vi.clearAllMocks()`
- [ ] 工厂函数创建测试数据（如 `makeUser()`、`makeMessage()`），避免重复构造
- [ ] 组件测试使用 `mount` + `global.plugins` 配置必要插件
- [ ] 异步测试正确 `await`（包括 `await nextTick()`、`await flushPromises()`）
- [ ] 定时器测试使用 `vi.useFakeTimers()` + `vi.advanceTimersByTimeAsync()`，`finally` 中 `vi.useRealTimers()`
- [ ] Mock 函数断言使用 `vi.mocked()` 获取类型安全的 mock
- [ ] 覆盖正常路径、失败路径和边界情况
- [ ] 竞态条件有专门测试（如快速切换、并发请求）
- [ ] WebSocket 使用 mock class 替代，通过 helper 函数提取注册的 handler
- [ ] `vi.mock` 不放在条件逻辑中（会被 hoisted 到文件顶部）
- [ ] `vi.stubEnv` / `vi.stubGlobal` 在 cleanup 中恢复
- [ ] 测试遵循 Arrange-Act-Assert 模式

### 正确示例

```typescript
// 模块级 mock
vi.mock('../../api/users', () => ({
  getUsers: vi.fn().mockResolvedValue([]),
  createUser: vi.fn(),
}))

// 每测试重置
beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

// 工厂函数
function makeUser(overrides: Partial<User> = {}): User {
  return { id: '1', name: 'Test', email: 'test@example.com', ...overrides }
}

// fake timers 配对清理
it('debounces search', async () => {
  vi.useFakeTimers()
  try {
    store.search('query')
    await vi.advanceTimersByTimeAsync(300)
    expect(api.search).toHaveBeenCalledOnce()
  } finally {
    vi.useRealTimers()
  }
})

// WS handler 提取
function getWsHandler(mockWs: any, event: string) {
  const call = mockWs.on.mock.calls.find((c: any[]) => c[0] === event)
  return call?.[1]
}
```

### 反模式

```typescript
// ❌ 忘记重置 Pinia（测试间状态泄漏）
// 缺少 beforeEach 中的 setActivePinia

// ❌ 条件 mock（vi.mock 会被 hoisted）
if (process.env.MODE === 'test') {
  vi.mock('./api') // 不会按预期工作
}

// ❌ fake timers 忘记清理
it('test', () => {
  vi.useFakeTimers()
  // ... 测试逻辑
  // 缺少 vi.useRealTimers()，影响后续测试
})

// ❌ 只测试正常路径
it('loads data', async () => {
  // 只测了成功，没测 API 失败、空数据、网络错误等
})
```

---

## 14. CSS / 样式规范

### 检查项

- [ ] 组件样式使用 `<style scoped>`，避免全局污染
- [ ] 颜色、间距、圆角、阴影、过渡使用 CSS 自定义属性（`--bg-main`、`--radius-md`、`--transition-fast`）
- [ ] 不硬编码颜色值（`#D97706`），使用已定义的 CSS 变量
- [ ] 布局使用 Flexbox / Grid，不用 float / position hack
- [ ] 响应式使用 `@media` 断点，断点值保持一致（如 768px、1024px）
- [ ] 类名使用 kebab-case（`chat-layout`、`sidebar-header`）
- [ ] 避免 `!important`（Element Plus 样式覆盖场景除外，需注释说明原因）
- [ ] 避免过度使用 `:deep()` 穿透 scoped 样式
- [ ] 最大宽度等全局约束使用 CSS 变量（`--max-width-chat`）
- [ ] 过渡动画使用变量（`--transition-fast`、`--transition-normal`），不硬编码 duration
- [ ] 避免内联 `style` 属性（简单一次性样式除外）
- [ ] 不使用 CSS 预处理器（SCSS/Less），保持纯 CSS + 自定义属性
- [ ] 新增 CSS 变量定义在 `variables.css` 中，不在组件内定义全局变量

### 正确示例

```css
/* 使用 CSS 变量 */
.card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  padding: 16px;
  transition: box-shadow var(--transition-fast);
}

.card:hover {
  box-shadow: var(--shadow-md);
}

/* 响应式 */
@media (max-width: 768px) {
  .sidebar { display: none; }
}
```

### 反模式

```css
/* ❌ 硬编码颜色 */
.header { background: #1a1a2e; color: #e0e0e0; }

/* ❌ 硬编码过渡时间 */
.btn { transition: all 0.3s ease; }

/* ❌ 过度穿透 */
:deep(.el-input__inner) { ... }
:deep(.el-button) { ... }
:deep(.el-dialog__body) { ... }
/* 如果需要大量 :deep，考虑使用 Element Plus 的 CSS 变量覆盖 */

/* ❌ !important 无注释 */
.custom-btn { color: red !important; }
```
