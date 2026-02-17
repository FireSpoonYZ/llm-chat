import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { ElMessage } from 'element-plus'
import type { Conversation, Message, ContentBlock, WsMessage, ToolResult } from '../types'
import * as convApi from '../api/conversations'
import { refreshSession } from '../api/auth'
import { WebSocketManager } from '../api/websocket'
import { t } from '../i18n'

function generateUUID(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  // Fallback for non-secure contexts / older browsers
  return '10000000-1000-4000-8000-100000000000'.replace(/[018]/g, c =>
    (+c ^ crypto.getRandomValues(new Uint8Array(1))[0] & 15 >> +c / 4).toString(16)
  )
}

function normalizeToolResult(raw: unknown): ToolResult {
  if (typeof raw === 'object' && raw !== null) {
    const obj = raw as Record<string, unknown>

    // New envelope format
    if (typeof obj.kind === 'string' && typeof obj.text === 'string' && typeof obj.success === 'boolean') {
      return {
        kind: obj.kind,
        text: obj.text,
        success: obj.success,
        error: typeof obj.error === 'string' ? obj.error : null,
        data: typeof obj.data === 'object' && obj.data !== null ? obj.data as Record<string, unknown> : {},
        meta: typeof obj.meta === 'object' && obj.meta !== null ? obj.meta as Record<string, unknown> : {},
      }
    }

    // Legacy bash structured format
    if (obj.kind === 'bash' && typeof obj.text === 'string') {
      const exitCode = typeof obj.exit_code === 'number' ? obj.exit_code : null
      const timedOut = Boolean(obj.timed_out)
      const errorFlag = Boolean(obj.error)
      const success = !timedOut && !errorFlag && (exitCode === 0 || exitCode === null)
      return {
        kind: 'bash',
        text: obj.text,
        success,
        error: success ? null : (timedOut ? 'command timed out' : 'command execution failed'),
        data: {
          stdout: typeof obj.stdout === 'string' ? obj.stdout : '',
          stderr: typeof obj.stderr === 'string' ? obj.stderr : '',
          exit_code: exitCode,
        },
        meta: {
          timed_out: timedOut,
          truncated: Boolean(obj.truncated),
          duration_ms: typeof obj.duration_ms === 'number' ? obj.duration_ms : undefined,
        },
      }
    }

    if (typeof obj.text === 'string') {
      return { kind: 'text', text: obj.text, success: true, error: null, data: {}, meta: {} }
    }
  }
  return { kind: 'text', text: String(raw ?? ''), success: true, error: null, data: {}, meta: {} }
}

