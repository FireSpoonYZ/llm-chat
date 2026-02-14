import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { ProviderConfig, McpServer, SystemPromptPreset } from '../types'
import * as usersApi from '../api/users'
import * as presetsApi from '../api/prompts'

export const useSettingsStore = defineStore('settings', () => {
  const providers = ref<ProviderConfig[]>([])
  const mcpServers = ref<McpServer[]>([])
  const presets = ref<SystemPromptPreset[]>([])

  const defaultPreset = computed(() =>
    presets.value.find(p => p.is_default) || presets.value[0],
  )

  async function loadProviders() {
    providers.value = await usersApi.listProviders()
  }

  async function saveProvider(name: string, providerType: string, apiKey: string, endpointUrl?: string, models: string[] = [], isDefault = false, imageModels: string[] = []) {
    await usersApi.upsertProvider(name, providerType, apiKey, endpointUrl, models, isDefault, imageModels)
    await loadProviders()
  }

  async function removeProvider(name: string) {
    await usersApi.deleteProvider(name)
    await loadProviders()
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
    providers, mcpServers, presets, defaultPreset,
    loadProviders, saveProvider, removeProvider,
    loadMcpServers, loadPresets, savePreset, editPreset, removePreset,
  }
})
