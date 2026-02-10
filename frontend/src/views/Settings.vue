<template>
  <el-container style="height: 100vh">
    <el-header style="display: flex; align-items: center; border-bottom: 1px solid #e4e7ed">
      <el-button @click="$router.push('/')">Back to Chat</el-button>
      <h3 style="margin-left: 16px">Settings</h3>
    </el-header>
    <el-main>
      <el-card>
        <template #header><h4>AI Providers</h4></template>
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
              <span v-if="!row.models.length" style="color: #909399">No models</span>
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

        <h4>{{ isEditing ? 'Edit Provider' : 'Add Provider' }}</h4>
        <el-form label-position="top" style="max-width: 500px">
          <el-form-item label="Name">
            <el-input v-model="form.name" placeholder="e.g. My OpenAI, Work Anthropic" />
          </el-form-item>
          <el-form-item label="API Type">
            <el-select v-model="form.providerType" placeholder="Select API type">
              <el-option label="OpenAI" value="openai" />
              <el-option label="Anthropic" value="anthropic" />
              <el-option label="Google" value="google" />
              <el-option label="Mistral" value="mistral" />
            </el-select>
          </el-form-item>
          <el-form-item label="API Key">
            <el-input v-model="form.apiKey" type="password" show-password :placeholder="isEditing ? '(leave empty to keep current)' : 'sk-...'" />
          </el-form-item>
          <el-form-item label="Models">
            <div style="display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 8px">
              <el-tag
                v-for="m in form.models"
                :key="m"
                closable
                @close="removeModel(m)"
              >{{ m }}</el-tag>
            </div>
            <el-select
              v-model="modelToAdd"
              placeholder="Add a model"
              filterable
              allow-create
              :disabled="!form.providerType"
              style="width: 100%"
              @change="addModel"
            >
              <el-option
                v-for="m in suggestedModels"
                :key="m"
                :label="m"
                :value="m"
              />
            </el-select>
          </el-form-item>
          <el-form-item label="Custom Endpoint (optional)">
            <el-input v-model="form.endpointUrl" placeholder="https://..." />
          </el-form-item>
          <el-form-item>
            <el-checkbox v-model="form.isDefault">Set as default</el-checkbox>
          </el-form-item>
          <el-form-item>
            <el-button type="primary" @click="handleSaveProvider">{{ isEditing ? 'Update Provider' : 'Save Provider' }}</el-button>
            <el-button v-if="isEditing" @click="resetForm">Cancel</el-button>
          </el-form-item>
        </el-form>
      </el-card>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref, computed, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '../stores/settings'
import { PROVIDER_MODELS, PROVIDER_LABELS } from '../constants/providers'
import type { ProviderConfig } from '../types'

const settingsStore = useSettingsStore()

const isEditing = ref(false)

const form = reactive({
  name: '',
  providerType: '',
  apiKey: '',
  models: [] as string[],
  endpointUrl: '',
  isDefault: false,
})

const modelToAdd = ref('')

const suggestedModels = computed(() => {
  const all = PROVIDER_MODELS[form.providerType] || []
  return all.filter(m => !form.models.includes(m))
})

function providerLabel(name: string) {
  return PROVIDER_LABELS[name] || name
}

function addModel(val: string) {
  if (val && !form.models.includes(val)) {
    form.models.push(val)
  }
  modelToAdd.value = ''
}

function removeModel(model: string) {
  form.models = form.models.filter(m => m !== model)
}

watch(() => form.providerType, (newVal, oldVal) => {
  if (!isEditing.value && oldVal) {
    form.models = []
  }
})

function resetForm() {
  isEditing.value = false
  form.name = ''
  form.providerType = ''
  form.apiKey = ''
  form.models = []
  form.endpointUrl = ''
  form.isDefault = false
}

function handleEditProvider(row: ProviderConfig) {
  isEditing.value = true
  form.name = row.name
  form.providerType = row.provider
  form.apiKey = ''
  form.models = [...row.models]
  form.endpointUrl = row.endpoint_url || ''
  form.isDefault = row.is_default
}

onMounted(async () => {
  await settingsStore.loadProviders()
})

async function handleSaveProvider() {
  if (!form.name.trim()) {
    ElMessage.warning('Name is required')
    return
  }
  if (!form.providerType) {
    ElMessage.warning('API type is required')
    return
  }
  if (!isEditing.value && !form.apiKey) {
    ElMessage.warning('API key is required')
    return
  }
  if (!form.models.length) {
    ElMessage.warning('Add at least one model')
    return
  }
  // When editing without new key, use a placeholder that backend will handle
  const apiKey = form.apiKey || '__KEEP_EXISTING__'
  try {
    await settingsStore.saveProvider(
      form.name,
      form.providerType,
      apiKey,
      form.endpointUrl || undefined,
      form.models,
      form.isDefault,
    )
    ElMessage.success(isEditing.value ? 'Provider updated' : 'Provider saved')
    resetForm()
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
</script>
