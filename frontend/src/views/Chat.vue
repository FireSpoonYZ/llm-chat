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
      <template v-if="chatStore.currentConversationId && currentConversation">
        <div style="padding: 8px 16px; border-bottom: 1px solid #e4e7ed; display: flex; align-items: center; gap: 8px; flex-shrink: 0">
          <span style="color: #606266; font-size: 13px">Model:</span>
          <el-cascader
            :model-value="cascaderValue"
            :options="cascaderOptions"
            :props="{ expandTrigger: 'hover' }"
            placeholder="Select provider / model"
            clearable
            size="small"
            style="width: 360px"
            @change="handleCascaderChange"
          />
        </div>
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
import { onMounted, onUnmounted, computed } from 'vue'
import { useAuthStore } from '../stores/auth'
import { useChatStore } from '../stores/chat'
import { useSettingsStore } from '../stores/settings'
import ConversationList from '../components/ConversationList.vue'
import ChatMessage from '../components/ChatMessage.vue'
import ChatInput from '../components/ChatInput.vue'

const auth = useAuthStore()
const chatStore = useChatStore()
const settingsStore = useSettingsStore()

onMounted(async () => {
  await chatStore.loadConversations()
  await settingsStore.loadProviders()
  if (auth.accessToken) {
    chatStore.connectWs(auth.accessToken)
  }
})

onUnmounted(() => {
  chatStore.disconnectWs()
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
  chatStore.sendMessage(content)
}

const currentConversation = computed(() =>
  chatStore.conversations.find(c => c.id === chatStore.currentConversationId)
)

const cascaderOptions = computed(() => {
  return settingsStore.providers.map(p => ({
    value: p.provider,
    label: p.name || p.provider,
    children: p.models.map(m => ({
      value: m,
      label: m,
    })),
  }))
})

const cascaderValue = computed(() => {
  const conv = currentConversation.value
  if (conv?.provider && conv?.model_name) {
    return [conv.provider, conv.model_name]
  }
  return []
})

async function handleCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      provider: val[0],
      model_name: val[1],
    })
  } else {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      provider: '',
      model_name: '',
    })
  }
}
</script>