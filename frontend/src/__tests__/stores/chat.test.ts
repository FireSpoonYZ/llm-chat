import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useChatStore } from '../../stores/chat'

// Mock the conversations API
vi.mock('../../api/conversations', () => ({
  listConversations: vi.fn().mockResolvedValue([]),
  listMessages: vi.fn().mockResolvedValue({ messages: [], total: 0 }),
  createConversation: vi.fn(),
  deleteConversation: vi.fn(),
  updateConversation: vi.fn(),
}))

// Mock WebSocketManager
vi.mock('../../api/websocket', () => ({
  WebSocketManager: vi.fn().mockImplementation(() => ({
    connect: vi.fn(),
    disconnect: vi.fn(),
    send: vi.fn(),
    on: vi.fn(),
    off: vi.fn(),
  })),
}))

import * as convApi from '../../api/conversations'

function makeMessage(overrides: Record<string, unknown> = {}) {
  return {
    id: 'msg-1',
    role: 'user' as const,
    content: 'Hello',
    tool_calls: null,
    tool_call_id: null,
    token_count: null,
    created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('chat store - editMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('should update content and truncate messages after the edited one', () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.editMessage('msg-1', 'Updated hello')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Updated hello')
    expect(store.messages[0].id).toBe('msg-1')
    expect(store.isStreaming).toBe(true)
  })

  it('should not allow editing assistant messages', () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.editMessage('msg-2', 'Hacked')

    expect(store.messages).toHaveLength(2)
    expect(store.messages[1].content).toBe('Hi there')
  })

  it('should do nothing without a current conversation', () => {
    const store = useChatStore()
    store.currentConversationId = null
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
    ]

    store.editMessage('msg-1', 'Updated')

    expect(store.messages[0].content).toBe('Hello')
  })
})

describe('chat store - regenerateMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('should delete assistant message and subsequent messages', () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.regenerateMessage('msg-2')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].id).toBe('msg-1')
    expect(store.isStreaming).toBe(true)
  })

  it('should not allow regenerating user messages', () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.regenerateMessage('msg-1')

    expect(store.messages).toHaveLength(2)
    expect(store.isStreaming).toBe(false)
  })

  it('should set isStreaming to true after regeneration', () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.regenerateMessage('msg-2')

    expect(store.isStreaming).toBe(true)
  })
})

describe('chat store - handleMessagesTruncated', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('should truncate messages after the given message id', () => {
    const store = useChatStore()
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'More' }),
    ]

    store.handleMessagesTruncated('msg-1', 'Updated hello')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Updated hello')
  })

  it('should truncate without updating content when no updatedContent provided', () => {
    const store = useChatStore()
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi' }),
    ]

    store.handleMessagesTruncated('msg-1')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Hello')
  })
})

describe('chat store - createConversation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('should call API with title and system prompt only', async () => {
    const mockConv = {
      id: 'conv-1', title: 'Test', provider: null, model_name: null,
      system_prompt_override: 'Be helpful', created_at: '', updated_at: '',
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation('Test', 'Be helpful')
    expect(convApi.createConversation).toHaveBeenCalledWith('Test', 'Be helpful', undefined, undefined)
    expect(result.id).toBe('conv-1')
    expect(store.conversations).toHaveLength(1)
  })

  it('should pass provider and model to API', async () => {
    const mockConv = {
      id: 'conv-2', title: 'New Conversation', provider: 'openai', model_name: 'gpt-4o',
      system_prompt_override: null, created_at: '', updated_at: '',
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation(undefined, 'prompt', 'openai', 'gpt-4o')
    expect(convApi.createConversation).toHaveBeenCalledWith(undefined, 'prompt', 'openai', 'gpt-4o')
    expect(result.provider).toBe('openai')
    expect(result.model_name).toBe('gpt-4o')
  })

  it('should prepend new conversation to list', async () => {
    const mockConv = {
      id: 'conv-3', title: 'Third', provider: null, model_name: null,
      system_prompt_override: null, created_at: '', updated_at: '',
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    store.conversations = [
      { id: 'existing', title: 'Old', provider: null, model_name: null, system_prompt_override: null, created_at: '', updated_at: '' },
    ]
    await store.createConversation()
    expect(store.conversations).toHaveLength(2)
    expect(store.conversations[0].id).toBe('conv-3')
  })
})
