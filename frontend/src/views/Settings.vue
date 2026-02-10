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
          <el-table-column prop="provider" label="Provider" />
          <el-table-column prop="model_name" label="Model" />
          <el-table-column label="Default">
            <template #default="{ row }">
              <el-tag v-if="row.is_default" type="success">Default</el-tag>
            </template>
          </el-table-column>
          <el-table-column label="Actions">
            <template #default="{ row }">
              <el-button text type="danger" @click="handleDeleteProvider(row.provider)">Delete</el-button>
            </template>
          </el-table-column>
        </el-table>

        <el-divider />

        <h4>Add Provider</h4>
        <el-form label-position="top" style="max-width: 500px">
          <el-form-item label="Provider">
            <el-select v-model="newProvider.provider" placeholder="Select provider">
              <el-option label="OpenAI" value="openai" />
              <el-option label="Anthropic" value="anthropic" />
              <el-option label="Google" value="google" />
              <el-option label="Mistral" value="mistral" />
            </el-select>
          </el-form-item>
          <el-form-item label="API Key">
            <el-input v-model="newProvider.apiKey" type="password" show-password placeholder="sk-..." />
          </el-form-item>
          <el-form-item label="Model Name (optional)">
            <el-input v-model="newProvider.modelName" placeholder="e.g. gpt-4o, claude-sonnet-4-20250514" />
          </el-form-item>
          <el-form-item label="Custom Endpoint (optional)">
            <el-input v-model="newProvider.endpointUrl" placeholder="https://..." />
          </el-form-item>
          <el-form-item>
            <el-checkbox v-model="newProvider.isDefault">Set as default</el-checkbox>
          </el-form-item>
          <el-form-item>
            <el-button type="primary" @click="handleSaveProvider">Save Provider</el-button>
          </el-form-item>
        </el-form>
      </el-card>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, reactive } from 'vue'
import { ElMessage } from 'element-plus'
import { useSettingsStore } from '../stores/settings'

const settingsStore = useSettingsStore()

const newProvider = reactive({
  provider: '',
  apiKey: '',
  modelName: '',
  endpointUrl: '',
  isDefault: false,
})

onMounted(async () => {
  await settingsStore.loadProviders()
})

async function handleSaveProvider() {
  if (!newProvider.provider || !newProvider.apiKey) {
    ElMessage.warning('Provider and API key are required')
    return
  }
  try {
    await settingsStore.saveProvider(
      newProvider.provider,
      newProvider.apiKey,
      newProvider.endpointUrl || undefined,
      newProvider.modelName || undefined,
      newProvider.isDefault,
    )
    ElMessage.success('Provider saved')
    newProvider.provider = ''
    newProvider.apiKey = ''
    newProvider.modelName = ''
    newProvider.endpointUrl = ''
    newProvider.isDefault = false
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || 'Failed to save provider')
  }
}

async function handleDeleteProvider(provider: string) {
  try {
    await settingsStore.removeProvider(provider)
    ElMessage.success('Provider deleted')
  } catch {
    ElMessage.error('Failed to delete provider')
  }
}
</script>
