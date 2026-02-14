<template>
  <div class="chat-layout">
    <!-- Sidebar -->
    <aside class="chat-sidebar" :class="{ collapsed: sidebarCollapsed }">
      <div class="sidebar-header">
        <el-button class="new-chat-btn" @click="showNewChatDialog = true">
          <el-icon><Plus /></el-icon>
          <span v-if="!sidebarCollapsed">New Chat</span>
        </el-button>
      </div>
      <ConversationList
        :conversations="chatStore.conversations"
        :current-id="chatStore.currentConversationId"
        @select="handleSelectConversation"
        @delete="handleDeleteConversation"
      />
      <div class="sidebar-footer">
        <el-button class="sidebar-link" text @click="$router.push('/settings')">
          <el-icon><Setting /></el-icon>
          <span>Settings</span>
        </el-button>
        <el-button class="sidebar-link logout" text @click="auth.logout()">
          <el-icon><SwitchButton /></el-icon>
          <span>Logout</span>
        </el-button>
      </div>
    </aside>

    <!-- Main chat area -->
    <main class="chat-main">
      <el-button class="sidebar-toggle" text @click="sidebarCollapsed = !sidebarCollapsed">
        <el-icon :size="18">
          <Expand v-if="sidebarCollapsed" />
          <Fold v-else />
        </el-icon>
      </el-button>

      <template v-if="chatStore.currentConversationId && currentConversation">
        <div class="chat-toolbar">
          <div class="toolbar-inner">
            <span class="toolbar-label">Model</span>
            <el-cascader
              :model-value="cascaderValue"
              :options="cascaderOptions"
              :props="{ expandTrigger: 'hover' }"
              placeholder="Select provider / model"
              clearable
              size="small"
              class="model-cascader"
              @change="handleCascaderChange"
            />
            <el-button class="toolbar-btn" text @click="showPromptDrawer = true">System Prompt</el-button>
            <el-button class="toolbar-btn" text @click="showFilesDrawer = true">Files</el-button>
          </div>
        </div>
        <div class="chat-messages">
          <div class="messages-inner">
            <ChatMessage
              v-for="msg in chatStore.messages"
              :key="msg.id"
              :message="msg"
              :is-streaming="chatStore.isStreaming"
              @edit="handleEditMessage"
              @regenerate="handleRegenerateMessage"
            />
            <div v-if="chatStore.isWaiting && !chatStore.isStreaming" v-loading="true" class="waiting-indicator" element-loading-background="transparent" />
            <ChatMessage
              v-if="chatStore.isStreaming"
              :message="{ id: 'streaming', role: 'assistant', content: chatStore.streamingContent, tool_calls: null, tool_call_id: null, token_count: null, created_at: '' }"
              :is-streaming="true"
              :streaming-blocks="chatStore.streamingBlocks"
            />
          </div>
        </div>
        <ChatInput @send="handleSend" :disabled="chatStore.isStreaming" :deep-thinking="deepThinking" @update:deep-thinking="toggleDeepThinking" />
        <div v-if="!chatStore.wsConnected" class="ws-status-bar ws-disconnected">
          <span class="ws-dot pulse"></span>
          <span>Connection lost, reconnecting...</span>
        </div>
        <div v-if="chatStore.sendFailed" class="ws-status-bar ws-send-failed">
          <span>Failed to send message. Please check your connection.</span>
        </div>
      </template>
      <template v-else>
        <div class="empty-state">
          <p>Select a conversation or start a new chat</p>
        </div>
      </template>
    </main>

    <!-- New Chat Dialog -->
    <el-dialog v-model="showNewChatDialog" title="New Chat" width="520px">
      <el-form label-position="top">
        <el-form-item label="System Prompt Preset">
          <el-select v-model="newChatPresetId" placeholder="Select a preset" style="width: 100%" @change="handlePresetSelect">
            <el-option
              v-for="preset in presets"
              :key="preset.id"
              :label="preset.name"
              :value="preset.id"
            >
              <span>{{ preset.name }}</span>
              <span style="color: var(--text-secondary); font-size: 12px; margin-left: 8px">{{ preset.description }}</span>
            </el-option>
            <el-option label="Custom" value="__custom__" />
          </el-select>
        </el-form-item>
        <el-form-item label="System Prompt">
          <el-input
            v-model="newChatPrompt"
            type="textarea"
            :rows="6"
            placeholder="Enter a custom system prompt or select a preset above"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showNewChatDialog = false">Cancel</el-button>
        <el-button type="primary" @click="handleCreateChat">Create</el-button>
      </template>
    </el-dialog>

    <!-- System Prompt Drawer -->
    <el-drawer v-model="showPromptDrawer" title="System Prompt" size="480px">
      <el-input
        v-model="editingPrompt"
        type="textarea"
        :rows="20"
        placeholder="System prompt for this conversation"
      />
      <div style="margin-top: 16px; text-align: right">
        <el-button type="primary" @click="handleSavePrompt">Save</el-button>
      </div>
    </el-drawer>

    <!-- Files Drawer -->
    <el-drawer v-model="showFilesDrawer" title="Workspace Files" size="480px" @open="fileBrowserRef?.refresh()">
      <FileBrowser v-if="chatStore.currentConversationId" ref="fileBrowserRef" :conversation-id="chatStore.currentConversationId" />
    </el-drawer>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Fold, Expand, Setting, SwitchButton } from '@element-plus/icons-vue'
