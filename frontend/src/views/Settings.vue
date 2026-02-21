<template>
  <div class="settings-layout">
    <header class="settings-header">
      <div class="settings-header-left">
        <el-button @click="$router.push('/')">{{ t('settings.backToChat') }}</el-button>
        <h3>{{ t('settings.title') }}</h3>
      </div>
      <LocaleToggle variant="header" />
    </header>
    <div class="settings-content">
      <el-tabs v-model="activeTab">
        <el-tab-pane :label="t('settings.tabs.providers')" name="providers">
          <el-table :data="settingsStore.providers" style="width: 100%">
            <el-table-column prop="name" :label="t('settings.table.name')" width="160" />
            <el-table-column :label="t('settings.table.apiType')" width="120">
              <template #default="{ row }">
                {{ providerLabel(row.provider) }}
              </template>
            </el-table-column>
            <el-table-column :label="t('settings.table.models')">
              <template #default="{ row }">
                <el-tag
                  v-for="m in row.models"
                  :key="m"
                  size="small"
                  style="margin-right: 4px; margin-bottom: 2px"
                >{{ m }}</el-tag>
                <span v-if="!row.models.length" class="text-muted">{{ t('settings.table.noModels') }}</span>
              </template>
            </el-table-column>
            <el-table-column :label="t('settings.table.imageModels')">
              <template #default="{ row }">
                <el-tag
                  v-for="m in row.image_models"
                  :key="m"
                  size="small"
                  type="warning"
                  style="margin-right: 4px; margin-bottom: 2px"
                >{{ m }}</el-tag>
                <span v-if="!row.image_models.length" class="text-muted">—</span>
              </template>
            </el-table-column>
            <el-table-column :label="t('common.actions')" width="190">
              <template #default="{ row }">
                <div class="row-actions">
                  <el-button text type="primary" @click="handleEditProvider(row)">{{ t('common.edit') }}</el-button>
                  <el-button text type="danger" @click="handleDeleteProvider(row.id)">{{ t('common.delete') }}</el-button>
                </div>
              </template>
            </el-table-column>
          </el-table>

          <el-divider />

          <h4>{{ isEditingProvider ? t('settings.provider.editTitle') : t('settings.provider.addTitle') }}</h4>
          <el-form
            ref="providerFormRef"
            :model="providerForm"
            :rules="providerRules"
            label-position="top"
            class="provider-form"
          >
            <el-form-item :label="t('settings.provider.name')" prop="name">
              <el-input v-model="providerForm.name" :placeholder="t('settings.provider.placeholders.name')" />
            </el-form-item>
            <el-form-item :label="t('settings.provider.apiType')" prop="providerType">
              <el-select v-model="providerForm.providerType" :placeholder="t('settings.provider.placeholders.apiType')">
                <el-option label="OpenAI" value="openai" />
                <el-option label="Anthropic" value="anthropic" />
                <el-option label="Google" value="google" />
                <el-option label="Mistral" value="mistral" />
              </el-select>
            </el-form-item>
            <el-form-item :label="t('settings.provider.apiKey')" prop="apiKey">
              <el-input
                v-model="providerForm.apiKey"
                type="password"
                show-password
                :placeholder="isEditingProvider ? t('settings.provider.placeholders.apiKeyKeep') : t('settings.provider.placeholders.apiKeyNew')"
              />
            </el-form-item>
            <el-form-item :label="t('settings.provider.modelsOptional')" prop="models">
              <div class="model-tags">
                <el-tag v-for="m in providerForm.models" :key="m" closable @close="removeModel(m)">{{ m }}</el-tag>
              </div>
              <el-input
                v-model="modelToAdd"
                :placeholder="t('settings.provider.placeholders.model')"
                @keyup.enter="addModel"
              >
                <template #append>
                  <el-button @click="addModel">{{ t('common.add') }}</el-button>
                </template>
              </el-input>
            </el-form-item>
            <el-form-item :label="t('settings.provider.imageModelsOptional')" prop="models">
              <div class="model-tags">
                <el-tag
                  v-for="m in providerForm.imageModels"
                  :key="m"
                  type="warning"
                  closable
                  @close="removeImageModel(m)"
                >{{ m }}</el-tag>
              </div>
              <el-input
                v-model="imageModelToAdd"
                :placeholder="t('settings.provider.placeholders.imageModel')"
                @keyup.enter="addImageModel"
              >
                <template #append>
                  <el-button @click="addImageModel">{{ t('common.add') }}</el-button>
                </template>
              </el-input>
            </el-form-item>
            <el-form-item :label="t('settings.provider.customEndpointOptional')">
              <el-input v-model="providerForm.endpointUrl" :placeholder="t('settings.provider.placeholders.endpoint')" />
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="handleSaveProvider">
                {{ isEditingProvider ? t('common.update') : t('common.save') }}
              </el-button>
              <el-button v-if="isEditingProvider" @click="resetProviderForm">{{ t('common.cancel') }}</el-button>
            </el-form-item>
          </el-form>

          <el-divider />

          <h4>{{ t('settings.defaults.title') }}</h4>
          <p class="text-muted defaults-desc">{{ t('settings.defaults.description') }}</p>
          <el-alert
            v-if="defaultsNeedRequiredConfig"
            :title="t('settings.defaults.missingRequired')"
            type="warning"
            show-icon
            :closable="false"
            style="margin-bottom: 12px"
          />
          <el-form label-position="top" class="provider-form">
            <el-form-item :label="t('settings.defaults.chatModel')">
              <el-cascader
                :model-value="chatDefaultValue"
                :options="chatModelOptions"
                :props="{ expandTrigger: 'hover' }"
                clearable
                @change="handleChatDefaultChange"
              />
            </el-form-item>
            <el-form-item :label="t('settings.defaults.subagentModel')">
              <el-cascader
                :model-value="subagentDefaultValue"
                :options="chatModelOptions"
                :props="{ expandTrigger: 'hover' }"
                clearable
                @change="handleSubagentDefaultChange"
              />
            </el-form-item>
            <el-form-item :label="t('settings.defaults.imageModelOptional')">
              <el-cascader
                :model-value="imageDefaultValue"
                :options="imageModelOptions"
                :props="{ expandTrigger: 'hover' }"
                clearable
                @change="handleImageDefaultChange"
              />
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="handleSaveModelDefaults">{{ t('common.save') }}</el-button>
              <el-button @click="syncModelDefaultsForm">{{ t('common.refresh') }}</el-button>
            </el-form-item>
          </el-form>
        </el-tab-pane>

        <el-tab-pane :label="t('settings.tabs.prompts')" name="presets">
          <el-table :data="settingsStore.presets" style="width: 100%">
            <el-table-column prop="name" :label="t('settings.table.name')" width="180" />
            <el-table-column :label="t('settings.table.description')">
              <template #default="{ row }">
                {{ row.description.length > 80 ? row.description.slice(0, 80) + '...' : row.description }}
              </template>
            </el-table-column>
            <el-table-column :label="t('common.default')" width="80">
              <template #default="{ row }">
                <el-tag v-if="row.is_default" type="success" size="small">{{ t('common.default') }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column :label="t('common.actions')" width="190">
              <template #default="{ row }">
                <div class="row-actions">
                  <el-button text type="primary" @click="handleEditPreset(row)">{{ t('common.edit') }}</el-button>
                  <el-button text type="danger" @click="handleDeletePreset(row.id)">{{ t('common.delete') }}</el-button>
                </div>
              </template>
            </el-table-column>
          </el-table>

          <el-divider />

          <h4>{{ isEditingPreset ? t('settings.preset.editTitle') : t('settings.preset.addTitle') }}</h4>
          <el-form ref="presetFormRef" :model="presetForm" :rules="presetRules" label-position="top" class="provider-form">
            <el-form-item :label="t('settings.preset.name')" prop="name">
              <el-input v-model="presetForm.name" :placeholder="t('settings.preset.placeholders.name')" />
            </el-form-item>
            <el-form-item :label="t('settings.preset.description')">
              <el-input v-model="presetForm.description" :placeholder="t('settings.preset.placeholders.description')" />
            </el-form-item>
            <el-form-item :label="t('settings.preset.content')" prop="content">
              <el-input v-model="presetForm.content" type="textarea" :rows="12" :placeholder="t('settings.preset.placeholders.content')" />
            </el-form-item>
            <el-form-item>
              <el-checkbox v-model="presetForm.isDefault">{{ t('settings.preset.setDefault') }}</el-checkbox>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="handleSavePreset">{{ isEditingPreset ? t('common.update') : t('common.save') }}</el-button>
              <el-button v-if="isEditingPreset" @click="resetPresetForm">{{ t('common.cancel') }}</el-button>
            </el-form-item>
          </el-form>
        </el-tab-pane>
      </el-tabs>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import type { FormInstance, FormRules } from 'element-plus'
import { useSettingsStore } from '../stores/settings'
import { PROVIDER_LABELS } from '../constants/providers'
import type { ModelDefaults, ProviderConfig, SystemPromptPreset } from '../types'
import LocaleToggle from '../components/LocaleToggle.vue'
import { t } from '../i18n'

const settingsStore = useSettingsStore()

const activeTab = ref('providers')
const isEditingProvider = ref(false)
const editingProviderId = ref('')
const isEditingPreset = ref(false)
const editingPresetId = ref('')

const providerForm = reactive({
  name: '',
  providerType: '',
  apiKey: '',
  models: [] as string[],
  imageModels: [] as string[],
  endpointUrl: '',
})

const defaultsForm = reactive({
  chatProvider: '',
  chatModel: '',
  subagentProvider: '',
  subagentModel: '',
  imageProvider: '',
  imageModel: '',
})

const presetForm = reactive({
  name: '',
  description: '',
  content: '',
  isDefault: false,
})

const modelToAdd = ref('')
const imageModelToAdd = ref('')

const providerFormRef = ref<FormInstance>()
const presetFormRef = ref<FormInstance>()

const providerRules = computed<FormRules>(() => ({
  name: [{ required: true, message: t('settings.provider.validation.nameRequired'), trigger: 'blur' }],
  providerType: [{ required: true, message: t('settings.provider.validation.apiTypeRequired'), trigger: 'change' }],
  apiKey: [{
    validator: (_rule, value, callback) => {
      if (!isEditingProvider.value && !value) {
        callback(new Error(t('settings.provider.validation.apiKeyRequired')))
      } else {
        callback()
      }
    },
    trigger: 'blur',
  }],
  models: [{
    validator: (_rule, _value, callback) => {
      if (!providerForm.models.length && !providerForm.imageModels.length) {
        callback(new Error(t('settings.provider.validation.atLeastOneModelRequired')))
      } else {
        callback()
      }
    },
    trigger: 'change',
  }],
}))

const presetRules = computed<FormRules>(() => ({
  name: [{ required: true, message: t('settings.preset.validation.nameRequired'), trigger: 'blur' }],
  content: [{ required: true, message: t('settings.preset.validation.contentRequired'), trigger: 'blur' }],
}))

const chatModelOptions = computed(() => settingsStore.providers.map(p => ({
  value: p.id,
  label: p.name || p.provider,
  children: p.models.map(m => ({ value: m, label: m })),
})))

const imageModelOptions = computed(() =>
  settingsStore.providers
    .filter(p => p.image_models.length > 0)
    .map(p => ({
      value: p.id,
      label: p.name || p.provider,
      children: p.image_models.map(m => ({ value: m, label: m })),
    })),
)

const chatDefaultValue = computed(() => (
  defaultsForm.chatProvider && defaultsForm.chatModel
    ? [defaultsForm.chatProvider, defaultsForm.chatModel]
    : []
))

const subagentDefaultValue = computed(() => (
  defaultsForm.subagentProvider && defaultsForm.subagentModel
    ? [defaultsForm.subagentProvider, defaultsForm.subagentModel]
    : []
))

const imageDefaultValue = computed(() => (
  defaultsForm.imageProvider && defaultsForm.imageModel
    ? [defaultsForm.imageProvider, defaultsForm.imageModel]
    : []
))

const defaultsNeedRequiredConfig = computed(() =>
  !defaultsForm.chatProvider || !defaultsForm.chatModel || !defaultsForm.subagentProvider || !defaultsForm.subagentModel,
)

function addModel() {
  const val = modelToAdd.value.trim()
  if (val && !providerForm.models.includes(val)) {
    providerForm.models.push(val)
    providerFormRef.value?.validateField('models').catch(() => {})
  }
  modelToAdd.value = ''
}

function addImageModel() {
  const val = imageModelToAdd.value.trim()
  if (val && !providerForm.imageModels.includes(val)) {
    providerForm.imageModels.push(val)
    providerFormRef.value?.validateField('models').catch(() => {})
  }
  imageModelToAdd.value = ''
}

function removeImageModel(model: string) {
  providerForm.imageModels = providerForm.imageModels.filter(m => m !== model)
  providerFormRef.value?.validateField('models').catch(() => {})
}

function providerLabel(name: string) {
  return PROVIDER_LABELS[name] || name
}

function removeModel(model: string) {
  providerForm.models = providerForm.models.filter(m => m !== model)
  providerFormRef.value?.validateField('models').catch(() => {})
}

function resetProviderForm() {
  isEditingProvider.value = false
  editingProviderId.value = ''
  providerForm.name = ''
  providerForm.providerType = ''
  providerForm.apiKey = ''
  providerForm.models = []
  providerForm.imageModels = []
  providerForm.endpointUrl = ''
  providerFormRef.value?.clearValidate()
}

function handleEditProvider(row: ProviderConfig) {
  isEditingProvider.value = true
  editingProviderId.value = row.id
  providerForm.name = row.name
  providerForm.providerType = row.provider
  providerForm.apiKey = ''
  providerForm.models = [...row.models]
  providerForm.imageModels = [...row.image_models]
  providerForm.endpointUrl = row.endpoint_url || ''
  providerFormRef.value?.clearValidate()
}

function syncModelDefaultsForm() {
  const defaults = settingsStore.modelDefaults
  defaultsForm.chatProvider = defaults?.chat_provider_id ?? ''
  defaultsForm.chatModel = defaults?.chat_model ?? ''
  defaultsForm.subagentProvider = defaults?.subagent_provider_id ?? ''
  defaultsForm.subagentModel = defaults?.subagent_model ?? ''
  defaultsForm.imageProvider = defaults?.image_provider_id ?? ''
  defaultsForm.imageModel = defaults?.image_model ?? ''
}

function applyDefaultChange(target: 'chat' | 'subagent' | 'image', val: string[] | null) {
  if (val && val.length === 2) {
    if (target === 'chat') {
      defaultsForm.chatProvider = val[0]
      defaultsForm.chatModel = val[1]
      return
    }
    if (target === 'subagent') {
      defaultsForm.subagentProvider = val[0]
      defaultsForm.subagentModel = val[1]
      return
    }
    defaultsForm.imageProvider = val[0]
    defaultsForm.imageModel = val[1]
    return
  }

  if (target === 'chat') {
    defaultsForm.chatProvider = ''
    defaultsForm.chatModel = ''
    return
  }
  if (target === 'subagent') {
    defaultsForm.subagentProvider = ''
    defaultsForm.subagentModel = ''
    return
  }
  defaultsForm.imageProvider = ''
  defaultsForm.imageModel = ''
}

function handleChatDefaultChange(val: string[] | null) {
  applyDefaultChange('chat', val)
}

function handleSubagentDefaultChange(val: string[] | null) {
  applyDefaultChange('subagent', val)
}

function handleImageDefaultChange(val: string[] | null) {
  applyDefaultChange('image', val)
}

function resetPresetForm() {
  isEditingPreset.value = false
  editingPresetId.value = ''
  presetForm.name = ''
  presetForm.description = ''
  presetForm.content = ''
  presetForm.isDefault = false
  presetFormRef.value?.clearValidate()
}

function handleEditPreset(row: SystemPromptPreset) {
  isEditingPreset.value = true
  editingPresetId.value = row.id
  presetForm.name = row.name
  presetForm.description = row.description
  presetForm.content = row.content
  presetForm.isDefault = row.is_default
}

watch(() => settingsStore.modelDefaults, () => {
  syncModelDefaultsForm()
}, { deep: false })

onMounted(async () => {
  await Promise.all([
    settingsStore.loadProviders(),
    settingsStore.loadPresets(),
    settingsStore.loadModelDefaults(),
  ])
  syncModelDefaultsForm()
})

async function handleSaveProvider() {
  if (!providerFormRef.value) return
  const valid = await providerFormRef.value.validate().catch(() => false)
  if (!valid) return
  const apiKey = providerForm.apiKey || '__KEEP_EXISTING__'
  try {
    await settingsStore.saveProvider(
      isEditingProvider.value ? editingProviderId.value : undefined,
      providerForm.name,
      providerForm.providerType,
      apiKey,
      providerForm.endpointUrl || undefined,
      providerForm.models,
      undefined,
      providerForm.imageModels,
    )
    ElMessage.success(isEditingProvider.value ? t('settings.provider.messages.updated') : t('settings.provider.messages.saved'))
    resetProviderForm()
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || t('settings.provider.messages.saveFailed'))
  }
}

