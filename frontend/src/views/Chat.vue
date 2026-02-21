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
          <div class="chat-toolbar-frame">
            <div class="toolbar-side">
              <div class="toolbar-actions">
                <el-button class="toolbar-btn" text @click="showFilesDrawer = true">{{ t('chat.files') }}</el-button>
                <el-button class="toolbar-btn" text @click="showShareDialog = true">{{ t('chat.share') }}</el-button>
              </div>
              <LocaleToggle variant="toolbar" class="toolbar-locale" />
            </div>
          </div>
        </div>
        <div class="chat-messages">
          <div class="messages-inner">
            <ChatMessage
              v-for="msg in chatStore.messages"
              :key="msg.id"
              :message="msg"
              :conversation-id="chatStore.currentConversationId || undefined"
              :is-streaming="chatStore.isStreaming || chatStore.isWaiting"
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
        <div class="chat-models">
          <div class="chat-models-inner">
            <div class="toolbar-main">
              <div class="toolbar-control">
                <span class="toolbar-label">{{ t('chat.model') }}</span>
                <el-cascader
                  :model-value="cascaderValue"
                  :options="cascaderOptions"
                  :props="{ expandTrigger: 'hover' }"
                  :placeholder="t('chat.selectProviderModel')"
                  size="small"
                  class="model-cascader"
                  @change="handleCascaderChange"
                />
              </div>
              <div class="toolbar-control">
                <span class="toolbar-label">{{ t('chat.subagent') }}</span>
                <el-cascader
                  :model-value="subagentCascaderValue"
                  :options="subagentCascaderOptions"
                  :props="{ expandTrigger: 'hover' }"
                  :placeholder="t('chat.selectSubagentModel')"
                  size="small"
                  class="model-cascader"
                  @change="handleSubagentCascaderChange"
                />
              </div>
              <div class="toolbar-control">
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
              </div>
            </div>
            <p v-if="modelDraftWarningMessage" class="model-warning">
              {{ modelDraftWarningMessage }}
            </p>
          </div>
        </div>
        <QuestionFlow
          v-if="chatStore.activeQuestionnaire"
          :questionnaire="chatStore.activeQuestionnaire"
          :disabled="chatStore.questionnaireSubmitting"
          @submit="handleSubmitQuestionnaire"
        />
        <ChatInput
          v-else
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
import { onMounted, onUnmounted, computed, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus, Fold, Expand, Setting, SwitchButton } from '@element-plus/icons-vue'
import { useAuthStore } from '../stores/auth'
import { useChatStore } from '../stores/chat'
import { useSettingsStore } from '../stores/settings'
import type { Conversation, ProviderConfig } from '../types'
import ConversationList from '../components/ConversationList.vue'
import ChatMessage from '../components/ChatMessage.vue'
import ChatInput from '../components/ChatInput.vue'
import QuestionFlow from '../components/QuestionFlow.vue'
import FileBrowser from '../components/FileBrowser.vue'
import LocaleToggle from '../components/LocaleToggle.vue'
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
const currentConversation = computed(() =>
  chatStore.conversations.find(c => c.id === chatStore.currentConversationId)
)

const deepThinking = computed(() => currentConversation.value?.deep_thinking ?? false)
const thinkingBudget = computed(() => currentConversation.value?.thinking_budget ?? null)
const subagentThinkingBudget = computed(() => currentConversation.value?.subagent_thinking_budget ?? null)

const presets = computed(() => settingsStore.presets)
type ModelRole = 'chat' | 'subagent' | 'image'
type ModelDraft = {
  providerId: string | null
  modelName: string | null
}

const chatModelDraft = ref<ModelDraft>({ providerId: null, modelName: null })
const subagentModelDraft = ref<ModelDraft>({ providerId: null, modelName: null })
const imageModelDraft = ref<ModelDraft>({ providerId: null, modelName: null })
const modelDraftSubmitting = ref(false)
let modelDraftResubmitRequested = false

