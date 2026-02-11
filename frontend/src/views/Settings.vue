<template>
  <div class="settings-layout">
    <header class="settings-header">
      <el-button @click="$router.push('/')">Back to Chat</el-button>
      <h3>Settings</h3>
    </header>
    <div class="settings-content">
      <el-tabs v-model="activeTab">
        <!-- Providers Tab -->
        <el-tab-pane label="Providers" name="providers">
          <el-table :data="settingsStore.providers" style="width: 100%">
            <el-table-column prop="name" label="Name" width="160" />
            <el-table-column label="API Type" width="120">
              <template #default="{ row }">
                {{ providerLabel(row.provider) }}
              </template>
            </el-table-column>
            <el-table-column label="Models">
              <template #default="{ row }">
                <el-tag
                  v-for="m in row.models"
                  :key="m"
                  size="small"
                  style="margin-right: 4px; margin-bottom: 2px"
                >{{ m }}</el-tag>
                <span v-if="!row.models.length" class="text-muted">No models</span>
              </template>
            </el-table-column>
            <el-table-column label="Default" width="80">
              <template #default="{ row }">
                <el-tag v-if="row.is_default" type="success" size="small">Default</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="Actions" width="140">
              <template #default="{ row }">
                <el-button text type="primary" @click="handleEditProvider(row)">Edit</el-button>
                <el-button text type="danger" @click="handleDeleteProvider(row.name)">Delete</el-button>
              </template>
            </el-table-column>
          </el-table>

          <el-divider />

          <h4>{{ isEditingProvider ? 'Edit Provider' : 'Add Provider' }}</h4>
          <el-form label-position="top" class="provider-form">
            <el-form-item label="Name">
              <el-input v-model="providerForm.name" placeholder="e.g. My OpenAI, Work Anthropic" />
            </el-form-item>
            <el-form-item label="API Type">
              <el-select v-model="providerForm.providerType" placeholder="Select API type">
                <el-option label="OpenAI" value="openai" />
                <el-option label="Anthropic" value="anthropic" />
                <el-option label="Google" value="google" />
                <el-option label="Mistral" value="mistral" />
              </el-select>
            </el-form-item>
            <el-form-item label="API Key">
              <el-input v-model="providerForm.apiKey" type="password" show-password :placeholder="isEditingProvider ? '(leave empty to keep current)' : 'sk-...'" />
            </el-form-item>
            <el-form-item label="Models">
              <div class="model-tags">
                <el-tag v-for="m in providerForm.models" :key="m" closable @close="removeModel(m)">{{ m }}</el-tag>
              </div>
              <el-select v-model="modelToAdd" placeholder="Add a model" filterable allow-create :disabled="!providerForm.providerType" style="width: 100%" @change="addModel">
                <el-option v-for="m in suggestedModels" :key="m" :label="m" :value="m" />
              </el-select>
            </el-form-item>
            <el-form-item label="Custom Endpoint (optional)">
              <el-input v-model="providerForm.endpointUrl" placeholder="https://..." />
            </el-form-item>
            <el-form-item>
              <el-checkbox v-model="providerForm.isDefault">Set as default</el-checkbox>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="handleSaveProvider">{{ isEditingProvider ? 'Update Provider' : 'Save Provider' }}</el-button>
              <el-button v-if="isEditingProvider" @click="resetProviderForm">Cancel</el-button>
            </el-form-item>
          </el-form>
        </el-tab-pane>

        <!-- System Prompts Tab -->
        <el-tab-pane label="System Prompts" name="presets">
          <el-table :data="settingsStore.presets" style="width: 100%">
            <el-table-column prop="name" label="Name" width="180" />
            <el-table-column label="Description">
              <template #default="{ row }">
                {{ row.description.length > 80 ? row.description.slice(0, 80) + '...' : row.description }}
              </template>
            </el-table-column>
            <el-table-column label="Default" width="80">
              <template #default="{ row }">
                <el-tag v-if="row.is_default" type="success" size="small">Default</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="Actions" width="140">
              <template #default="{ row }">
                <el-button text type="primary" @click="handleEditPreset(row)">Edit</el-button>
                <el-button text type="danger" @click="handleDeletePreset(row.id)">Delete</el-button>
              </template>
            </el-table-column>
          </el-table>

          <el-divider />

          <h4>{{ isEditingPreset ? 'Edit Preset' : 'Add Preset' }}</h4>
          <el-form label-position="top" class="provider-form">
            <el-form-item label="Name">
              <el-input v-model="presetForm.name" placeholder="Preset name" />
            </el-form-item>
            <el-form-item label="Description">
              <el-input v-model="presetForm.description" placeholder="Short description" />
            </el-form-item>
            <el-form-item label="Content">
              <el-input v-model="presetForm.content" type="textarea" :rows="12" placeholder="System prompt content" />
            </el-form-item>
            <el-form-item>
              <el-checkbox v-model="presetForm.isDefault">Set as default</el-checkbox>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="handleSavePreset">{{ isEditingPreset ? 'Update Preset' : 'Save Preset' }}</el-button>
              <el-button v-if="isEditingPreset" @click="resetPresetForm">Cancel</el-button>
            </el-form-item>
          </el-form>
        </el-tab-pane>
      </el-tabs>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref, computed, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '../stores/settings'
