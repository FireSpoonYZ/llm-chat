import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Conversation, Message, WsMessage } from '../types'
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
  const streamingContent = ref('')
  const isStreaming = ref(false)
  const totalMessages = ref(0)
  const wsConnected = ref(false)

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
      if (!isStreaming.value) isStreaming.value = true
      streamingContent.value += (msg.delta as string) || ''
    })

    ws.on('complete', (msg: WsMessage) => {
      messages.value.push({
        id: (msg.message_id as string) || generateId(),
        role: 'assistant',
        content: (msg.content as string) || streamingContent.value,
        tool_calls: null,
        tool_call_id: null,
        token_count: null,
        created_at: new Date().toISOString(),
      })
      streamingContent.value = ''
      isStreaming.value = false
    })

    ws.on('error', (msg: WsMessage) => {
      console.error('WS error:', msg.message)
      isStreaming.value = false
      streamingContent.value = ''
    })

    ws.on('container_status', (msg: WsMessage) => {
      console.log('Container status:', msg.status, msg.message)
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

  async function createConversation(title?: string) {
    const conv = await convApi.createConversation(title)
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
    ws?.send({ type: 'user_message', content })
  }

  function addMessage(msg: Message) {
    messages.value.push(msg)
  }

  function appendStreamDelta(delta: string) {
    streamingContent.value += delta
  }

  function clearStream() {
    streamingContent.value = ''
    isStreaming.value = false
  }

  return {
    conversations, currentConversationId, messages, streamingContent, isStreaming, totalMessages, wsConnected,
    connectWs, disconnectWs, loadConversations, createConversation, selectConversation, deleteConversation,
    updateConversation, sendMessage, addMessage, appendStreamDelta, clearStream,
  }
})