function toModelDraft(providerId: string | null | undefined, modelName: string | null | undefined): ModelDraft {
  return {
    providerId: providerId || null,
    modelName: modelName || null,
  }
}

function resetModelDrafts(conversation?: Conversation) {
  chatModelDraft.value = toModelDraft(conversation?.provider_id, conversation?.model_name)
  subagentModelDraft.value = toModelDraft(conversation?.subagent_provider_id, conversation?.subagent_model)
  imageModelDraft.value = toModelDraft(conversation?.image_provider_id, conversation?.image_model)
}

function getApiErrorMessage(error: unknown): string | null {
  if (typeof error !== 'object' || error === null) return null
  const maybeError = error as {
    response?: { data?: { message?: unknown } }
    message?: unknown
  }
  const serverMessage = maybeError.response?.data?.message
  if (typeof serverMessage === 'string' && serverMessage.trim().length > 0) {
    return serverMessage
  }
  if (typeof maybeError.message === 'string' && maybeError.message.trim().length > 0) {
    return maybeError.message
  }
  return null
}

function findProvider(providerId: string | null): ProviderConfig | undefined {
  if (!providerId) return undefined
  return settingsStore.providers.find(p => p.id === providerId)
}

function providerHasModel(providerId: string | null, modelName: string | null, useImageModels: boolean): boolean {
  if (!providerId || !modelName) return false
  const provider = findProvider(providerId)
  if (!provider) return false
  const models = useImageModels ? provider.image_models : provider.models
  return models.includes(modelName)
}

function collectInvalidModelRoles(): ModelRole[] {
  const invalid = new Set<ModelRole>()
  const { providerId: chatProviderId, modelName: chatModelName } = chatModelDraft.value
  const { providerId: subagentProviderId, modelName: subagentModelName } = subagentModelDraft.value
  const { providerId: imageProviderId, modelName: imageModelName } = imageModelDraft.value

  if (!providerHasModel(chatProviderId, chatModelName, false)) {
    invalid.add('chat')
  }
  if (!providerHasModel(subagentProviderId, subagentModelName, false)) {
    invalid.add('subagent')
  }
  if (Boolean(imageProviderId) !== Boolean(imageModelName)) {
    invalid.add('image')
  } else if ((imageProviderId || imageModelName) && !providerHasModel(imageProviderId, imageModelName, true)) {
    invalid.add('image')
  }

  return Array.from(invalid)
}

const modelDraftWarningMessage = computed(() => {
  const invalidRoles = collectInvalidModelRoles()
  if (invalidRoles.length === 0) return ''
  const roleLabels = invalidRoles.map((role) => {
    if (role === 'chat') return t('chat.model')
    if (role === 'subagent') return t('chat.subagent')
    return t('chat.image')
  })
  return t('chat.messages.invalidConversationModelConfigRoles', {
    roles: roleLabels.join(', '),
  })
})

function getModelValidationMessageKey(): string | null {
  const { providerId: chatProviderId, modelName: chatModelName } = chatModelDraft.value
  if (!providerHasModel(chatProviderId, chatModelName, false)) {
    return 'chat.messages.invalidMainModelSelection'
  }

  const { providerId: subagentProviderId, modelName: subagentModelName } = subagentModelDraft.value
  if (!providerHasModel(subagentProviderId, subagentModelName, false)) {
    return 'chat.messages.invalidSubagentModelSelection'
  }

  const { providerId: imageProviderId, modelName: imageModelName } = imageModelDraft.value
  if (Boolean(imageProviderId) !== Boolean(imageModelName)) {
    return 'chat.messages.invalidImageModelPair'
  }
  if ((imageProviderId || imageModelName) && !providerHasModel(imageProviderId, imageModelName, true)) {
    return 'chat.messages.invalidImageModelSelection'
  }

  return null
}

