import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { ProviderConfig, McpServer } from '../types'
import * as usersApi from '../api/users'

export const useSettingsStore = defineStore('settings', () => {
  const providers = ref<ProviderConfig[]>([])
  const mcpServers = ref<McpServer[]>([])

  async function loadProviders() {
    providers.value = await usersApi.listProviders()
  }

  async function saveProvider(provider: string, apiKey: string, endpointUrl?: string, modelName?: string, isDefault = false) {
    await usersApi.upsertProvider(provider, apiKey, endpointUrl, modelName, isDefault)
    await loadProviders()
  }

  async function removeProvider(provider: string) {
    await usersApi.deleteProvider(provider)
    await loadProviders()
  }

  async function loadMcpServers() {
    mcpServers.value = await usersApi.listMcpServers()
  }

  return { providers, mcpServers, loadProviders, saveProvider, removeProvider, loadMcpServers }
})