import { PROVIDER_MODELS, PROVIDER_LABELS } from '../constants/providers'
import type { ProviderConfig, SystemPromptPreset } from '../types'

const settingsStore = useSettingsStore()

const activeTab = ref('providers')
const isEditingProvider = ref(false)
const isEditingPreset = ref(false)
const editingPresetId = ref('')

const providerForm = reactive({
  name: '',
  providerType: '',
  apiKey: '',
  models: [] as string[],
  endpointUrl: '',
  isDefault: false,
})

const presetForm = reactive({
  name: '',
  description: '',
  content: '',
  isDefault: false,
})

const modelToAdd = ref('')

const suggestedModels = computed(() => {
  const all = PROVIDER_MODELS[providerForm.providerType] || []
  return all.filter(m => !providerForm.models.includes(m))
})

function providerLabel(name: string) {
  return PROVIDER_LABELS[name] || name
}

function addModel(val: string) {
  if (val && !providerForm.models.includes(val)) {
    providerForm.models.push(val)
  }
  modelToAdd.value = ''
}

function removeModel(model: string) {
  providerForm.models = providerForm.models.filter(m => m !== model)
}

watch(() => providerForm.providerType, (newVal, oldVal) => {
  if (!isEditingProvider.value && oldVal) {
    providerForm.models = []
  }
})

function resetProviderForm() {
  isEditingProvider.value = false
  providerForm.name = ''
  providerForm.providerType = ''
  providerForm.apiKey = ''
  providerForm.models = []
  providerForm.endpointUrl = ''
  providerForm.isDefault = false
}

function handleEditProvider(row: ProviderConfig) {
  isEditingProvider.value = true
  providerForm.name = row.name
  providerForm.providerType = row.provider
  providerForm.apiKey = ''
  providerForm.models = [...row.models]
  providerForm.endpointUrl = row.endpoint_url || ''
  providerForm.isDefault = row.is_default
}

function resetPresetForm() {
  isEditingPreset.value = false
  editingPresetId.value = ''
  presetForm.name = ''
  presetForm.description = ''
  presetForm.content = ''
  presetForm.isDefault = false
}

function handleEditPreset(row: SystemPromptPreset) {
  isEditingPreset.value = true
  editingPresetId.value = row.id
  presetForm.name = row.name
  presetForm.description = row.description
  presetForm.content = row.content
  presetForm.isDefault = row.is_default
}

onMounted(async () => {
  await settingsStore.loadProviders()
  await settingsStore.loadPresets()
})

async function handleSaveProvider() {
  if (!providerForm.name.trim()) {
    ElMessage.warning('Name is required')
    return
  }
  if (!providerForm.providerType) {
    ElMessage.warning('API type is required')
    return
  }
  if (!isEditingProvider.value && !providerForm.apiKey) {
    ElMessage.warning('API key is required')
    return
  }
  if (!providerForm.models.length) {
    ElMessage.warning('Add at least one model')
    return
  }
  const apiKey = providerForm.apiKey || '__KEEP_EXISTING__'
  try {
    await settingsStore.saveProvider(
      providerForm.name,
      providerForm.providerType,
      apiKey,
      providerForm.endpointUrl || undefined,
      providerForm.models,
      providerForm.isDefault,
    )
    ElMessage.success(isEditingProvider.value ? 'Provider updated' : 'Provider saved')
    resetProviderForm()
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || 'Failed to save provider')
  }
}

async function handleDeleteProvider(name: string) {
  try {
    await settingsStore.removeProvider(name)
    ElMessage.success('Provider deleted')
  } catch {
    ElMessage.error('Failed to delete provider')
  }
}

async function handleSavePreset() {
  if (!presetForm.name.trim()) {
    ElMessage.warning('Name is required')
    return
  }
  if (!presetForm.content.trim()) {
    ElMessage.warning('Content is required')
    return
  }
  try {
    if (isEditingPreset.value) {
      await settingsStore.editPreset(editingPresetId.value, {
        name: presetForm.name,
        description: presetForm.description,
        content: presetForm.content,
        is_default: presetForm.isDefault,
      })
      ElMessage.success('Preset updated')
    } else {
      await settingsStore.savePreset({
        name: presetForm.name,
        description: presetForm.description,
        content: presetForm.content,
        is_default: presetForm.isDefault,
      })
      ElMessage.success('Preset saved')
    }
    resetPresetForm()
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || 'Failed to save preset')
  }
}

async function handleDeletePreset(id: string) {
  try {
    await settingsStore.removePreset(id)
    ElMessage.success('Preset deleted')
  } catch {
    ElMessage.error('Failed to delete preset')
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
  gap: 16px;
  padding: 12px 24px;
  border-bottom: 1px solid var(--border-light);
  flex-shrink: 0;
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
  max-width: 500px;
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
</style>