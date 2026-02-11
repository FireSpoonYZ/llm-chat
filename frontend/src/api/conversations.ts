import client from './client'
import type { Conversation, MessagesResponse, McpServer } from '../types'

export async function listConversations(): Promise<Conversation[]> {
  const { data } = await client.get<Conversation[]>('/conversations')
  return data
}

export async function createConversation(title?: string, systemPromptOverride?: string, provider?: string, modelName?: string): Promise<Conversation> {
  const { data } = await client.post<Conversation>('/conversations', {
    title,
    system_prompt_override: systemPromptOverride || undefined,
    provider: provider || undefined,
    model_name: modelName || undefined,
  })
  return data
}

export async function getConversation(id: string): Promise<Conversation> {
  const { data } = await client.get<Conversation>(`/conversations/${id}`)
  return data
}

export async function updateConversation(id: string, updates: Partial<Conversation>): Promise<Conversation> {
  const { data } = await client.put<Conversation>(`/conversations/${id}`, updates)
  return data
}

export async function deleteConversation(id: string): Promise<void> {
  await client.delete(`/conversations/${id}`)
}

export async function listMessages(id: string, limit = 50, offset = 0): Promise<MessagesResponse> {
  const { data } = await client.get<MessagesResponse>(`/conversations/${id}/messages`, { params: { limit, offset } })
  return data
}

export async function getConversationMcpServers(id: string): Promise<McpServer[]> {
  const { data } = await client.get<McpServer[]>(`/conversations/${id}/mcp-servers`)
  return data
}

export async function setConversationMcpServers(id: string, serverIds: string[]): Promise<void> {
  await client.put(`/conversations/${id}/mcp-servers`, { server_ids: serverIds })
}
