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

  async function saveProvider(name: string, providerType: string, apiKey: string, endpointUrl?: string, models: string[] = [], isDefault = false) {
    await usersApi.upsertProvider(name, providerType, apiKey, endpointUrl, models, isDefault)
    await loadProviders()
  }

  async function removeProvider(name: string) {
    await usersApi.deleteProvider(name)
    await loadProviders()
  }

  async function loadMcpServers() {
    mcpServers.value = await usersApi.listMcpServers()
  }

  return { providers, mcpServers, loadProviders, saveProvider, removeProvider, loadMcpServers }
})
