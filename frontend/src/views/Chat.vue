<template>
  <div class="chat-layout">
    <!-- Sidebar -->
    <aside class="chat-sidebar" :class="{ collapsed: sidebarCollapsed }">
      <div class="sidebar-header">
        <el-button class="new-chat-btn" @click="showNewChatDialog = true">
          <el-icon><Plus /></el-icon>
          <span v-if="!sidebarCollapsed">{{ t('chat.newChat') }}</span>
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
          <span>{{ t('chat.settings') }}</span>
        </el-button>
        <el-button class="sidebar-link logout" text @click="auth.logout()">
          <el-icon><SwitchButton /></el-icon>
          <span>{{ t('chat.logout') }}</span>
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
            <span class="toolbar-label">{{ t('chat.model') }}</span>
            <el-cascader
              :model-value="cascaderValue"
              :options="cascaderOptions"
              :props="{ expandTrigger: 'hover' }"
              :placeholder="t('chat.selectProviderModel')"
              clearable
              size="small"
              class="model-cascader"
              @change="handleCascaderChange"
            />
            <span class="toolbar-label">{{ t('chat.subagent') }}</span>
            <el-cascader
              :model-value="subagentCascaderValue"
              :options="subagentCascaderOptions"
              :props="{ expandTrigger: 'hover' }"
              :placeholder="t('chat.selectSubagentModel')"
              clearable
              size="small"
              class="model-cascader"
              @change="handleSubagentCascaderChange"
            />
            <span class="toolbar-label">{{ t('chat.image') }}</span>
            <el-cascader
              :model-value="imageCascaderValue"
              :options="imageCascaderOptions"
              :props="{ expandTrigger: 'hover' }"
              :placeholder="t('chat.selectImageModel')"
              clearable
              size="small"
              class="model-cascader"
              @change="handleImageCascaderChange"
            />
            <el-button class="toolbar-btn" text @click="showFilesDrawer = true">{{ t('chat.files') }}</el-button>
            <el-button class="toolbar-btn" text @click="showShareDialog = true">{{ t('chat.share') }}</el-button>
          </div>
        </div>
        <div class="chat-messages">
          <div class="messages-inner">
            <ChatMessage
              v-for="msg in chatStore.messages"
              :key="msg.id"
              :message="msg"
              :conversation-id="chatStore.currentConversationId || undefined"
              :is-streaming="chatStore.isStreaming"
              @edit="handleEditMessage"
              @regenerate="handleRegenerateMessage"
            />
            <div v-if="chatStore.isWaiting && !chatStore.isStreaming" v-loading="true" class="waiting-indicator" element-loading-background="transparent" />
            <ChatMessage
              v-if="chatStore.isStreaming"
              :message="{ id: 'streaming', role: 'assistant', content: chatStore.streamingContent, tool_calls: null, tool_call_id: null, token_count: null, created_at: '' }"
              :conversation-id="chatStore.currentConversationId || undefined"
              :is-streaming="true"
              :streaming-blocks="chatStore.streamingBlocks"
            />
          </div>
        </div>
        <ChatInput
          @send="handleSend"
          @stop="chatStore.cancelGeneration"
          :disabled="chatStore.isStreaming || chatStore.isWaiting"
          :streaming="chatStore.isStreaming || chatStore.isWaiting"
          :deep-thinking="deepThinking"
          :thinking-budget="thinkingBudget"
          :subagent-thinking-budget="subagentThinkingBudget"
          @update:deep-thinking="toggleDeepThinking"
          @update:thinking-budget="updateThinkingBudget"
          @update:subagent-thinking-budget="updateSubagentThinkingBudget"
          @attach-files="handleAttachFiles"
        />
        <div v-if="!chatStore.wsConnected" class="ws-status-bar ws-disconnected">
          <span class="ws-dot pulse"></span>
          <span>{{ t('chat.connectionLost') }}</span>
        </div>
        <div v-if="chatStore.sendFailed" class="ws-status-bar ws-send-failed">
          <span>{{ t('chat.sendFailed') }}</span>
        </div>
      </template>
      <template v-else>
        <div class="empty-state">
          <p>{{ t('chat.emptyState') }}</p>
        </div>
      </template>
    </main>

    <!-- New Chat Dialog -->
    <el-dialog v-model="showNewChatDialog" :title="t('chat.dialog.newChatTitle')" width="520px">
      <el-form label-position="top">
        <el-form-item :label="t('chat.dialog.systemPromptPreset')">
          <el-select v-model="newChatPresetId" :placeholder="t('chat.dialog.selectPreset')" style="width: 100%" @change="handlePresetSelect">
            <el-option
              v-for="preset in presets"
              :key="preset.id"
              :label="preset.name"
              :value="preset.id"
            >
              <span>{{ preset.name }}</span>
              <span style="color: var(--text-secondary); font-size: 12px; margin-left: 8px">{{ preset.description }}</span>
            </el-option>
            <el-option :label="t('chat.dialog.customPreset')" value="__custom__" />
          </el-select>
        </el-form-item>
        <el-form-item :label="t('chat.dialog.systemPrompt')">
          <el-input
            v-model="newChatPrompt"
            type="textarea"
            :rows="6"
            :placeholder="t('chat.dialog.customPromptPlaceholder')"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showNewChatDialog = false">{{ t('common.cancel') }}</el-button>
        <el-button type="primary" @click="handleCreateChat">{{ t('common.create') }}</el-button>
      </template>
    </el-dialog>

    <!-- Files Drawer -->
    <el-drawer v-model="showFilesDrawer" :title="t('chat.dialog.workspaceFilesTitle')" size="480px" @open="fileBrowserRef?.refresh()">
      <FileBrowser v-if="chatStore.currentConversationId" ref="fileBrowserRef" :conversation-id="chatStore.currentConversationId" />
    </el-drawer>

    <!-- Share Dialog -->
    <el-dialog v-model="showShareDialog" :title="t('chat.dialog.shareConversationTitle')" width="480px">
      <template v-if="!currentConversation?.share_token">
        <p style="color: var(--text-secondary); margin: 0 0 16px">{{ t('chat.dialog.shareCreateHint') }}</p>
        <el-button type="primary" @click="handleCreateShare" :loading="shareLoading">{{ t('chat.dialog.createLink') }}</el-button>
      </template>
      <template v-else>
        <p style="color: var(--text-secondary); margin: 0 0 12px">{{ t('chat.dialog.shareActiveHint') }}</p>
        <div style="display: flex; gap: 8px; margin-bottom: 16px">
          <el-input :model-value="shareUrl" readonly />
          <el-button @click="copyShareUrl">{{ t('common.copy') }}</el-button>
        </div>
        <el-button type="danger" plain @click="handleRevokeShare" :loading="shareLoading">{{ t('chat.dialog.stopSharing') }}</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, computed, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Fold, Expand, Setting, SwitchButton } from '@element-plus/icons-vue'
