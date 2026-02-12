import client from './client'
import type { Conversation, MessagesResponse, McpServer, ListFilesResponse } from '../types'

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

export async function listFiles(id: string, path = '', recursive = false): Promise<ListFilesResponse> {
  const { data } = await client.get<ListFilesResponse>(`/conversations/${id}/files`, {
    params: { path, ...(recursive ? { recursive: true } : {}) },
  })
  return data
}

function triggerBlobDownload(blob: Blob, filename: string) {
  const url = window.URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  window.URL.revokeObjectURL(url)
  document.body.removeChild(a)
}

function extractFilename(headers: Record<string, unknown>, fallback: string): string {
  const disposition = String(headers['content-disposition'] || '')
  const match = disposition.match(/filename="?([^";\n]+)"?/)
  return match ? match[1] : fallback
}

export async function downloadFile(id: string, path: string): Promise<void> {
  const response = await client.get(`/conversations/${id}/files/download`, {
    params: { path },
    responseType: 'blob',
  })
  const fallback = path.split('/').pop() || 'download'
  const filename = extractFilename(response.headers, fallback)
  triggerBlobDownload(new Blob([response.data]), filename)
}

export async function downloadBatch(id: string, paths: string[]): Promise<void> {
  const response = await client.post(`/conversations/${id}/files/download-batch`, { paths }, {
    responseType: 'blob',
  })
  const filename = extractFilename(response.headers, 'download.zip')
  triggerBlobDownload(new Blob([response.data]), filename)
}
