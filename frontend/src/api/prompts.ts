import client from './client'
import type { SystemPromptPreset } from '../types'

export async function listPresets(): Promise<SystemPromptPreset[]> {
  const { data } = await client.get<SystemPromptPreset[]>('/presets')
  return data
}

export async function createPreset(payload: {
  name: string
  description?: string
  content: string
  is_default?: boolean
}): Promise<SystemPromptPreset> {
  const { data } = await client.post<SystemPromptPreset>('/presets', payload)
  return data
}

export async function updatePreset(
  id: string,
  payload: {
    name?: string
    description?: string
    content?: string
    is_default?: boolean
  },
): Promise<SystemPromptPreset> {
  const { data } = await client.put<SystemPromptPreset>(`/presets/${id}`, payload)
  return data
}

export async function deletePreset(id: string): Promise<void> {
  await client.delete(`/presets/${id}`)
}
