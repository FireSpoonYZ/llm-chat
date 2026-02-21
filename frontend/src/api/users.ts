import client from './client'
import type { ProviderConfig, McpServer, ModelDefaults, User } from '../types'

export async function getProfile(): Promise<User> {
  const { data } = await client.get<User>('/users/me')
  return data
}

export async function listProviders(): Promise<ProviderConfig[]> {
  const { data } = await client.get<ProviderConfig[]>('/users/me/providers')
  return data
}

export async function upsertProvider(
  id: string | undefined,
  name: string,
  providerType: string,
  apiKey: string,
  endpointUrl?: string,
  models: string[] = [],
  isDefault?: boolean,
  imageModels: string[] = [],
): Promise<ProviderConfig> {
  const payload: Record<string, unknown> = {
    id,
    name,
    provider_type: providerType,
    api_key: apiKey,
    endpoint_url: endpointUrl || null,
    models,
    image_models: imageModels,
  }
  if (typeof isDefault === 'boolean') {
    payload.is_default = isDefault
  }
  const { data } = await client.post<ProviderConfig>('/users/me/providers', payload)
  return data
}

export async function deleteProvider(id: string): Promise<void> {
  await client.delete(`/users/me/providers/${encodeURIComponent(id)}`)
}

export async function listMcpServers(): Promise<McpServer[]> {
  const { data } = await client.get<McpServer[]>('/mcp-servers')
  return data
}

export async function getModelDefaults(): Promise<ModelDefaults> {
  const { data } = await client.get<ModelDefaults>('/users/me/model-defaults')
  return data
}

export async function updateModelDefaults(payload: ModelDefaults): Promise<ModelDefaults> {
  const { data } = await client.put<ModelDefaults>('/users/me/model-defaults', payload)
  return data
}