import { useAuthStore } from '../stores/auth'
import { useChatStore } from '../stores/chat'
import { useSettingsStore } from '../stores/settings'
import ConversationList from '../components/ConversationList.vue'
import ChatMessage from '../components/ChatMessage.vue'
import ChatInput from '../components/ChatInput.vue'
import FileBrowser from '../components/FileBrowser.vue'

const auth = useAuthStore()
const chatStore = useChatStore()
const settingsStore = useSettingsStore()

const sidebarCollapsed = ref(false)
const showNewChatDialog = ref(false)
const newChatPresetId = ref('')
const newChatPrompt = ref('')
const showPromptDrawer = ref(false)
const editingPrompt = ref('')
const showFilesDrawer = ref(false)
const fileBrowserRef = ref<InstanceType<typeof FileBrowser> | null>(null)

const deepThinking = computed(() => currentConversation.value?.deep_thinking ?? false)

const presets = computed(() => settingsStore.presets)

onMounted(async () => {
  await Promise.all([
    chatStore.loadConversations(),
    settingsStore.loadProviders(),
    settingsStore.loadPresets(),
  ])
  if (auth.accessToken) {
    chatStore.connectWs(auth.accessToken)
  }
  if (settingsStore.defaultPreset) {
    newChatPresetId.value = settingsStore.defaultPreset.id
    newChatPrompt.value = settingsStore.defaultPreset.content
  }
})

onUnmounted(() => {
  chatStore.disconnectWs()
})

function handlePresetSelect(presetId: string) {
  if (presetId === '__custom__') {
    newChatPrompt.value = ''
    return
  }
  const preset = presets.value.find(p => p.id === presetId)
  if (preset) newChatPrompt.value = preset.content
}

async function handleCreateChat() {
  try {
    const prompt = newChatPrompt.value.trim() || undefined
    const defaultProvider = settingsStore.providers.find(p => p.is_default)
    const provider = defaultProvider?.provider
    const modelName = defaultProvider?.models[0]
    const conv = await chatStore.createConversation(undefined, prompt, provider, modelName)
    await chatStore.selectConversation(conv.id)
    showNewChatDialog.value = false
    if (settingsStore.defaultPreset) {
      newChatPresetId.value = settingsStore.defaultPreset.id
      newChatPrompt.value = settingsStore.defaultPreset.content
    } else {
      newChatPresetId.value = ''
      newChatPrompt.value = ''
    }
  } catch (e) {
    ElMessage.error('Failed to create chat')
    console.error(e)
  }
}

