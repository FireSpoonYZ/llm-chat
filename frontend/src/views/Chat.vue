<template>
  <el-container style="height: 100vh">
    <!-- Sidebar -->
    <el-aside width="280px" style="border-right: 1px solid #e4e7ed; background: #fafafa">
      <div style="padding: 12px">
        <el-button type="primary" style="width: 100%" @click="handleNewChat">
          New Chat
        </el-button>
      </div>
      <ConversationList
        :conversations="chatStore.conversations"
        :current-id="chatStore.currentConversationId"
        @select="handleSelectConversation"
        @delete="handleDeleteConversation"
      />
      <div style="padding: 12px; border-top: 1px solid #e4e7ed; position: absolute; bottom: 0; width: 280px; box-sizing: border-box">
        <el-button text @click="$router.push('/settings')">Settings</el-button>
        <el-button text type="danger" @click="auth.logout()">Logout</el-button>
      </div>
    </el-aside>

    <!-- Main chat area -->
    <el-main style="padding: 0; display: flex; flex-direction: column">
      <template v-if="chatStore.currentConversationId">
        <div style="flex: 1; overflow-y: auto; padding: 20px">
          <ChatMessage
            v-for="msg in chatStore.messages"
            :key="msg.id"
            :message="msg"
          />
          <ChatMessage
            v-if="chatStore.isStreaming"
            :message="{ id: 'streaming', role: 'assistant', content: chatStore.streamingContent, tool_calls: null, tool_call_id: null, token_count: null, created_at: '' }"
          />
        </div>
        <ChatInput @send="handleSend" :disabled="chatStore.isStreaming" />
      </template>
      <template v-else>
        <div style="flex: 1; display: flex; align-items: center; justify-content: center; color: #909399">
          <p>Select a conversation or start a new chat</p>
        </div>
      </template>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useAuthStore } from '../stores/auth'
import { useChatStore } from '../stores/chat'
import ConversationList from '../components/ConversationList.vue'
import ChatMessage from '../components/ChatMessage.vue'
import ChatInput from '../components/ChatInput.vue'

const auth = useAuthStore()
const chatStore = useChatStore()

onMounted(async () => {
  await chatStore.loadConversations()
})

async function handleNewChat() {
  const conv = await chatStore.createConversation()
  await chatStore.selectConversation(conv.id)
}

async function handleSelectConversation(id: string) {
  await chatStore.selectConversation(id)
}

async function handleDeleteConversation(id: string) {
  await chatStore.deleteConversation(id)
}

function handleSend(content: string) {
  // Will be wired to WebSocket in Phase 4
  // For now, just add the message locally
  chatStore.addMessage({
    id: crypto.randomUUID(),
    role: 'user',
    content,
    tool_calls: null,
    tool_call_id: null,
    token_count: null,
    created_at: new Date().toISOString(),
  })
}
</script>
