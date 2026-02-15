import { describe, it, expect, vi, beforeEach } from 'vitest'

const { mockPublicGet } = vi.hoisted(() => ({
  mockPublicGet: vi.fn(),
}))

// Mock the authenticated client
vi.mock('../../api/client', () => ({
  default: {
    post: vi.fn(),
    delete: vi.fn(),
  },
}))

// Mock axios.create to return a mock client for the public endpoints
vi.mock('axios', () => ({
  default: {
    create: () => ({
      get: mockPublicGet,
    }),
  },
}))

import client from '../../api/client'
import { createShare, revokeShare, getSharedConversation, getSharedMessages } from '../../api/sharing'

describe('sharing API - authenticated', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('createShare calls POST with correct URL', async () => {
    const mockResp = { data: { share_token: 'abc123', share_url: '/share/abc123' } }
    vi.mocked(client.post).mockResolvedValueOnce(mockResp)

    const result = await createShare('conv-1')

    expect(client.post).toHaveBeenCalledWith('/conversations/conv-1/share')
    expect(result.share_token).toBe('abc123')
  })

  it('revokeShare calls DELETE with correct URL', async () => {
    vi.mocked(client.delete).mockResolvedValueOnce({} as any)

    await revokeShare('conv-1')

    expect(client.delete).toHaveBeenCalledWith('/conversations/conv-1/share')
  })
})

describe('sharing API - public', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('getSharedConversation calls GET with token', async () => {
    const mockResp = { data: { title: 'Test', model_name: null, created_at: '', updated_at: '' } }
    mockPublicGet.mockResolvedValueOnce(mockResp)

    const result = await getSharedConversation('token123')

    expect(mockPublicGet).toHaveBeenCalledWith('/shared/token123')
    expect(result.title).toBe('Test')
  })

  it('getSharedMessages calls GET with pagination', async () => {
    const mockResp = { data: { messages: [], total: 0 } }
    mockPublicGet.mockResolvedValueOnce(mockResp)

    const result = await getSharedMessages('token123', 20, 10)

    expect(mockPublicGet).toHaveBeenCalledWith('/shared/token123/messages', {
      params: { limit: 20, offset: 10 },
    })
    expect(result.total).toBe(0)
  })
})