import { useAuthStore } from '../stores/auth'
import { useChatStore } from '../stores/chat'
import { useSettingsStore } from '../stores/settings'
import ConversationList from '../components/ConversationList.vue'
import ChatMessage from '../components/ChatMessage.vue'
import ChatInput from '../components/ChatInput.vue'
import FileBrowser from '../components/FileBrowser.vue'
import { uploadFiles } from '../api/conversations'
import { createShare, revokeShare } from '../api/sharing'
import { t } from '../i18n'

const auth = useAuthStore()
const chatStore = useChatStore()
const settingsStore = useSettingsStore()

const sidebarCollapsed = ref(false)
const showNewChatDialog = ref(false)
const newChatPresetId = ref('')
const newChatPrompt = ref('')
const showFilesDrawer = ref(false)
const fileBrowserRef = ref<InstanceType<typeof FileBrowser> | null>(null)
const showShareDialog = ref(false)
const shareLoading = ref(false)

const deepThinking = computed(() => currentConversation.value?.deep_thinking ?? false)
const thinkingBudget = computed(() => currentConversation.value?.thinking_budget ?? null)
const subagentThinkingBudget = computed(() => currentConversation.value?.subagent_thinking_budget ?? null)

const presets = computed(() => settingsStore.presets)

onMounted(async () => {
  const [convResult, providersResult, presetsResult] = await Promise.allSettled([
    chatStore.loadConversations(),
    settingsStore.loadProviders(),
    settingsStore.loadPresets(),
  ])

  if (convResult.status === 'rejected') {
    ElMessage.error(t('chat.messages.failedLoadConversations'))
  }
  if (providersResult.status === 'rejected') {
    ElMessage.error(t('chat.messages.failedLoadProviders'))
  }
  if (presetsResult.status === 'rejected') {
    ElMessage.error(t('chat.messages.failedLoadPromptPresets'))
  }

  if (auth.isAuthenticated) {
    chatStore.connectWs()
  }
  if (presetsResult.status === 'fulfilled' && settingsStore.defaultPreset) {
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
    const provider = defaultProvider?.name
    const modelName = defaultProvider?.models[0]
    const conv = await chatStore.createConversation(
      undefined,
      prompt,
      provider,
      modelName,
      undefined,
      undefined,
      provider,
      modelName,
    )
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
    ElMessage.error(t('chat.messages.failedCreateChat'))
    console.error(e)
  }
}

async function handleSelectConversation(id: string) {
  try {
    await chatStore.selectConversation(id)
  } catch (e) {
    ElMessage.error(t('chat.messages.failedLoadConversation'))
    console.error(e)
  }
}

async function handleDeleteConversation(id: string) {
  try {
    await ElMessageBox.confirm(t('chat.confirmDeleteConversation'), t('common.confirm'), { type: 'warning' })
  } catch { return }
  try {
    await chatStore.deleteConversation(id)
  } catch (e) {
    ElMessage.error(t('chat.messages.failedDeleteConversation'))
    console.error(e)
  }
}

function handleSend(content: string) {
  chatStore.sendMessage(content)
}

async function handleAttachFiles(files: File[]) {
  if (!chatStore.currentConversationId) return
  try {
    await uploadFiles(chatStore.currentConversationId, files)
    ElMessage.success(t('chat.messages.uploadedFiles', { count: files.length }))
    if (showFilesDrawer.value) {
      fileBrowserRef.value?.refresh()
    }
  } catch {
    ElMessage.error(t('chat.messages.uploadFailed'))
  }
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

function updateThinkingBudget(value: number | null) {
  if (!chatStore.currentConversationId) return
  chatStore.updateConversation(chatStore.currentConversationId, {
    thinking_budget: value,
  })
}

function updateSubagentThinkingBudget(value: number | null) {
  if (!chatStore.currentConversationId) return
  chatStore.updateConversation(chatStore.currentConversationId, {
    subagent_thinking_budget: value,
  })
}

const currentConversation = computed(() =>
  chatStore.conversations.find(c => c.id === chatStore.currentConversationId)
)

const shareUrl = computed(() => {
  const token = currentConversation.value?.share_token
  if (!token) return ''
  return `${window.location.origin}/share/${token}`
})

async function handleCreateShare() {
  if (!chatStore.currentConversationId) return
  shareLoading.value = true
  try {
    const resp = await createShare(chatStore.currentConversationId)
    // Update the conversation in the store
    const conv = chatStore.conversations.find(c => c.id === chatStore.currentConversationId)
    if (conv) conv.share_token = resp.share_token
  } catch {
    ElMessage.error(t('chat.messages.failedCreateShareLink'))
  } finally {
    shareLoading.value = false
  }
}

async function handleRevokeShare() {
  if (!chatStore.currentConversationId) return
  shareLoading.value = true
  try {
    await revokeShare(chatStore.currentConversationId)
    const conv = chatStore.conversations.find(c => c.id === chatStore.currentConversationId)
    if (conv) conv.share_token = null
  } catch {
    ElMessage.error(t('chat.messages.failedRevokeShareLink'))
  } finally {
    shareLoading.value = false
  }
}

async function copyShareUrl() {
  try {
    await navigator.clipboard.writeText(shareUrl.value)
    ElMessage.success({ message: t('chat.messages.linkCopied'), duration: 1500 })
  } catch {
    ElMessage.error(t('chat.messages.copyFailed'))
  }
}

const cascaderOptions = computed(() => {
  return settingsStore.providers.map(p => ({
    value: p.name,
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

const imageCascaderOptions = computed(() => {
  return settingsStore.providers
    .filter(p => p.image_models.length > 0)
    .map(p => ({
      value: p.name,
      label: p.name || p.provider,
      children: p.image_models.map(m => ({
        value: m,
        label: m,
      })),
    }))
})

const subagentCascaderOptions = computed(() => {
  return settingsStore.providers.map(p => ({
    value: p.name,
    label: p.name || p.provider,
    children: p.models.map(m => ({
      value: m,
      label: m,
    })),
  }))
})

const subagentCascaderValue = computed(() => {
  const conv = currentConversation.value
  if (conv?.subagent_provider && conv?.subagent_model) {
    return [conv.subagent_provider, conv.subagent_model]
  }
  return []
})

async function handleSubagentCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      subagent_provider: val[0],
      subagent_model: val[1],
    })
  } else {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      subagent_provider: '',
      subagent_model: '',
    })
  }
}

const imageCascaderValue = computed(() => {
  const conv = currentConversation.value
  if (conv?.image_provider && conv?.image_model) {
    return [conv.image_provider, conv.image_model]
  }
  return []
})

async function handleImageCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      image_provider: val[0],
      image_model: val[1],
    })
  } else {
    await chatStore.updateConversation(chatStore.currentConversationId, {
      image_provider: '',
      image_model: '',
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
  overflow-x: auto;
  overflow-y: hidden;
}
.toolbar-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: nowrap;
  min-width: max-content;
  padding-left: 40px;
}
.toolbar-label {
  color: var(--text-secondary);
  font-size: 13px;
  white-space: nowrap;
  flex: 0 0 auto;
}
.model-cascader {
  width: 300px;
  min-width: 300px;
  flex: 0 0 300px;
}
.toolbar-btn {
  color: var(--text-secondary) !important;
  font-size: 13px;
  white-space: nowrap;
  flex: 0 0 auto;
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