async function handleSelectConversation(id: string) {
  try {
    await chatStore.selectConversation(id)
  } catch (e) {
    ElMessage.error('Failed to load conversation')
    console.error(e)
  }
}

async function handleDeleteConversation(id: string) {
  try {
    await ElMessageBox.confirm('Delete this conversation? This cannot be undone.', 'Confirm', { type: 'warning' })
  } catch { return }
  await chatStore.deleteConversation(id)
}

function handleSend(content: string) {
  chatStore.sendMessage(content)
}

function handleEditMessage(messageId: string, newContent: string) {
  chatStore.editMessage(messageId, newContent)
}

function handleRegenerateMessage(messageId: string) {
  chatStore.regenerateMessage(messageId)
}

function toggleDeepThinking() {
  if (!chatStore.currentConversationId) return
  chatStore.updateConversation(chatStore.currentConversationId, {
    deep_thinking: !deepThinking.value,
  })
}

watch(showPromptDrawer, (open) => {
  if (open) {
    editingPrompt.value = currentConversation.value?.system_prompt_override || ''
  }
})

async function handleSavePrompt() {
  if (!chatStore.currentConversationId) return
  await chatStore.updateConversation(chatStore.currentConversationId, {
    system_prompt_override: editingPrompt.value || '',
  })
  showPromptDrawer.value = false
  ElMessage.success('System prompt updated')
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

<style scoped>
.chat-layout {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.chat-sidebar {
  width: var(--sidebar-width);
  background: var(--bg-sidebar);
  display: flex;
  flex-direction: column;
  transition: width var(--transition-normal);
  overflow: hidden;
  flex-shrink: 0;
}
.chat-sidebar.collapsed {
  width: 0;
}

.sidebar-header {
  padding: 16px 12px;
}
.new-chat-btn {
  width: 100%;
  justify-content: center;
  gap: 8px;
  border: 1px solid rgba(255, 255, 255, 0.15) !important;
  color: var(--text-sidebar) !important;
  font-size: 14px;
  background: transparent !important;
}
.new-chat-btn:hover {
  background: var(--bg-sidebar-hover) !important;
}

.sidebar-footer {
  margin-top: auto;
  padding: 12px;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  gap: 4px;
}
.sidebar-link {
  color: var(--text-sidebar-muted) !important;
  font-size: 13px;
  gap: 6px;
}
.sidebar-link:hover {
  color: var(--text-sidebar) !important;
  background: var(--bg-sidebar-hover) !important;
}
.sidebar-link.logout:hover {
  color: #F87171 !important;
}

.chat-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--bg-main);
  position: relative;
  min-width: 0;
}

.sidebar-toggle {
  position: absolute;
  top: 12px;
  left: 12px;
  z-index: 10;
  color: var(--text-secondary) !important;
}

.chat-toolbar {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}
.toolbar-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding-left: 40px;
}
.toolbar-label {
  color: var(--text-secondary);
  font-size: 13px;
}
.model-cascader {
  width: 340px;
}
.toolbar-btn {
  color: var(--text-secondary) !important;
  font-size: 13px;
}

.waiting-indicator {
  height: 60px;
}

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 24px 16px;
}
.messages-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
}

.empty-state {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
  font-size: 15px;
}

.ws-status-bar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 6px 16px;
  font-size: 13px;
  flex-shrink: 0;
}
.ws-disconnected {
  color: #D97706;
  background: rgba(217, 119, 6, 0.08);
}
.ws-send-failed {
  color: #EF4444;
  background: rgba(239, 68, 68, 0.08);
}
.ws-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #D97706;
}
.ws-dot.pulse {
  animation: ws-pulse 1.5s infinite;
}
@keyframes ws-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}
</style>