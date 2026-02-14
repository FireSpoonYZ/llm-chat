import client from './client'
import type { ProviderConfig, McpServer } from '../types'

export async function getProfile() {
  const { data } = await client.get('/users/me')
  return data
}

export async function listProviders(): Promise<ProviderConfig[]> {
  const { data } = await client.get<ProviderConfig[]>('/users/me/providers')
  return data
}

export async function upsertProvider(name: string, providerType: string, apiKey: string, endpointUrl?: string, models: string[] = [], isDefault = false, imageModels: string[] = []): Promise<ProviderConfig> {
  const { data } = await client.post<ProviderConfig>('/users/me/providers', {
    name,
    provider_type: providerType,
    api_key: apiKey,
    endpoint_url: endpointUrl || null,
    models,
    image_models: imageModels,
    is_default: isDefault,
  })
  return data
}

export async function deleteProvider(name: string): Promise<void> {
  await client.delete(`/users/me/providers/${encodeURIComponent(name)}`)
}

export async function listMcpServers(): Promise<McpServer[]> {
  const { data } = await client.get<McpServer[]>('/mcp-servers')
  return data
}
