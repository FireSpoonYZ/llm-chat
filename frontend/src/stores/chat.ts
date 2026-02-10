import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Conversation, Message } from '../types'
import * as convApi from '../api/conversations'

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Conversation[]>([])
  const currentConversationId = ref<string | null>(null)
  const messages = ref<Message[]>([])
  const streamingContent = ref('')
  const isStreaming = ref(false)
  const totalMessages = ref(0)

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
    conversations, currentConversationId, messages, streamingContent, isStreaming, totalMessages,
    loadConversations, createConversation, selectConversation, deleteConversation, updateConversation,
    addMessage, appendStreamDelta, clearStream,
  }
})