export const useChatStore = defineStore('chat', () => {
  type ToolCallBlock = ContentBlock & { type: 'tool_call' }
  type BufferedTaskTraceDelta = {
    eventType: string
    payload: Record<string, unknown>
  }

  const MAX_PENDING_TASK_TRACE_EVENTS = 256

  const conversations = ref<Conversation[]>([])
  const currentConversationId = ref<string | null>(null)
  const messages = ref<Message[]>([])
  const streamingBlocks = ref<ContentBlock[]>([])
  const isStreaming = ref(false)
  const isWaiting = ref(false)
  const totalMessages = ref(0)
  const wsConnected = ref(false)
  const sendFailed = ref(false)

  // Backward-compatible computed: concatenate all text blocks
  const streamingContent = computed(() =>
    streamingBlocks.value
      .filter((b): b is ContentBlock & { type: 'text' } => b.type === 'text')
      .map(b => b.content)
      .join('')
  )

  let ws: WebSocketManager | null = null
  let lastSelectRequestId = 0
  const pendingTaskTraceByToolCallId = new Map<string, BufferedTaskTraceDelta[]>()

  function ensureTaskTrace(taskBlock: ToolCallBlock): Record<string, unknown>[] {
    const taskResult = normalizeToolResult(taskBlock.result)
    taskResult.kind = 'task'
    const data = {
      ...(taskResult.data ?? {}),
    } as Record<string, unknown>
    const trace = Array.isArray(data.trace)
      ? data.trace.filter((x): x is Record<string, unknown> => typeof x === 'object' && x !== null)
      : []
    data.trace = trace
    taskResult.data = data
    taskBlock.result = taskResult
    return trace
  }

  function applyTaskTraceDelta(taskBlock: ToolCallBlock, eventType: string, payload: Record<string, unknown>) {
    const trace = ensureTaskTrace(taskBlock)

    if (eventType === 'assistant_delta') {
      const delta = typeof payload.delta === 'string' ? payload.delta : ''
      if (!delta) return
      const last = trace[trace.length - 1]
      if (last?.type === 'text') {
        last.content = String(last.content ?? '') + delta
      } else {
        trace.push({ type: 'text', content: delta })
      }
      return
    }

    if (eventType === 'thinking_delta') {
      const delta = typeof payload.delta === 'string' ? payload.delta : ''
      if (!delta) return
      const last = trace[trace.length - 1]
      if (last?.type === 'thinking') {
        last.content = String(last.content ?? '') + delta
      } else {
        trace.push({ type: 'thinking', content: delta })
      }
      return
    }

    if (eventType === 'tool_call') {
      trace.push({
        type: 'tool_call',
        id: payload.tool_call_id,
        name: payload.tool_name,
        input: payload.tool_input,
        result: null,
        isError: false,
      })
      return
    }

    if (eventType === 'tool_result') {
      const nestedToolCallId = typeof payload.tool_call_id === 'string' ? payload.tool_call_id : ''
      for (const item of trace) {
        if (item.type === 'tool_call' && item.id === nestedToolCallId) {
          item.result = payload.result
          item.isError = Boolean(payload.is_error)
          return
        }
      }
      return
    }

    if (eventType === 'error') {
      const message = typeof payload.message === 'string' ? payload.message : t('store.unknownError')
      trace.push({ type: 'text', content: `Error: ${message}` })
    }
  }

  function bufferTaskTraceDelta(toolCallId: string, eventType: string, payload: Record<string, unknown>) {
    const buffered = pendingTaskTraceByToolCallId.get(toolCallId) ?? []
    buffered.push({ eventType, payload })
    if (buffered.length > MAX_PENDING_TASK_TRACE_EVENTS) {
      buffered.splice(0, buffered.length - MAX_PENDING_TASK_TRACE_EVENTS)
    }
    pendingTaskTraceByToolCallId.set(toolCallId, buffered)
  }

  function replayBufferedTaskTrace(taskBlock: ToolCallBlock) {
    const buffered = pendingTaskTraceByToolCallId.get(taskBlock.id)
    if (!buffered || buffered.length === 0) return
    pendingTaskTraceByToolCallId.delete(taskBlock.id)
    for (const item of buffered) {
      applyTaskTraceDelta(taskBlock, item.eventType, item.payload)
    }
  }

  function connectWs() {
    if (ws) ws.disconnect()
    pendingTaskTraceByToolCallId.clear()
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
    ws = new WebSocketManager(`${protocol}//${location.host}/api/ws`, refreshSession)

    ws.on('ws_connected', () => {
      wsConnected.value = true
      if (currentConversationId.value) {
        ws!.send({ type: 'join_conversation', conversation_id: currentConversationId.value })
      }
    })
    ws.on('ws_disconnected', () => { wsConnected.value = false })
    ws.on('auth_failed', () => { disconnectWs(); window.location.href = '/login' })

    ws.on('message_saved', (msg: WsMessage) => {
      const pending = messages.value.find(m => m.id.startsWith('pending-'))
      if (pending) pending.id = msg.message_id as string
    })

    ws.on('assistant_delta', (msg: WsMessage) => {
      if (isWaiting.value) isWaiting.value = false
      if (!isStreaming.value) isStreaming.value = true
      const last = streamingBlocks.value[streamingBlocks.value.length - 1]
      const delta = (msg.delta as string) || ''
      if (last && last.type === 'text') {
        last.content += delta
      } else {
        streamingBlocks.value.push({ type: 'text', content: delta })
      }
    })

    ws.on('thinking_delta', (msg: WsMessage) => {
      if (isWaiting.value) isWaiting.value = false
      if (!isStreaming.value) isStreaming.value = true
      const delta = (msg.delta as string) || ''
      const last = streamingBlocks.value[streamingBlocks.value.length - 1]
      if (last && last.type === 'thinking') {
        last.content += delta
      } else {
        streamingBlocks.value.push({ type: 'thinking', content: delta })
      }
    })

    ws.on('tool_call', (msg: WsMessage) => {
      if (isWaiting.value) isWaiting.value = false
      const block: ToolCallBlock = {
        type: 'tool_call',
        id: msg.tool_call_id as string,
        name: msg.tool_name as string,
        input: msg.tool_input as Record<string, unknown> | undefined,
        result: undefined,
        isError: false,
        isLoading: true,
      }
      streamingBlocks.value.push(block)
      if (block.name === 'task') {
        replayBufferedTaskTrace(block)
      }
    })

    ws.on('tool_result', (msg: WsMessage) => {
      const tc = streamingBlocks.value.find(
        (b): b is ContentBlock & { type: 'tool_call' } =>
          b.type === 'tool_call' && b.id === (msg.tool_call_id as string)
      )
      if (tc) {
        tc.result = normalizeToolResult(msg.result)
        tc.isError = (msg.is_error as boolean) || false
        tc.isLoading = false
        if (tc.isError) {
          const resultText = typeof tc.result === 'string'
            ? tc.result
            : tc.result?.text || ''
          ElMessage.error({
            message: t('store.toolFailed', { tool: tc.name, message: resultText }),
            duration: 5000,
            showClose: true,
          })
        }
      }
    })

    ws.on('task_trace_delta', (msg: WsMessage) => {
      const taskToolCallId = typeof msg.tool_call_id === 'string' ? msg.tool_call_id : ''
      if (!taskToolCallId) return

      const eventType = typeof msg.event_type === 'string' ? msg.event_type : ''
      const payload = typeof msg.payload === 'object' && msg.payload !== null
        ? msg.payload as Record<string, unknown>
        : {}

      const taskBlock = streamingBlocks.value.find(
        (b): b is ToolCallBlock =>
          b.type === 'tool_call' && b.id === taskToolCallId && b.name === 'task'
      )
      if (!taskBlock) {
        bufferTaskTraceDelta(taskToolCallId, eventType, payload)
        return
      }
      applyTaskTraceDelta(taskBlock, eventType, payload)
    })

    ws.on('complete', (msg: WsMessage) => {
      // Prefer backend tool_calls (matches DB) over local streamingBlocks
      let toolCallsJson: string | null = null
      if (msg.tool_calls != null) {
        const blocks = Array.isArray(msg.tool_calls) ? msg.tool_calls : null
        if (blocks && blocks.length > 0) {
          toolCallsJson = JSON.stringify(blocks)
        }
      }
      if (!toolCallsJson) {
        const hasBlocks = streamingBlocks.value.some(
          b => b.type === 'tool_call' || b.type === 'thinking'
        )
        if (hasBlocks) {
          toolCallsJson = JSON.stringify(streamingBlocks.value)
        }
      }

      messages.value.push({
        id: (msg.message_id as string) || generateUUID(),
        role: 'assistant',
        content: (msg.content as string) || streamingContent.value,
        tool_calls: toolCallsJson,
        tool_call_id: null,
        token_count: null,
        created_at: new Date().toISOString(),
      })
      streamingBlocks.value = []
      pendingTaskTraceByToolCallId.clear()
      isStreaming.value = false
      isWaiting.value = false
    })

    ws.on('error', (msg: WsMessage) => {
      console.error('WS error:', msg.message)
      const errorMessage = typeof msg.message === 'string' && msg.message
        ? msg.message
        : t('store.unknownError')
      const hasContent = streamingBlocks.value.length > 0
      if (hasContent) {
        const hasBlocks = streamingBlocks.value.some(b => b.type === 'tool_call' || b.type === 'thinking')
        messages.value.push({
          id: generateUUID(),
          role: 'assistant',
          content: streamingContent.value || t('store.errorMessage', { message: errorMessage }),
          tool_calls: hasBlocks
            ? JSON.stringify(streamingBlocks.value) : null,
          tool_call_id: null,
          token_count: null,
          created_at: new Date().toISOString(),
        })
      }
      streamingBlocks.value = []
      pendingTaskTraceByToolCallId.clear()
      isStreaming.value = false
      isWaiting.value = false
    })

    ws.on('container_status', (msg: WsMessage) => {
      console.log('Container status:', msg.status, msg.message)
      if (msg.status === 'disconnected' && (isStreaming.value || isWaiting.value)) {
        const hasContent = streamingBlocks.value.length > 0
        if (hasContent) {
          const hasBlocks = streamingBlocks.value.some(
            b => b.type === 'tool_call' || b.type === 'thinking'
          )
          messages.value.push({
            id: generateUUID(),
            role: 'assistant',
            content: streamingContent.value || t('store.containerDisconnected'),
            tool_calls: hasBlocks ? JSON.stringify(streamingBlocks.value) : null,
            tool_call_id: null, token_count: null,
            created_at: new Date().toISOString(),
          })
        }
        streamingBlocks.value = []
        pendingTaskTraceByToolCallId.clear()
        isStreaming.value = false
        isWaiting.value = false
        ElMessage.warning({ message: t('store.containerDisconnectedWarning'), duration: 5000, showClose: true })
      }
    })

    ws.on('messages_truncated', (msg: WsMessage) => {
      handleMessagesTruncated(
        msg.after_message_id as string,
        msg.updated_content as string | undefined,
      )
    })

    ws.connect()
  }

  function disconnectWs() {
    ws?.disconnect()
    ws = null
    wsConnected.value = false
  }

  async function loadConversations() {
    conversations.value = await convApi.listConversations()
  }

  async function createConversation(
    title?: string,
    systemPromptOverride?: string,
    provider?: string,
    modelName?: string,
    imageProvider?: string,
    imageModel?: string,
    subagentProvider?: string,
    subagentModel?: string,
    thinkingBudget?: number | null,
    subagentThinkingBudget?: number | null,
  ) {
    const conv = await convApi.createConversation(
      title,
      systemPromptOverride,
      provider,
      modelName,
      imageProvider,
      imageModel,
      subagentProvider,
      subagentModel,
      thinkingBudget,
      subagentThinkingBudget,
    )
    conversations.value.unshift(conv)
    return conv
  }

  async function selectConversation(id: string) {
    const requestId = ++lastSelectRequestId
    currentConversationId.value = id
    ws?.send({ type: 'join_conversation', conversation_id: id })
    const resp = await convApi.listMessages(id)
    if (requestId !== lastSelectRequestId || currentConversationId.value !== id) {
      return
    }
    messages.value = resp.messages
    totalMessages.value = resp.total
  }

  async function deleteConversation(id: string) {
    await convApi.deleteConversation(id)
    conversations.value = conversations.value.filter(c => c.id !== id)
    if (currentConversationId.value === id) {
      currentConversationId.value = null
      messages.value = []
    }
  }

  async function updateConversation(id: string, updates: Partial<Conversation>) {
    const updated = await convApi.updateConversation(id, updates)
    const idx = conversations.value.findIndex(c => c.id === id)
    if (idx >= 0) conversations.value[idx] = updated
  }

  function sendMessage(content: string, attachments?: string[]) {
    if (!currentConversationId.value) return
    messages.value.push({
      id: `pending-${generateUUID()}`,
      role: 'user',
      content,
      tool_calls: null,
      tool_call_id: null,
      token_count: null,
      created_at: new Date().toISOString(),
    })
    isWaiting.value = true
    const payload: WsMessage = { type: 'user_message', content }
    if (attachments && attachments.length > 0) {
      payload.attachments = attachments
    }
    const sent = ws?.send(payload) ?? false
    if (!sent) {
      messages.value.pop()
      isWaiting.value = false
      triggerSendFailed()
    }
  }

  function addMessage(msg: Message) {
    messages.value.push(msg)
  }

  function appendStreamDelta(delta: string) {
    const last = streamingBlocks.value[streamingBlocks.value.length - 1]
    if (last && last.type === 'text') {
      last.content += delta
    } else {
      streamingBlocks.value.push({ type: 'text', content: delta })
    }
  }

  function clearStream() {
    streamingBlocks.value = []
    pendingTaskTraceByToolCallId.clear()
    isStreaming.value = false
  }

  function editMessage(messageId: string, newContent: string) {
    if (!currentConversationId.value) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'user') return

    // Save state for rollback
    const prevContent = messages.value[idx].content
    const prevMessages = messages.value.slice()

    // Optimistic update: modify content + truncate subsequent messages
    messages.value[idx].content = newContent
    messages.value = messages.value.slice(0, idx + 1)
    isWaiting.value = true
    isStreaming.value = true

    const sent = ws?.send({ type: 'edit_message', message_id: messageId, content: newContent }) ?? false
    if (!sent) {
      messages.value = prevMessages
      messages.value[idx].content = prevContent
      isWaiting.value = false
      isStreaming.value = false
      triggerSendFailed()
    }
  }

  function regenerateMessage(messageId: string) {
    if (!currentConversationId.value) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'assistant') return

    // Save state for rollback
    const prevMessages = messages.value.slice()

    // Optimistic update: remove assistant message and everything after it
    messages.value = messages.value.slice(0, idx)
    isWaiting.value = true
    isStreaming.value = true

    const sent = ws?.send({ type: 'regenerate', message_id: messageId }) ?? false
    if (!sent) {
      messages.value = prevMessages
      isWaiting.value = false
      isStreaming.value = false
      triggerSendFailed()
    }
  }

  function cancelGeneration() {
    if (!isStreaming.value && !isWaiting.value) return
    ws?.send({ type: 'cancel' })
  }

  function handleMessagesTruncated(afterMessageId: string, updatedContent?: string) {
    const idx = messages.value.findIndex(m => m.id === afterMessageId)
    if (idx < 0) return
    if (updatedContent !== undefined) {
      messages.value[idx].content = updatedContent
    }
    messages.value = messages.value.slice(0, idx + 1)
  }

  let sendFailedTimer: ReturnType<typeof setTimeout> | null = null
  function triggerSendFailed() {
    sendFailed.value = true
    if (sendFailedTimer) clearTimeout(sendFailedTimer)
    sendFailedTimer = setTimeout(() => { sendFailed.value = false }, 3000)
  }

  return {
    conversations, currentConversationId, messages, streamingContent, streamingBlocks, isStreaming, isWaiting, totalMessages, wsConnected, sendFailed,
    connectWs, disconnectWs, loadConversations, createConversation, selectConversation, deleteConversation,
    updateConversation, sendMessage, addMessage, appendStreamDelta, clearStream,
    editMessage, regenerateMessage, handleMessagesTruncated, cancelGeneration,
  }
})