function toNullable(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

async function handleSaveModelDefaults() {
  if (!defaultsForm.chatProvider || !defaultsForm.chatModel) {
    ElMessage.error(t('settings.defaults.validation.chatRequired'))
    return
  }
  if (!defaultsForm.subagentProvider || !defaultsForm.subagentModel) {
    ElMessage.error(t('settings.defaults.validation.subagentRequired'))
    return
  }
  if ((defaultsForm.imageProvider && !defaultsForm.imageModel) || (!defaultsForm.imageProvider && defaultsForm.imageModel)) {
    ElMessage.error(t('settings.defaults.validation.imagePairRequired'))
    return
  }

  const payload: ModelDefaults = {
    chat_provider_id: toNullable(defaultsForm.chatProvider),
    chat_model: toNullable(defaultsForm.chatModel),
    subagent_provider_id: toNullable(defaultsForm.subagentProvider),
    subagent_model: toNullable(defaultsForm.subagentModel),
    image_provider_id: toNullable(defaultsForm.imageProvider),
    image_model: toNullable(defaultsForm.imageModel),
  }

  try {
    await settingsStore.saveModelDefaults(payload)
    syncModelDefaultsForm()
    ElMessage.success(t('settings.defaults.messages.saved'))
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || t('settings.defaults.messages.saveFailed'))
  }
}

