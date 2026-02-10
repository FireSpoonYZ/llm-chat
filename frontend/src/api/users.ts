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

export async function upsertProvider(provider: string, apiKey: string, endpointUrl?: string, modelName?: string, isDefault = false): Promise<ProviderConfig> {
  const { data } = await client.post<ProviderConfig>('/users/me/providers', {
    provider,
    api_key: apiKey,
    endpoint_url: endpointUrl || null,
    model_name: modelName || null,
    is_default: isDefault,
  })
  return data
}

export async function deleteProvider(provider: string): Promise<void> {
  await client.delete(`/users/me/providers/${provider}`)
}

export async function listMcpServers(): Promise<McpServer[]> {
  const { data } = await client.get<McpServer[]>('/mcp-servers')
  return data
}
