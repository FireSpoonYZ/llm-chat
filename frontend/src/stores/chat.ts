import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Conversation, Message, ContentBlock, WsMessage } from '../types'
import * as convApi from '../api/conversations'
import { WebSocketManager } from '../api/websocket'

function generateId(): string {
  const arr = new Uint8Array(16)
  crypto.getRandomValues(arr)
  return Array.from(arr, b => b.toString(16).padStart(2, '0')).join('')
}

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Conversation[]>([])
  const currentConversationId = ref<string | null>(null)
  const messages = ref<Message[]>([])
  const streamingBlocks = ref<ContentBlock[]>([])
  const isStreaming = ref(false)
  const isWaiting = ref(false)
  const totalMessages = ref(0)
  const wsConnected = ref(false)

  // Backward-compatible computed: concatenate all text blocks
  const streamingContent = computed(() =>
    streamingBlocks.value
      .filter((b): b is ContentBlock & { type: 'text' } => b.type === 'text')
      .map(b => b.content)
      .join('')
  )

  let ws: WebSocketManager | null = null

  function connectWs(token: string) {
    if (ws) ws.disconnect()
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
    ws = new WebSocketManager(`${protocol}//${location.host}/api/ws`)

    ws.on('ws_connected', () => {
      wsConnected.value = true
      if (currentConversationId.value) {
        ws!.send({ type: 'join_conversation', conversation_id: currentConversationId.value })
      }
    })
    ws.on('ws_disconnected', () => { wsConnected.value = false })

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
      streamingBlocks.value.push({
        type: 'tool_call',
        id: msg.tool_call_id as string,
        name: msg.tool_name as string,
        input: msg.tool_input as Record<string, unknown> | undefined,
        result: undefined,
        isError: false,
        isLoading: true,
      })
    })

    ws.on('tool_result', (msg: WsMessage) => {
      const tc = streamingBlocks.value.find(
        (b): b is ContentBlock & { type: 'tool_call' } =>
          b.type === 'tool_call' && b.id === (msg.tool_call_id as string)
      )
      if (tc) {
        tc.result = msg.result as string
        tc.isError = (msg.is_error as boolean) || false
        tc.isLoading = false
      }
    })

    ws.on('complete', (msg: WsMessage) => {
      const hasBlocks = streamingBlocks.value.some(b => b.type === 'tool_call' || b.type === 'thinking')
      messages.value.push({
        id: (msg.message_id as string) || generateId(),
        role: 'assistant',
        content: (msg.content as string) || streamingContent.value,
        tool_calls: hasBlocks
          ? JSON.stringify(streamingBlocks.value) : null,
        tool_call_id: null,
        token_count: null,
        created_at: new Date().toISOString(),
      })
      streamingBlocks.value = []
      isStreaming.value = false
      isWaiting.value = false
    })

    ws.on('error', (msg: WsMessage) => {
      console.error('WS error:', msg.message)
      const hasContent = streamingBlocks.value.length > 0
      if (hasContent) {
        const hasBlocks = streamingBlocks.value.some(b => b.type === 'tool_call' || b.type === 'thinking')
        messages.value.push({
          id: generateId(),
          role: 'assistant',
          content: streamingContent.value || `[Error: ${msg.message || 'Unknown error'}]`,
          tool_calls: hasBlocks
            ? JSON.stringify(streamingBlocks.value) : null,
          tool_call_id: null,
          token_count: null,
          created_at: new Date().toISOString(),
        })
      }
      streamingBlocks.value = []
      isStreaming.value = false
      isWaiting.value = false
    })

    ws.on('container_status', (msg: WsMessage) => {
      console.log('Container status:', msg.status, msg.message)
    })

    ws.on('messages_truncated', (msg: WsMessage) => {
      handleMessagesTruncated(
        msg.after_message_id as string,
        msg.updated_content as string | undefined,
      )
    })

    ws.connect(token)
  }

  function disconnectWs() {
    ws?.disconnect()
    ws = null
    wsConnected.value = false
  }

  async function loadConversations() {
    conversations.value = await convApi.listConversations()
  }

  async function createConversation(title?: string, systemPromptOverride?: string, provider?: string, modelName?: string) {
    const conv = await convApi.createConversation(title, systemPromptOverride, provider, modelName)
    conversations.value.unshift(conv)
    return conv
  }

  async function selectConversation(id: string) {
    currentConversationId.value = id
    const resp = await convApi.listMessages(id)
    messages.value = resp.messages
    totalMessages.value = resp.total
    ws?.send({ type: 'join_conversation', conversation_id: id })
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

  function sendMessage(content: string) {
    if (!currentConversationId.value) return
    messages.value.push({
      id: `pending-${generateId()}`,
      role: 'user',
      content,
      tool_calls: null,
      tool_call_id: null,
      token_count: null,
      created_at: new Date().toISOString(),
    })
    isWaiting.value = true
    ws?.send({ type: 'user_message', content })
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
    isStreaming.value = false
  }

  function editMessage(messageId: string, newContent: string) {
    if (!currentConversationId.value) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'user') return

    // Optimistic update: modify content + truncate subsequent messages
    messages.value[idx].content = newContent
    messages.value = messages.value.slice(0, idx + 1)
    isWaiting.value = true
    isStreaming.value = true

    ws?.send({ type: 'edit_message', message_id: messageId, content: newContent })
  }

  function regenerateMessage(messageId: string) {
    if (!currentConversationId.value) return
    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx < 0) return
    if (messages.value[idx].role !== 'assistant') return

    // Optimistic update: remove assistant message and everything after it
    messages.value = messages.value.slice(0, idx)
    isWaiting.value = true
    isStreaming.value = true

    ws?.send({ type: 'regenerate', message_id: messageId })
  }

  function handleMessagesTruncated(afterMessageId: string, updatedContent?: string) {
    const idx = messages.value.findIndex(m => m.id === afterMessageId)
    if (idx < 0) return
    if (updatedContent !== undefined) {
      messages.value[idx].content = updatedContent
    }
    messages.value = messages.value.slice(0, idx + 1)
  }

  return {
    conversations, currentConversationId, messages, streamingContent, streamingBlocks, isStreaming, isWaiting, totalMessages, wsConnected,
    connectWs, disconnectWs, loadConversations, createConversation, selectConversation, deleteConversation,
    updateConversation, sendMessage, addMessage, appendStreamDelta, clearStream,
    editMessage, regenerateMessage, handleMessagesTruncated,
  }
})