async function updateConversationWithErrorHandling(
  updates: Partial<Conversation>,
  fallbackMessageKey = 'chat.messages.failedUpdateConversation',
): Promise<{ success: boolean, statusCode: number | null, didRequest: boolean }> {
  if (!chatStore.currentConversationId) return { success: false, statusCode: null, didRequest: false }
  try {
    await chatStore.updateConversation(chatStore.currentConversationId, updates)
    return { success: true, statusCode: null, didRequest: true }
  } catch (error) {
    const statusCode = (
      typeof error === 'object'
      && error !== null
      && 'response' in error
      && typeof (error as { response?: { status?: unknown } }).response?.status === 'number'
    )
      ? (error as { response: { status: number } }).response.status
      : null
    ElMessage.error(getApiErrorMessage(error) || t(fallbackMessageKey))
    return { success: false, statusCode, didRequest: true }
  }
}

async function submitModelDraftAttempt(): Promise<{ success: boolean, statusCode: number | null, didRequest: boolean }> {
  const messageKey = getModelValidationMessageKey()
  if (messageKey) {
    ElMessage.error(t(messageKey))
    return { success: false, statusCode: null, didRequest: false }
  }
  const { providerId: imageProviderId, modelName: imageModelName } = imageModelDraft.value
  return await updateConversationWithErrorHandling(
    {
      provider_id: chatModelDraft.value.providerId,
      model_name: chatModelDraft.value.modelName,
      subagent_provider_id: subagentModelDraft.value.providerId,
      subagent_model: subagentModelDraft.value.modelName,
      image_provider_id: imageProviderId || '',
      image_model: imageModelName || '',
    },
    'chat.messages.modelUpdateFailed',
  )
}

async function submitModelDraft() {
  if (modelDraftSubmitting.value) {
    modelDraftResubmitRequested = true
    return
  }

  modelDraftSubmitting.value = true
  try {
    do {
      modelDraftResubmitRequested = false
      const result = await submitModelDraftAttempt()
      if (!result.success) {
        if (result.didRequest && result.statusCode === 400) {
          await settingsStore.loadProviders().catch(() => {})
        }
        if (result.didRequest && !modelDraftResubmitRequested) {
          resetModelDrafts(currentConversation.value)
        }
      }
    } while (modelDraftResubmitRequested)
  } finally {
    modelDraftSubmitting.value = false
  }
}

onMounted(async () => {
  const [convResult, providersResult, presetsResult, modelDefaultsResult] = await Promise.allSettled([
    chatStore.loadConversations(),
    settingsStore.loadProviders(),
    settingsStore.loadPresets(),
    settingsStore.loadModelDefaults(),
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
  if (modelDefaultsResult.status === 'rejected') {
    ElMessage.error(t('chat.messages.failedLoadModelDefaults'))
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

watch(
  currentConversation,
  (conversation) => {
    resetModelDrafts(conversation)
  },
  { immediate: true },
)

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
    const defaults = settingsStore.modelDefaults
    const provider = defaults?.chat_provider_id || undefined
    const modelName = defaults?.chat_model || undefined
    const subagentProvider = defaults?.subagent_provider_id || undefined
    const subagentModel = defaults?.subagent_model || undefined
    const imageProvider = defaults?.image_provider_id || undefined
    const imageModel = defaults?.image_model || undefined

    if (!provider || !modelName || !subagentProvider || !subagentModel) {
      ElMessage.error(t('chat.messages.missingRequiredDefaults'))
      return
    }

    const conv = await chatStore.createConversation(
      undefined,
      prompt,
      provider,
      modelName,
      imageProvider,
      imageModel,
      subagentProvider,
      subagentModel,
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

function handleSubmitQuestionnaire(answers: Array<{
  id: string
  question: string
  selected_options: string[]
  free_text: string
  notes: string
}>) {
  chatStore.submitQuestionnaireAnswers(answers)
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

async function toggleDeepThinking() {
  if (!chatStore.currentConversationId) return
  await updateConversationWithErrorHandling({
    deep_thinking: !deepThinking.value,
  })
}

async function updateThinkingBudget(value: number | null) {
  if (!chatStore.currentConversationId) return
  await updateConversationWithErrorHandling({
    thinking_budget: value,
  })
}

async function updateSubagentThinkingBudget(value: number | null) {
  if (!chatStore.currentConversationId) return
  await updateConversationWithErrorHandling({
    subagent_thinking_budget: value,
  })
}

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
    value: p.id,
    label: p.name || p.provider,
    children: p.models.map(m => ({
      value: m,
      label: m,
    })),
  }))
})

const cascaderValue = computed(() => {
  if (chatModelDraft.value.providerId && chatModelDraft.value.modelName) {
    return [chatModelDraft.value.providerId, chatModelDraft.value.modelName]
  }
  return []
})

async function handleCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    chatModelDraft.value = {
      providerId: val[0],
      modelName: val[1],
    }
    await submitModelDraft()
  }
}

