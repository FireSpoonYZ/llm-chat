import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { ModelDefaults, ProviderConfig, McpServer, SystemPromptPreset } from '../types'
import * as usersApi from '../api/users'
import * as presetsApi from '../api/prompts'

export const useSettingsStore = defineStore('settings', () => {
  const providers = ref<ProviderConfig[]>([])
  const mcpServers = ref<McpServer[]>([])
  const presets = ref<SystemPromptPreset[]>([])
  const modelDefaults = ref<ModelDefaults | null>(null)

  const defaultPreset = computed(() =>
    presets.value.find(p => p.is_default) || presets.value[0],
  )

  async function loadProviders() {
    providers.value = await usersApi.listProviders()
  }

  async function saveProvider(
    id: string | undefined,
    name: string,
    providerType: string,
    apiKey: string,
    endpointUrl?: string,
    models: string[] = [],
    isDefault?: boolean,
    imageModels: string[] = [],
  ) {
    await usersApi.upsertProvider(id, name, providerType, apiKey, endpointUrl, models, isDefault, imageModels)
    await Promise.all([loadProviders(), loadModelDefaults()])
  }

  async function removeProvider(id: string) {
    await usersApi.deleteProvider(id)
    await Promise.all([loadProviders(), loadModelDefaults()])
  }

  async function loadModelDefaults() {
    modelDefaults.value = await usersApi.getModelDefaults()
  }

  async function saveModelDefaults(payload: ModelDefaults) {
    modelDefaults.value = await usersApi.updateModelDefaults(payload)
  }

  async function loadMcpServers() {
    mcpServers.value = await usersApi.listMcpServers()
  }

  async function loadPresets() {
    presets.value = await presetsApi.listPresets()
  }

  async function savePreset(payload: {
    name: string
    description?: string
    content: string
    is_default?: boolean
  }) {
    await presetsApi.createPreset(payload)
    await loadPresets()
  }

  async function editPreset(
    id: string,
    payload: {
      name?: string
      description?: string
      content?: string
      is_default?: boolean
    },
  ) {
    await presetsApi.updatePreset(id, payload)
    await loadPresets()
  }

  async function removePreset(id: string) {
    await presetsApi.deletePreset(id)
    await loadPresets()
  }

  return {
    providers, mcpServers, presets, defaultPreset, modelDefaults,
    loadProviders, saveProvider, removeProvider,
    loadModelDefaults, saveModelDefaults,
    loadMcpServers, loadPresets, savePreset, editPreset, removePreset,
  }
})
