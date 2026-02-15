import axios from 'axios'
import client from './client'
import type { SharedConversation, MessagesResponse } from '../types'

// Unauthenticated client for public shared endpoints
const publicClient = axios.create({
  baseURL: '/api',
  headers: { 'Content-Type': 'application/json' },
})

export interface ShareResponse {
  share_token: string
  share_url: string
}

export async function createShare(conversationId: string): Promise<ShareResponse> {
  const { data } = await client.post<ShareResponse>(`/conversations/${conversationId}/share`)
  return data
}

export async function revokeShare(conversationId: string): Promise<void> {
  await client.delete(`/conversations/${conversationId}/share`)
}

export async function getSharedConversation(shareToken: string): Promise<SharedConversation> {
  const { data } = await publicClient.get<SharedConversation>(`/shared/${shareToken}`)
  return data
}

export async function getSharedMessages(shareToken: string, limit = 50, offset = 0): Promise<MessagesResponse> {
  const { data } = await publicClient.get<MessagesResponse>(`/shared/${shareToken}/messages`, {
    params: { limit, offset },
  })
  return data
}