const imageCascaderOptions = computed(() => {
  return settingsStore.providers
    .filter(p => p.image_models.length > 0)
    .map(p => ({
      value: p.id,
      label: p.name || p.provider,
      children: p.image_models.map(m => ({
        value: m,
        label: m,
      })),
    }))
})

const subagentCascaderOptions = computed(() => {
  return settingsStore.providers.map(p => ({
    value: p.id,
    label: p.name || p.provider,
    children: p.models.map(m => ({
      value: m,
      label: m,
    })),
  }))
})

const subagentCascaderValue = computed(() => {
  if (subagentModelDraft.value.providerId && subagentModelDraft.value.modelName) {
    return [subagentModelDraft.value.providerId, subagentModelDraft.value.modelName]
  }
  return []
})

async function handleSubagentCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    subagentModelDraft.value = {
      providerId: val[0],
      modelName: val[1],
    }
    await submitModelDraft()
  }
}

const imageCascaderValue = computed(() => {
  if (imageModelDraft.value.providerId && imageModelDraft.value.modelName) {
    return [imageModelDraft.value.providerId, imageModelDraft.value.modelName]
  }
  return []
})

async function handleImageCascaderChange(val: string[] | null) {
  if (!chatStore.currentConversationId) return
  if (val && val.length === 2) {
    imageModelDraft.value = {
      providerId: val[0],
      modelName: val[1],
    }
  } else {
    imageModelDraft.value = {
      providerId: null,
      modelName: null,
    }
  }
  await submitModelDraft()
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
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.72), rgba(250, 249, 246, 0.72));
}

.chat-toolbar-frame {
  max-width: var(--max-width-chat);
  margin: 0 auto;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  padding-left: 40px;
}

.chat-models {
  padding: 8px 16px 0;
  flex-shrink: 0;
}

.chat-models-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
  min-width: 0;
}

.toolbar-main {
  display: grid;
  grid-template-columns: repeat(3, minmax(180px, 1fr));
  gap: 10px;
  min-width: 0;
}

.toolbar-control {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.toolbar-label {
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  white-space: nowrap;
}

.model-cascader {
  width: 100%;
}

.model-warning {
  margin: 8px 0 0;
  color: #B45309;
  font-size: 12px;
}

.toolbar-side {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.toolbar-btn {
  color: var(--text-secondary) !important;
  font-size: 13px;
  white-space: nowrap;
  padding: 0 8px !important;
}

.toolbar-btn:hover {
  color: var(--text-primary) !important;
  background: rgba(0, 0, 0, 0.04) !important;
}

.toolbar-locale {
  flex-shrink: 0;
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

@media (max-width: 768px) {
  .chat-toolbar {
    padding: 8px 10px;
  }

  .chat-toolbar-frame {
    padding-left: 30px;
  }

  .chat-models {
    padding: 8px 10px 0;
  }

  .toolbar-main {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .toolbar-side {
    justify-content: space-between;
    width: 100%;
  }

  .toolbar-actions {
    gap: 2px;
  }
}

@media (max-width: 1024px) and (min-width: 769px) {
  .toolbar-main {
    grid-template-columns: repeat(3, minmax(150px, 1fr));
  }
}
</style>
