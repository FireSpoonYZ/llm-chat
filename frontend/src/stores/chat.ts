import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { ElMessage } from 'element-plus'
import type {
  ActiveQuestionnaire,
  Conversation,
  Message,
  MessagePart,
  ContentBlock,
  WsMessage,
  ToolResult,
  QuestionAnswer,
} from '../types'
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
  type BufferedTraceDelta = {
    eventType: string
    payload: Record<string, unknown>
  }
  type PendingMutationKind = 'edit' | 'regenerate'
  type PendingMutation = {
    id: string
    kind: PendingMutationKind
    expectedAfterMessageId: string
    timeoutId: ReturnType<typeof setTimeout>
  }

  const MAX_PENDING_SUBAGENT_TRACE_EVENTS = 256
  const MUTATION_CONFIRM_TIMEOUT_MS = 8000
  const EXPECTED_RECONNECT_WINDOW_MS = 15000
  const EXPLORE_TOOL_NAMES = new Set(['explore'])
  const LEGACY_TASK_TOOL_NAMES = new Set(['task'])
  const MUTATION_ERROR_CODES = new Set(['no_conversation', 'invalid_message', 'edit_failed', 'regenerate_failed'])

  const conversations = ref<Conversation[]>([])
  const currentConversationId = ref<string | null>(null)
  const messages = ref<Message[]>([])
  const streamingBlocks = ref<ContentBlock[]>([])
  const isStreaming = ref(false)
  const isWaiting = ref(false)
  const totalMessages = ref(0)
  const wsConnected = ref(false)
  const sendFailed = ref(false)
  const activeQuestionnaire = ref<ActiveQuestionnaire | null>(null)
  const questionnaireSubmitting = ref(false)
  const QUESTIONNAIRE_RECOVERABLE_ERROR_CODES = new Set([
    'invalid_question_answer',
    'question_not_pending',
    'container_not_connected',
  ])

  // Backward-compatible computed: concatenate all text blocks
  const streamingContent = computed(() =>
    streamingBlocks.value
      .filter((b): b is ContentBlock & { type: 'text' } => b.type === 'text')
      .map(b => b.content)
      .join('')
  )

  let ws: WebSocketManager | null = null
  let lastSelectRequestId = 0
  let expectedReconnectUntil = 0
  let expectedReconnectReason = ''
  const pendingSubagentTraceByToolCallId = new Map<string, BufferedTraceDelta[]>()
  const pendingLegacyTaskTraceByToolCallId = new Map<string, BufferedTraceDelta[]>()
  const pendingMutation = ref<PendingMutation | null>(null)

  function isExploreToolName(toolName: string): boolean {
    return EXPLORE_TOOL_NAMES.has(toolName)
  }

  function isLegacyTaskToolName(toolName: string): boolean {
    return LEGACY_TASK_TOOL_NAMES.has(toolName)
  }

  function ensureSubagentTrace(subagentBlock: ToolCallBlock): Record<string, unknown>[] {
    const subagentResult = normalizeToolResult(subagentBlock.result)
    if (subagentResult.kind === 'text') {
      subagentResult.kind = subagentBlock.name
    }
    const data = {
      ...(subagentResult.data ?? {}),
    } as Record<string, unknown>
    const trace = Array.isArray(data.trace)
      ? data.trace.filter((x): x is Record<string, unknown> => typeof x === 'object' && x !== null)
      : []
    data.trace = trace
    subagentResult.data = data
    subagentBlock.result = subagentResult
    return trace
  }

  function applySubagentTraceDelta(subagentBlock: ToolCallBlock, eventType: string, payload: Record<string, unknown>) {
    const trace = ensureSubagentTrace(subagentBlock)

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

  function bufferTraceDelta(
    traceBuffer: Map<string, BufferedTraceDelta[]>,
    toolCallId: string,
    eventType: string,
    payload: Record<string, unknown>,
  ) {
    const buffered = traceBuffer.get(toolCallId) ?? []
    buffered.push({ eventType, payload })
    if (buffered.length > MAX_PENDING_SUBAGENT_TRACE_EVENTS) {
      buffered.splice(0, buffered.length - MAX_PENDING_SUBAGENT_TRACE_EVENTS)
    }
    traceBuffer.set(toolCallId, buffered)
  }

  function replayBufferedTrace(
    traceBuffer: Map<string, BufferedTraceDelta[]>,
    subagentBlock: ToolCallBlock,
  ) {
    const buffered = traceBuffer.get(subagentBlock.id)
    if (!buffered || buffered.length === 0) return
    traceBuffer.delete(subagentBlock.id)
    for (const item of buffered) {
      applySubagentTraceDelta(subagentBlock, item.eventType, item.payload)
    }
  }

  function clearPendingMutation() {
    if (pendingMutation.value?.timeoutId) {
      clearTimeout(pendingMutation.value.timeoutId)
    }
    pendingMutation.value = null
  }

  function parseMessageParts(raw: unknown): MessagePart[] | undefined {
    if (!Array.isArray(raw)) return undefined
    const parsed: MessagePart[] = []
    for (const item of raw) {
      if (typeof item !== 'object' || item === null) continue
      const obj = item as Record<string, unknown>
      if (typeof obj.type !== 'string') continue
      parsed.push({
        type: obj.type,
        text: typeof obj.text === 'string' || obj.text === null ? obj.text : null,
        json_payload: obj.json_payload ?? null,
        tool_call_id: typeof obj.tool_call_id === 'string' || obj.tool_call_id === null
          ? obj.tool_call_id
          : null,
        seq: typeof obj.seq === 'number' ? obj.seq : null,
      })
    }
    return parsed
  }

  async function reloadCurrentConversationMessages() {
    const convId = currentConversationId.value
    if (!convId) return
    try {
      const resp = await convApi.listMessages(convId)
      if (currentConversationId.value !== convId) return
      messages.value = resp.messages
      totalMessages.value = resp.total
    } catch (error) {
      console.error('Failed to reload messages after mutation failure:', error)
    }
  }

  function onPendingMutationFailed() {
    const pending = pendingMutation.value
    if (!pending) return
    clearPendingMutation()
    if (!isStreaming.value) {
      isWaiting.value = false
    }
    ElMessage.error(
      pending.kind === 'edit'
        ? t('chat.messages.editNotApplied')
        : t('chat.messages.regenerateNotApplied'),
    )
    void reloadCurrentConversationMessages()
  }

  function startPendingMutation(
    kind: PendingMutationKind,
    expectedAfterMessageId: string,
  ) {
    clearPendingMutation()
    const mutationId = generateUUID()
    const timeoutId = setTimeout(() => {
      if (!pendingMutation.value || pendingMutation.value.id !== mutationId) return
      clearPendingMutation()
      if (!isStreaming.value) {
        isWaiting.value = false
      }
      ElMessage.error(t('chat.messages.operationTimeout'))
      void reloadCurrentConversationMessages()
    }, MUTATION_CONFIRM_TIMEOUT_MS)

    pendingMutation.value = {
      id: mutationId,
      kind,
      expectedAfterMessageId,
      timeoutId,
    }
  }

  function connectWs() {
    if (ws) ws.disconnect()
    expectedReconnectUntil = 0
    expectedReconnectReason = ''
    pendingSubagentTraceByToolCallId.clear()
    pendingLegacyTaskTraceByToolCallId.clear()
    activeQuestionnaire.value = null
    questionnaireSubmitting.value = false
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
      if (isExploreToolName(block.name)) {
        replayBufferedTrace(pendingSubagentTraceByToolCallId, block)
      } else if (isLegacyTaskToolName(block.name)) {
        replayBufferedTrace(pendingLegacyTaskTraceByToolCallId, block)
      }
    })

    ws.on('tool_result', (msg: WsMessage) => {
      const normalized = normalizeToolResult(msg.result)
      const tc = streamingBlocks.value.find(
        (b): b is ContentBlock & { type: 'tool_call' } =>
          b.type === 'tool_call' && b.id === (msg.tool_call_id as string)
      )
      if (tc) {
        tc.result = normalized
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
      if (activeQuestionnaire.value && questionnaireSubmitting.value && normalized.kind === 'question') {
        const questionnaireId = typeof normalized.data?.questionnaire_id === 'string'
          ? normalized.data.questionnaire_id
          : ''
        if (questionnaireId === activeQuestionnaire.value.questionnaire_id) {
          activeQuestionnaire.value = null
          questionnaireSubmitting.value = false
          isWaiting.value = false
        }
      }
    })

    ws.on('question', (msg: WsMessage) => {
      const questionnaireId = typeof msg.questionnaire_id === 'string'
        ? msg.questionnaire_id
        : ''
      const rawQuestions = Array.isArray(msg.questions) ? msg.questions : []
      if (!questionnaireId || rawQuestions.length === 0) return

      activeQuestionnaire.value = {
        questionnaire_id: questionnaireId,
        title: typeof msg.title === 'string' ? msg.title : null,
        questions: rawQuestions
          .filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
          .map((item, index) => ({
            id: typeof item.id === 'string' && item.id ? item.id : `q${index + 1}`,
            header: typeof item.header === 'string' ? item.header : null,
            question: typeof item.question === 'string' ? item.question : '',
            options: Array.isArray(item.options) ? item.options.map(x => String(x)) : [],
            placeholder: typeof item.placeholder === 'string' ? item.placeholder : null,
            multiple: Boolean(item.multiple),
              required: typeof item.required === 'boolean' ? item.required : true,
          })),
      }
      questionnaireSubmitting.value = false
    })

    const handleSubagentTraceDelta = (msg: WsMessage) => {
      const parentToolCallId = typeof msg.tool_call_id === 'string' ? msg.tool_call_id : ''
      if (!parentToolCallId) return

      const eventType = typeof msg.event_type === 'string' ? msg.event_type : ''
      const payload = typeof msg.payload === 'object' && msg.payload !== null
        ? msg.payload as Record<string, unknown>
        : {}

      const subagentBlock = streamingBlocks.value.find(
        (b): b is ToolCallBlock =>
          b.type === 'tool_call' && b.id === parentToolCallId && isExploreToolName(b.name)
      )
      if (!subagentBlock) {
        bufferTraceDelta(
          pendingSubagentTraceByToolCallId,
          parentToolCallId,
          eventType,
          payload,
        )
        return
      }
      applySubagentTraceDelta(subagentBlock, eventType, payload)
    }
    const handleTaskTraceDelta = (msg: WsMessage) => {
      const parentToolCallId = typeof msg.tool_call_id === 'string' ? msg.tool_call_id : ''
      if (!parentToolCallId) return

      const eventType = typeof msg.event_type === 'string' ? msg.event_type : ''
      const payload = typeof msg.payload === 'object' && msg.payload !== null
        ? msg.payload as Record<string, unknown>
        : {}

      const taskBlock = streamingBlocks.value.find(
        (b): b is ToolCallBlock =>
          b.type === 'tool_call' && b.id === parentToolCallId && isLegacyTaskToolName(b.name)
      )
      if (!taskBlock) {
        bufferTraceDelta(
          pendingLegacyTaskTraceByToolCallId,
          parentToolCallId,
          eventType,
          payload,
        )
        return
      }
      applySubagentTraceDelta(taskBlock, eventType, payload)
    }
    ws.on('subagent_trace_delta', handleSubagentTraceDelta)
    ws.on('task_trace_delta', handleTaskTraceDelta)

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
      pendingSubagentTraceByToolCallId.clear()
      pendingLegacyTaskTraceByToolCallId.clear()
      isStreaming.value = false
      isWaiting.value = false
      activeQuestionnaire.value = null
      questionnaireSubmitting.value = false
    })

    ws.on('error', (msg: WsMessage) => {
      console.error('WS error:', msg.message)
      const errorCode = typeof msg.code === 'string' ? msg.code : ''
      if (
        activeQuestionnaire.value &&
        questionnaireSubmitting.value &&
        QUESTIONNAIRE_RECOVERABLE_ERROR_CODES.has(errorCode)
      ) {
        questionnaireSubmitting.value = false
        isWaiting.value = false
        isStreaming.value = false
        return
      }
      if (pendingMutation.value && MUTATION_ERROR_CODES.has(errorCode)) {
        onPendingMutationFailed()
        return
      }
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
      pendingSubagentTraceByToolCallId.clear()
      pendingLegacyTaskTraceByToolCallId.clear()
      isStreaming.value = false
      isWaiting.value = false
      activeQuestionnaire.value = null
      questionnaireSubmitting.value = false
    })

    ws.on('container_status', (msg: WsMessage) => {
      const status = typeof msg.status === 'string' ? msg.status : ''
      const reason = typeof msg.reason === 'string' ? msg.reason : ''
      if (status === 'starting' || status === 'restarting') {
        expectedReconnectUntil = Date.now() + EXPECTED_RECONNECT_WINDOW_MS
        expectedReconnectReason = reason || 'expected_reconnect'
        return
      }
      if (status === 'connected') {
        expectedReconnectUntil = 0
        expectedReconnectReason = ''
        return
      }
      const expectedDisconnect = (
        status === 'disconnected'
        && expectedReconnectReason.length > 0
        && Date.now() <= expectedReconnectUntil
      )
      if (expectedDisconnect) {
        return
      }
      if (status === 'disconnected' && (isStreaming.value || isWaiting.value)) {
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
        pendingSubagentTraceByToolCallId.clear()
        pendingLegacyTaskTraceByToolCallId.clear()
        isStreaming.value = false
        isWaiting.value = false
        activeQuestionnaire.value = null
        questionnaireSubmitting.value = false
        ElMessage.warning({ message: t('store.containerDisconnectedWarning'), duration: 5000, showClose: true })
      }
    })

    ws.on('messages_truncated', (msg: WsMessage) => {
      const afterMessageId = msg.after_message_id as string
      handleMessagesTruncated(
        afterMessageId,
        msg.updated_content as string | undefined,
        parseMessageParts(msg.updated_parts),
      )
      if (
        pendingMutation.value &&
        pendingMutation.value.expectedAfterMessageId === afterMessageId
      ) {
        clearPendingMutation()
      }
    })

    ws.connect()
  }

  function disconnectWs() {
    clearPendingMutation()
    expectedReconnectUntil = 0
    expectedReconnectReason = ''
    ws?.disconnect()
    ws = null
    wsConnected.value = false
    activeQuestionnaire.value = null
    questionnaireSubmitting.value = false
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
    clearPendingMutation()
    const requestId = ++lastSelectRequestId
    currentConversationId.value = id
    activeQuestionnaire.value = null
    questionnaireSubmitting.value = false
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
    if (activeQuestionnaire.value) return
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

  function submitQuestionnaireAnswers(answers: QuestionAnswer[]) {
    const questionnaire = activeQuestionnaire.value
    if (!questionnaire || questionnaireSubmitting.value) return

    const payload: WsMessage = {
      type: 'question_answer',
      questionnaire_id: questionnaire.questionnaire_id,
      answers,
    }
    const sent = ws?.send(payload) ?? false
    if (!sent) {
      triggerSendFailed()
      return
    }
    questionnaireSubmitting.value = true
    isWaiting.value = true
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
    clearPendingMutation()
    streamingBlocks.value = []
    pendingSubagentTraceByToolCallId.clear()
    pendingLegacyTaskTraceByToolCallId.clear()
    isStreaming.value = false
    activeQuestionnaire.value = null
    questionnaireSubmitting.value = false
  }

  function editMessage(messageId: string, newContent: string) {
    if (!currentConversationId.value) return
    if (pendingMutation.value) return
    if (messageId.startsWith('pending-')) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'user') return
    if (newContent === messages.value[idx].content) return

    startPendingMutation('edit', messageId)
    isWaiting.value = true

    const sent = ws?.send({ type: 'edit_message', message_id: messageId, content: newContent }) ?? false
    if (!sent) {
      clearPendingMutation()
      isWaiting.value = false
      triggerSendFailed()
    }
  }

  function regenerateMessage(messageId: string) {
    if (!currentConversationId.value) return
    if (pendingMutation.value) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'assistant') return
    let lastUserMsgId: string | null = null
    for (let i = idx - 1; i >= 0; i -= 1) {
      if (messages.value[i].role === 'user') {
        lastUserMsgId = messages.value[i].id
        break
      }
    }
    if (!lastUserMsgId) return

    startPendingMutation('regenerate', lastUserMsgId)
    isWaiting.value = true

    const sent = ws?.send({ type: 'regenerate', message_id: messageId }) ?? false
    if (!sent) {
      clearPendingMutation()
      isWaiting.value = false
      triggerSendFailed()
    }
  }

  function cancelGeneration() {
    if (!isStreaming.value && !isWaiting.value) return
    ws?.send({ type: 'cancel' })
  }

  function handleMessagesTruncated(
    afterMessageId: string,
    updatedContent?: string,
    updatedParts?: MessagePart[],
  ) {
    const idx = messages.value.findIndex(m => m.id === afterMessageId)
    if (idx < 0) {
      void reloadCurrentConversationMessages()
      return
    }
    if (updatedParts !== undefined) {
      messages.value[idx].parts = updatedParts
      if (updatedContent !== undefined) {
        messages.value[idx].content = updatedContent
      } else {
        const textPart = updatedParts.find(
          p => p.type === 'text' && typeof p.text === 'string' && p.text.length > 0,
        )
        if (textPart?.text) {
          messages.value[idx].content = textPart.text
        }
      }
    } else if (updatedContent !== undefined) {
      messages.value[idx].content = updatedContent
      messages.value[idx].parts = [{
        type: 'text',
        text: updatedContent,
        json_payload: null,
        tool_call_id: null,
        seq: 0,
      }]
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
    conversations, currentConversationId, messages, streamingContent, streamingBlocks, isStreaming, isWaiting, totalMessages, wsConnected, sendFailed, activeQuestionnaire, questionnaireSubmitting,
    connectWs, disconnectWs, loadConversations, createConversation, selectConversation, deleteConversation,
    updateConversation, sendMessage, submitQuestionnaireAnswers, addMessage, appendStreamDelta, clearStream,
    editMessage, regenerateMessage, handleMessagesTruncated, cancelGeneration,
  }
})