async function handleDeleteProvider(id: string) {
  try {
    await ElMessageBox.confirm(t('settings.provider.confirmDelete'), t('common.confirm'), { type: 'warning' })
  } catch { return }
  try {
    await settingsStore.removeProvider(id)
    syncModelDefaultsForm()
    ElMessage.success(t('settings.provider.messages.deleted'))
  } catch {
    ElMessage.error(t('settings.provider.messages.deleteFailed'))
  }
}

async function handleSavePreset() {
  if (!presetFormRef.value) return
  const valid = await presetFormRef.value.validate().catch(() => false)
  if (!valid) return
  try {
    if (isEditingPreset.value) {
      await settingsStore.editPreset(editingPresetId.value, {
        name: presetForm.name,
        description: presetForm.description,
        content: presetForm.content,
        is_default: presetForm.isDefault,
      })
      ElMessage.success(t('settings.preset.messages.updated'))
    } else {
      await settingsStore.savePreset({
        name: presetForm.name,
        description: presetForm.description,
        content: presetForm.content,
        is_default: presetForm.isDefault,
      })
      ElMessage.success(t('settings.preset.messages.saved'))
    }
    resetPresetForm()
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || t('settings.preset.messages.saveFailed'))
  }
}

async function handleDeletePreset(id: string) {
  try {
    await ElMessageBox.confirm(t('settings.preset.confirmDelete'), t('common.confirm'), { type: 'warning' })
  } catch { return }
  try {
    await settingsStore.removePreset(id)
    ElMessage.success(t('settings.preset.messages.deleted'))
  } catch {
    ElMessage.error(t('settings.preset.messages.deleteFailed'))
  }
}
</script>

<style scoped>
.settings-layout {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-main);
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 24px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
}

.settings-header-left {
  display: flex;
  align-items: center;
  gap: 16px;
  min-width: 0;
}

.settings-header h3 {
  margin: 0;
  color: var(--text-primary);
}

.settings-content {
  flex: 1;
  overflow-y: auto;
  padding: 24px;
  max-width: 900px;
  margin: 0 auto;
  width: 100%;
}

.provider-form {
  max-width: 520px;
}

.model-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 8px;
}

.text-muted {
  color: var(--text-muted);
}

.defaults-desc {
  margin-top: 0;
  margin-bottom: 8px;
}

.row-actions {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  gap: 4px;
}

.row-actions :deep(.el-button) {
  white-space: nowrap;
}

@media (max-width: 768px) {
  .settings-header {
    padding: 10px 12px;
  }

  .settings-header-left {
    gap: 10px;
  }

  .settings-header h3 {
    font-size: 16px;
  }
}
</style>
