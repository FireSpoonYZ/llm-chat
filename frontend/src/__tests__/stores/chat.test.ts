import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { mount } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import { useChatStore } from '../../stores/chat'
import ChatMessage from '../../components/ChatMessage.vue'
import type { ContentBlock } from '../../types'

// Mock the conversations API
vi.mock('../../api/conversations', () => ({
  listConversations: vi.fn().mockResolvedValue([]),
  listMessages: vi.fn().mockResolvedValue({ messages: [], total: 0 }),
  createConversation: vi.fn(),
  deleteConversation: vi.fn(),
  updateConversation: vi.fn(),
}))

// Mock auth
vi.mock('../../api/auth', () => ({
  refreshSession: vi.fn().mockResolvedValue(true),
}))

// Mock WebSocketManager
const mockWsInstances: any[] = []
vi.mock('../../api/websocket', () => ({
  WebSocketManager: class {
    connect = vi.fn()
    disconnect = vi.fn()
    send = vi.fn().mockReturnValue(true)
    on = vi.fn()
    off = vi.fn()
    constructor(_url: string, _sessionRefresher?: () => Promise<boolean>) { mockWsInstances.push(this) }
  },
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

// Helper to extract WS event handlers from mock
function getWsHandler(mockWs: any, event: string): ((...args: any[]) => void) | undefined {
  const call = mockWs.on.mock.calls.find((c: any[]) => c[0] === event)
  return call ? call[1] : undefined
}

describe('chat store - complete handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('should prefer backend tool_calls over local streamingBlocks', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]

    // Simulate streaming state
    store.isStreaming = true
    store.streamingBlocks = [
      { type: 'text', content: 'local text' },
    ]

    const backendBlocks = [
      { type: 'thinking', content: 'I thought about it' },
      { type: 'text', content: 'Here is the answer' },
      { type: 'tool_call', id: 'tc-1', name: 'search', input: {}, result: 'ok', isError: false },
    ]

    const handler = getWsHandler(mockWs, 'complete')!
    handler({
      type: 'complete',
      message_id: 'msg-1',
      content: 'Here is the answer',
      tool_calls: backendBlocks,
    })

    expect(store.messages).toHaveLength(1)
    const msg = store.messages[0]
    expect(msg.tool_calls).toBe(JSON.stringify(backendBlocks))
    expect(store.isStreaming).toBe(false)
    expect(store.streamingBlocks).toHaveLength(0)
  })

  it('should fall back to streamingBlocks when backend sends no tool_calls', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]

    const localBlocks = [
      { type: 'thinking' as const, content: 'hmm' },
      { type: 'text' as const, content: 'answer' },
    ]
    store.isStreaming = true
    store.streamingBlocks = [...localBlocks]

    const handler = getWsHandler(mockWs, 'complete')!
    handler({
      type: 'complete',
      message_id: 'msg-2',
      content: 'answer',
    })

    expect(store.messages).toHaveLength(1)
    const parsed = JSON.parse(store.messages[0].tool_calls!)
    expect(parsed).toHaveLength(2)
    expect(parsed[0].type).toBe('thinking')
  })

  it('should set tool_calls to null when no rich blocks exist', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]

    store.isStreaming = true
    store.streamingBlocks = [
      { type: 'text' as const, content: 'just text' },
    ]

    const handler = getWsHandler(mockWs, 'complete')!
    handler({
      type: 'complete',
      message_id: 'msg-3',
      content: 'just text',
    })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].tool_calls).toBeNull()
    expect(store.messages[0].content).toBe('just text')
  })
})

describe('chat store - editMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('waits for backend confirmation instead of optimistic update', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.editMessage('msg-1', 'Updated hello')

    expect(store.messages).toHaveLength(3)
    expect(store.messages[0].content).toBe('Hello')
    expect(store.isWaiting).toBe(true)
    expect(store.isStreaming).toBe(false)
    expect(mockWs.send).toHaveBeenCalledWith({
      type: 'edit_message',
      message_id: 'msg-1',
      content: 'Updated hello',
    })
  })

  it('submits edit even when content is unchanged', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Same content' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.editMessage('msg-1', 'Same content')

    expect(store.isWaiting).toBe(true)
    expect(mockWs.send).toHaveBeenCalledWith({
      type: 'edit_message',
      message_id: 'msg-1',
      content: 'Same content',
    })
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

  it('should not edit pending user messages', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'pending-123', role: 'user', content: 'Hello' }),
    ]

    store.editMessage('pending-123', 'Updated')

    expect(mockWs.send).not.toHaveBeenCalledWith({
      type: 'edit_message',
      message_id: 'pending-123',
      content: 'Updated',
    })
    expect(store.messages[0].content).toBe('Hello')
    expect(store.isWaiting).toBe(false)
  })
})

describe('chat store - regenerateMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('waits for backend confirmation instead of optimistic delete', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.regenerateMessage('msg-2')

    expect(store.messages).toHaveLength(3)
    expect(store.isWaiting).toBe(true)
    expect(store.isStreaming).toBe(false)
    expect(mockWs.send).toHaveBeenCalledWith({
      type: 'regenerate',
      message_id: 'msg-2',
    })
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
    store.connectWs()
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.regenerateMessage('msg-2')

    expect(store.isStreaming).toBe(false)
    expect(store.isWaiting).toBe(true)
  })
})

describe('chat store - handleMessagesTruncated', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
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

  it('should update parts and content together when updated_parts is provided', () => {
    const store = useChatStore()
    store.messages = [
      makeMessage({
        id: 'msg-1',
        role: 'user',
        content: 'Old content',
        parts: [
          { seq: 0, type: 'text', text: 'Old content', json_payload: null, tool_call_id: null },
        ],
      }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi' }),
    ]

    store.handleMessagesTruncated(
      'msg-1',
      'Updated via parts',
      [
        { seq: 0, type: 'text', text: 'Updated via parts', json_payload: null, tool_call_id: null },
      ],
    )

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Updated via parts')
    expect(store.messages[0].parts).toEqual([
      { seq: 0, type: 'text', text: 'Updated via parts', json_payload: null, tool_call_id: null },
    ])
  })

  it('reloads server messages when local truncate target is missing', async () => {
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' })]
    vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: [], total: 0 })

    store.handleMessagesTruncated('missing-id', 'Updated')
    await Promise.resolve()

    expect(convApi.listMessages).toHaveBeenCalledWith('conv-1')
  })
})

describe('chat store - mutation confirmation flow', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
    vi.clearAllMocks()
  })

  it('applies edit only after messages_truncated confirmation', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.editMessage('msg-1', 'Updated hello')
    expect(store.messages).toHaveLength(3)
    expect(store.isWaiting).toBe(true)

    truncateHandler({
      type: 'messages_truncated',
      after_message_id: 'msg-1',
      updated_content: 'Updated hello',
    })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Updated hello')
    expect(store.isWaiting).toBe(true)
  })

  it('applies unchanged edit after messages_truncated confirmation', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Same content' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.editMessage('msg-1', 'Same content')
    expect(store.isWaiting).toBe(true)

    truncateHandler({
      type: 'messages_truncated',
      after_message_id: 'msg-1',
      updated_content: 'Same content',
    })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('Same content')
    expect(store.isWaiting).toBe(true)
  })

  it('applies regenerate only after messages_truncated confirmation', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      makeMessage({ id: 'msg-3', role: 'user', content: 'Follow up' }),
    ]

    store.regenerateMessage('msg-2')
    expect(store.messages).toHaveLength(3)
    expect(store.isWaiting).toBe(true)

    truncateHandler({
      type: 'messages_truncated',
      after_message_id: 'msg-1',
    })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].id).toBe('msg-1')
    expect(store.isWaiting).toBe(true)
  })

  it('clears waiting and reloads messages on mutation error', async () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const errorHandler = getWsHandler(mockWs, 'error')!
    const reloaded = [makeMessage({ id: 'server-1', role: 'user', content: 'Server state' })]
    vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: reloaded, total: 1 })
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.editMessage('msg-1', 'Updated')
    expect(store.isWaiting).toBe(true)

    errorHandler({
      type: 'error',
      code: 'invalid_message',
      message: 'Message not found',
    })
    await Promise.resolve()

    expect(store.isWaiting).toBe(false)
    expect(store.messages).toEqual(reloaded)
    expect(convApi.listMessages).toHaveBeenCalledWith('conv-1')
  })

  it('clears waiting and reloads messages on unchanged-edit error', async () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const errorHandler = getWsHandler(mockWs, 'error')!
    const reloaded = [makeMessage({ id: 'server-1', role: 'user', content: 'Server state' })]
    vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: reloaded, total: 1 })
    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Same content' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.editMessage('msg-1', 'Same content')
    expect(store.isWaiting).toBe(true)

    errorHandler({
      type: 'error',
      code: 'edit_failed',
      message: 'Failed to apply edit. Please try again.',
    })
    await Promise.resolve()

    expect(store.isWaiting).toBe(false)
    expect(store.messages).toEqual(reloaded)
    expect(convApi.listMessages).toHaveBeenCalledWith('conv-1')
  })

  it('stops waiting when no response starts after truncate confirmation', async () => {
    vi.useFakeTimers()
    try {
      const store = useChatStore()
      store.connectWs()
      const mockWs = mockWsInstances[mockWsInstances.length - 1]
      const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
      const reloaded = [makeMessage({ id: 'server-1', role: 'user', content: 'Server state' })]
      vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: reloaded, total: 1 })
      store.currentConversationId = 'conv-1'
      store.messages = [
        makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
        makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      ]

      store.editMessage('msg-1', 'Updated')
      truncateHandler({
        type: 'messages_truncated',
        after_message_id: 'msg-1',
        updated_content: 'Updated',
      })
      expect(store.isWaiting).toBe(true)

      await vi.advanceTimersByTimeAsync(20000)
      await Promise.resolve()

      expect(store.isWaiting).toBe(false)
      expect(convApi.listMessages).toHaveBeenCalledWith('conv-1')
      expect(store.messages).toEqual(reloaded)
    } finally {
      vi.useRealTimers()
    }
  })

  it('still times out if only container connected arrives after truncate confirmation', async () => {
    vi.useFakeTimers()
    try {
      const store = useChatStore()
      store.connectWs()
      const mockWs = mockWsInstances[mockWsInstances.length - 1]
      const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
      const containerStatusHandler = getWsHandler(mockWs, 'container_status')!
      const reloaded = [makeMessage({ id: 'server-2', role: 'user', content: 'Synced state' })]
      vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: reloaded, total: 1 })
      store.currentConversationId = 'conv-1'
      store.messages = [
        makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
        makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      ]

      store.editMessage('msg-1', 'Updated')
      truncateHandler({
        type: 'messages_truncated',
        after_message_id: 'msg-1',
        updated_content: 'Updated',
      })
      containerStatusHandler({
        type: 'container_status',
        status: 'connected',
      })

      await vi.advanceTimersByTimeAsync(20000)
      await Promise.resolve()

      expect(store.isWaiting).toBe(false)
      expect(convApi.listMessages).toHaveBeenCalledWith('conv-1')
      expect(store.messages).toEqual(reloaded)
    } finally {
      vi.useRealTimers()
    }
  })

  it('cancels timeout when first follow-up event is tool_result', async () => {
    vi.useFakeTimers()
    try {
      const store = useChatStore()
      store.connectWs()
      const mockWs = mockWsInstances[mockWsInstances.length - 1]
      const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
      const toolResultHandler = getWsHandler(mockWs, 'tool_result')!
      store.currentConversationId = 'conv-1'
      store.messages = [
        makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
        makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      ]

      store.editMessage('msg-1', 'Updated')
      truncateHandler({
        type: 'messages_truncated',
        after_message_id: 'msg-1',
        updated_content: 'Updated',
      })
      toolResultHandler({
        type: 'tool_result',
        tool_call_id: 'tc-1',
        is_error: false,
        result: { kind: 'text', text: 'ok', success: true },
      })

      await vi.advanceTimersByTimeAsync(20000)

      expect(convApi.listMessages).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it('cancels timeout when first follow-up event is question', async () => {
    vi.useFakeTimers()
    try {
      const store = useChatStore()
      store.connectWs()
      const mockWs = mockWsInstances[mockWsInstances.length - 1]
      const truncateHandler = getWsHandler(mockWs, 'messages_truncated')!
      const questionHandler = getWsHandler(mockWs, 'question')!
      store.currentConversationId = 'conv-1'
      store.messages = [
        makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
        makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
      ]

      store.editMessage('msg-1', 'Updated')
      truncateHandler({
        type: 'messages_truncated',
        after_message_id: 'msg-1',
        updated_content: 'Updated',
      })
      questionHandler({
        type: 'question',
        questionnaire_id: 'q-1',
        questions: [{ id: 'a', question: 'Need detail?', options: ['yes'] }],
      })

      await vi.advanceTimersByTimeAsync(20000)

      expect(convApi.listMessages).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('chat store - selectConversation race handling', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
    vi.clearAllMocks()
  })

  it('keeps the latest conversation messages when requests resolve out of order', async () => {
    const store = useChatStore()
    store.connectWs()

    let resolveFirst!: (value: { messages: ReturnType<typeof makeMessage>[]; total: number }) => void
    let resolveSecond!: (value: { messages: ReturnType<typeof makeMessage>[]; total: number }) => void

    vi.mocked(convApi.listMessages)
      .mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve }))
      .mockImplementationOnce(() => new Promise((resolve) => { resolveSecond = resolve }))

    const firstSelect = store.selectConversation('conv-1')
    const secondSelect = store.selectConversation('conv-2')

    resolveSecond({ messages: [makeMessage({ id: 'msg-2', content: 'second' })], total: 1 })
    await secondSelect

    resolveFirst({ messages: [makeMessage({ id: 'msg-1', content: 'first' })], total: 1 })
    await firstSelect

    expect(store.currentConversationId).toBe('conv-2')
    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].id).toBe('msg-2')
  })

  it('preserves structured parts from API messages', async () => {
    const store = useChatStore()
    store.connectWs()
    vi.mocked(convApi.listMessages).mockResolvedValueOnce({
      messages: [
        makeMessage({
          id: 'msg-parts',
          role: 'assistant',
          content: 'legacy',
          parts: [
            { seq: 0, type: 'text', text: 'from parts', json_payload: null, tool_call_id: null },
          ],
        }),
      ],
      total: 1,
    })

    await store.selectConversation('conv-parts')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].id).toBe('msg-parts')
    expect(store.messages[0].parts).toEqual([
      { seq: 0, type: 'text', text: 'from parts', json_payload: null, tool_call_id: null },
    ])
  })
})

describe('chat store - createConversation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('should call API with title and system prompt only', async () => {
    const mockConv = {
      id: 'conv-1', title: 'Test', provider_id: null, model_name: null,
      system_prompt_override: 'Be helpful', deep_thinking: false, created_at: '', updated_at: '',
      image_provider_id: null, image_model: null, share_token: null, thinking_budget: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation('Test', 'Be helpful')
    expect(convApi.createConversation).toHaveBeenCalledWith(
      'Test',
      'Be helpful',
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    )
    expect(result.id).toBe('conv-1')
    expect(store.conversations).toHaveLength(1)
  })

  it('should pass provider_id and model to API', async () => {
    const mockConv = {
      id: 'conv-2', title: 'New Conversation', provider_id: 'openai', model_name: 'gpt-4o',
      system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '',
      image_provider_id: null, image_model: null, share_token: null, thinking_budget: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation(undefined, 'prompt', 'openai', 'gpt-4o')
    expect(convApi.createConversation).toHaveBeenCalledWith(
      undefined,
      'prompt',
      'openai',
      'gpt-4o',
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    )
    expect(result.provider_id).toBe('openai')
    expect(result.model_name).toBe('gpt-4o')
  })

  it('should prepend new conversation to list', async () => {
    const mockConv = {
      id: 'conv-3', title: 'Third', provider_id: null, model_name: null,
      system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '',
      image_provider_id: null, image_model: null, share_token: null, thinking_budget: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    store.conversations = [
      { id: 'existing', title: 'Old', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null },
    ]
    await store.createConversation()
    expect(store.conversations).toHaveLength(2)
    expect(store.conversations[0].id).toBe('conv-3')
  })

  it('should pass subagent provider_id and model to API', async () => {
    const mockConv = {
      id: 'conv-4', title: 'Subagent Chat', provider_id: 'openai', model_name: 'gpt-4o',
      subagent_provider_id: 'openai', subagent_model: 'gpt-4.1-mini',
      system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '',
      image_provider_id: null, image_model: null, share_token: null, thinking_budget: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation(
      'Subagent Chat',
      undefined,
      'openai',
      'gpt-4o',
      undefined,
      undefined,
      'openai',
      'gpt-4.1-mini',
    )
    expect(convApi.createConversation).toHaveBeenCalledWith(
      'Subagent Chat',
      undefined,
      'openai',
      'gpt-4o',
      undefined,
      undefined,
      'openai',
      'gpt-4.1-mini',
      undefined,
      undefined,
    )
    expect(result.subagent_provider_id).toBe('openai')
    expect(result.subagent_model).toBe('gpt-4.1-mini')
  })
})

describe('chat store - cancelGeneration', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('sends cancel message when isStreaming is true', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockClear()

    store.isStreaming = true
    store.cancelGeneration()

    expect(mockWs.send).toHaveBeenCalledWith({ type: 'cancel' })
  })

  it('sends cancel message when isWaiting is true', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockClear()

    store.isWaiting = true
    store.cancelGeneration()

    expect(mockWs.send).toHaveBeenCalledWith({ type: 'cancel' })
  })

  it('does nothing when neither streaming nor waiting', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockClear()

    store.cancelGeneration()

    expect(mockWs.send).not.toHaveBeenCalled()
  })

  it('does not reset isStreaming/isWaiting (waits for backend)', () => {
    const store = useChatStore()
    store.connectWs()

    store.isStreaming = true
    store.isWaiting = true
    store.cancelGeneration()

    expect(store.isStreaming).toBe(true)
    expect(store.isWaiting).toBe(true)
  })
})

describe('chat store - sendMessage failure', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('should remove optimistic message and set sendFailed when ws is disconnected', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockReturnValue(false)

    store.currentConversationId = 'conv-1'
    store.messages = []

    store.sendMessage('Hello')

    expect(store.messages).toHaveLength(0)
    expect(store.isWaiting).toBe(false)
    expect(store.sendFailed).toBe(true)
  })

  it('should rollback editMessage when send fails', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockReturnValue(false)

    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.editMessage('msg-1', 'Updated')

    expect(store.messages).toHaveLength(2)
    expect(store.messages[0].content).toBe('Hello')
    expect(store.isStreaming).toBe(false)
    expect(store.isWaiting).toBe(false)
    expect(store.sendFailed).toBe(true)
  })

  it('should keep regenerate state unchanged when send fails', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    mockWs.send.mockReturnValue(false)

    store.currentConversationId = 'conv-1'
    store.messages = [
      makeMessage({ id: 'msg-1', role: 'user', content: 'Hello' }),
      makeMessage({ id: 'msg-2', role: 'assistant', content: 'Hi there' }),
    ]

    store.regenerateMessage('msg-2')

    expect(store.messages).toHaveLength(2)
    expect(store.messages[1].content).toBe('Hi there')
    expect(store.isStreaming).toBe(false)
    expect(store.isWaiting).toBe(false)
    expect(store.sendFailed).toBe(true)
  })
})

describe('chat store - assistant_delta handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('accumulates text into streaming blocks', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const handler = getWsHandler(mockWs, 'assistant_delta')!

    handler({ type: 'assistant_delta', delta: 'Hello' })
    handler({ type: 'assistant_delta', delta: ' world' })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('text')
    if (block.type === 'text') expect(block.content).toBe('Hello world')
    expect(store.isStreaming).toBe(true)
    expect(store.isWaiting).toBe(false)
  })

  it('creates new text block after non-text block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const thinkHandler = getWsHandler(mockWs, 'thinking_delta')!
    const deltaHandler = getWsHandler(mockWs, 'assistant_delta')!

    thinkHandler({ type: 'thinking_delta', delta: 'thinking...' })
    deltaHandler({ type: 'assistant_delta', delta: 'answer' })

    expect(store.streamingBlocks).toHaveLength(2)
    expect(store.streamingBlocks[0].type).toBe('thinking')
    expect(store.streamingBlocks[1].type).toBe('text')
    const textBlock = store.streamingBlocks[1]
    if (textBlock.type === 'text') expect(textBlock.content).toBe('answer')
  })
})

describe('chat store - thinking_delta handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('accumulates thinking content', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const handler = getWsHandler(mockWs, 'thinking_delta')!

    handler({ type: 'thinking_delta', delta: 'Let me ' })
    handler({ type: 'thinking_delta', delta: 'think...' })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('thinking')
    if (block.type === 'thinking') expect(block.content).toBe('Let me think...')
  })
})

describe('chat store - tool_call / tool_result handlers', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('tool_call adds a tool_call block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const handler = getWsHandler(mockWs, 'tool_call')!

    handler({ type: 'tool_call', tool_call_id: 'tc-1', tool_name: 'search', tool_input: { q: 'test' } })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      expect(block.id).toBe('tc-1')
      expect(block.name).toBe('search')
      expect(block.isLoading).toBe(true)
    }
  })

  it('tool_result matches by tool_call_id and updates the correct block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const trHandler = getWsHandler(mockWs, 'tool_result')!

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-1', tool_name: 'search' })
    tcHandler({ type: 'tool_call', tool_call_id: 'tc-2', tool_name: 'bash' })
    trHandler({ type: 'tool_result', tool_call_id: 'tc-1', result: 'found it', is_error: false })

    expect(store.streamingBlocks).toHaveLength(2)
    const block1 = store.streamingBlocks[0]
    const block2 = store.streamingBlocks[1]
    if (block1.type === 'tool_call') {
      expect(block1.result).toEqual({
        kind: 'text',
        text: 'found it',
        success: true,
        error: null,
        data: {},
        meta: {},
      })
      expect(block1.isLoading).toBe(false)
      expect(block1.isError).toBe(false)
    }
    if (block2.type === 'tool_call') {
      expect(block2.result).toBeUndefined()
      expect(block2.isLoading).toBe(true)
    }
  })

  it('tool_result keeps structured bash payload', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const trHandler = getWsHandler(mockWs, 'tool_result')!

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-bash', tool_name: 'bash' })
    trHandler({
      type: 'tool_result',
      tool_call_id: 'tc-bash',
      result: {
        kind: 'bash',
        text: 'ok',
        stdout: 'ok',
        stderr: '',
        exit_code: 0,
        timed_out: false,
        truncated: false,
        duration_ms: 12,
        error: false,
      },
      is_error: false,
    })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      expect(block.result).toEqual({
        kind: 'bash',
        text: 'ok',
        success: true,
        error: null,
        data: {
          stdout: 'ok',
          stderr: '',
          exit_code: 0,
        },
        meta: {
          timed_out: false,
          truncated: false,
          duration_ms: 12,
        },
      })
      expect(block.isLoading).toBe(false)
      expect(block.isError).toBe(false)
    }
  })
})

describe('chat store - questionnaire submit flow', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('keeps questionnaire during submit and clears it after matching question tool_result', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const questionHandler = getWsHandler(mockWs, 'question')!
    const toolResultHandler = getWsHandler(mockWs, 'tool_result')!

    questionHandler({
      type: 'question',
      questionnaire_id: 'qq-1',
      title: 'Clarify',
      questions: [{ id: 'q1', question: 'Pick one', options: ['A'] }],
    })
    expect(store.activeQuestionnaire?.questionnaire_id).toBe('qq-1')
    expect(store.questionnaireSubmitting).toBe(false)

    store.submitQuestionnaireAnswers([{
      id: 'q1',
      question: 'Pick one',
      selected_options: ['A'],
      free_text: '',
      notes: 'note-1',
    }])
    expect(mockWs.send).toHaveBeenCalledWith({
      type: 'question_answer',
      questionnaire_id: 'qq-1',
      answers: [{
        id: 'q1',
        question: 'Pick one',
        selected_options: ['A'],
        free_text: '',
        notes: 'note-1',
      }],
    })
    expect(store.activeQuestionnaire?.questionnaire_id).toBe('qq-1')
    expect(store.questionnaireSubmitting).toBe(true)
    expect(store.isWaiting).toBe(true)

    toolResultHandler({
      type: 'tool_result',
      tool_call_id: 'tc-question-1',
      is_error: false,
      result: {
        kind: 'question',
        text: '{"ok":true}',
        success: true,
        error: null,
        data: {
          questionnaire_id: 'qq-1',
          answers: [{ id: 'q1', selected_options: ['A'], free_text: '', notes: 'note-1' }],
        },
        meta: {},
      },
    })
    expect(store.activeQuestionnaire).toBeNull()
    expect(store.questionnaireSubmitting).toBe(false)
    expect(store.isWaiting).toBe(false)
  })

  it('keeps questionnaire open for retry on recoverable question submit errors', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const questionHandler = getWsHandler(mockWs, 'question')!
    const errorHandler = getWsHandler(mockWs, 'error')!

    questionHandler({
      type: 'question',
      questionnaire_id: 'qq-2',
      title: 'Clarify',
      questions: [{ id: 'q1', question: 'Pick one', options: ['A'] }],
    })
    store.submitQuestionnaireAnswers([{
      id: 'q1',
      question: 'Pick one',
      selected_options: ['A'],
      free_text: '',
      notes: 'note-2',
    }])
    expect(store.questionnaireSubmitting).toBe(true)

    errorHandler({
      type: 'error',
      code: 'container_not_connected',
      message: 'Container is not connected for question answers',
    })

    expect(store.activeQuestionnaire?.questionnaire_id).toBe('qq-2')
    expect(store.questionnaireSubmitting).toBe(false)
    expect(store.isWaiting).toBe(false)
  })
})

describe('chat store - subagent trace handlers', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('merges streamed subagent trace into the explore tool block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const traceHandler = getWsHandler(mockWs, 'subagent_trace_delta')!

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-explore-1', tool_name: 'explore' })

    traceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-explore-1',
      event_type: 'assistant_delta',
      payload: { delta: 'Investigating ' },
    })
    traceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-explore-1',
      event_type: 'assistant_delta',
      payload: { delta: 'repo' },
    })
    traceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-explore-1',
      event_type: 'tool_call',
      payload: {
        tool_call_id: 'sub-tc-1',
        tool_name: 'read',
        tool_input: { file_path: 'README.md' },
      },
    })
    traceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-explore-1',
      event_type: 'tool_result',
      payload: {
        tool_call_id: 'sub-tc-1',
        result: { kind: 'read', text: 'ok', success: true, error: null, data: {}, meta: {} },
        is_error: false,
      },
    })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      expect(typeof block.result).toBe('object')
      const result = block.result as { kind: string; data?: Record<string, unknown> }
      expect(result.kind).toBe('explore')
      const trace = (result.data?.trace ?? []) as Array<Record<string, unknown>>
      expect(trace).toHaveLength(2)
      expect(trace[0].type).toBe('text')
      expect(trace[0].content).toBe('Investigating repo')
      expect(trace[1].type).toBe('tool_call')
      expect(trace[1].id).toBe('sub-tc-1')
      expect((trace[1].result as Record<string, unknown>).kind).toBe('read')
      expect(trace[1].isError).toBe(false)
    }
  })

  it('keeps backward compatibility for legacy task_trace_delta buffering', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const traceHandler = getWsHandler(mockWs, 'task_trace_delta')!

    traceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-task-buffered',
      event_type: 'assistant_delta',
      payload: { delta: 'Early ' },
    })
    traceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-task-buffered',
      event_type: 'assistant_delta',
      payload: { delta: 'trace' },
    })
    traceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-task-buffered',
      event_type: 'tool_call',
      payload: {
        tool_call_id: 'sub-tc-buf-1',
        tool_name: 'read',
        tool_input: { file_path: 'README.md' },
      },
    })

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-task-buffered', tool_name: 'task' })

    traceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-task-buffered',
      event_type: 'tool_result',
      payload: {
        tool_call_id: 'sub-tc-buf-1',
        result: { kind: 'read', text: 'ok', success: true, error: null, data: {}, meta: {} },
        is_error: false,
      },
    })

    expect(store.streamingBlocks).toHaveLength(1)
    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      const result = block.result as { kind: string; data?: Record<string, unknown> }
      expect(result.kind).toBe('task')
      const trace = (result.data?.trace ?? []) as Array<Record<string, unknown>>
      expect(trace).toHaveLength(2)
      expect(trace[0].type).toBe('text')
      expect(trace[0].content).toBe('Early trace')
      expect(trace[1].type).toBe('tool_call')
      expect(trace[1].id).toBe('sub-tc-buf-1')
      expect((trace[1].result as Record<string, unknown>).kind).toBe('read')
    }
  })

  it('does not apply task_trace_delta to an active explore block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const subagentTraceHandler = getWsHandler(mockWs, 'subagent_trace_delta')!
    const legacyTraceHandler = getWsHandler(mockWs, 'task_trace_delta')!

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-mixed-1', tool_name: 'explore' })

    legacyTraceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-mixed-1',
      event_type: 'assistant_delta',
      payload: { delta: 'legacy-trace' },
    })
    subagentTraceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-mixed-1',
      event_type: 'assistant_delta',
      payload: { delta: 'explore-trace' },
    })

    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      const result = block.result as { kind: string; data?: Record<string, unknown> }
      expect(result.kind).toBe('explore')
      const trace = (result.data?.trace ?? []) as Array<Record<string, unknown>>
      expect(trace).toHaveLength(1)
      expect(trace[0].type).toBe('text')
      expect(trace[0].content).toBe('explore-trace')
    }
  })

  it('does not apply subagent_trace_delta to an active legacy task block', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const subagentTraceHandler = getWsHandler(mockWs, 'subagent_trace_delta')!
    const legacyTraceHandler = getWsHandler(mockWs, 'task_trace_delta')!

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-mixed-2', tool_name: 'task' })

    subagentTraceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-mixed-2',
      event_type: 'assistant_delta',
      payload: { delta: 'explore-trace' },
    })
    legacyTraceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-mixed-2',
      event_type: 'assistant_delta',
      payload: { delta: 'legacy-trace' },
    })

    const block = store.streamingBlocks[0]
    expect(block.type).toBe('tool_call')
    if (block.type === 'tool_call') {
      const result = block.result as { kind: string; data?: Record<string, unknown> }
      expect(result.kind).toBe('task')
      const trace = (result.data?.trace ?? []) as Array<Record<string, unknown>>
      expect(trace).toHaveLength(1)
      expect(trace[0].type).toBe('text')
      expect(trace[0].content).toBe('legacy-trace')
    }
  })

  it('keeps subagent and legacy trace buffers isolated during replay', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const subagentTraceHandler = getWsHandler(mockWs, 'subagent_trace_delta')!
    const legacyTraceHandler = getWsHandler(mockWs, 'task_trace_delta')!

    subagentTraceHandler({
      type: 'subagent_trace_delta',
      tool_call_id: 'tc-shared',
      event_type: 'assistant_delta',
      payload: { delta: 'explore-buffered' },
    })
    legacyTraceHandler({
      type: 'task_trace_delta',
      tool_call_id: 'tc-shared',
      event_type: 'assistant_delta',
      payload: { delta: 'task-buffered' },
    })

    tcHandler({ type: 'tool_call', tool_call_id: 'tc-shared', tool_name: 'explore' })
    tcHandler({ type: 'tool_call', tool_call_id: 'tc-shared', tool_name: 'task' })

    const exploreBlock = store.streamingBlocks.find(
      (b): b is Extract<ContentBlock, { type: 'tool_call' }> =>
        b.type === 'tool_call' && b.name === 'explore' && b.id === 'tc-shared'
    )!
    const taskBlock = store.streamingBlocks.find(
      (b): b is Extract<ContentBlock, { type: 'tool_call' }> =>
        b.type === 'tool_call' && b.name === 'task' && b.id === 'tc-shared'
    )!

    const exploreTrace = (
      (exploreBlock.result as { data?: Record<string, unknown> }).data?.trace ?? []
    ) as Array<Record<string, unknown>>
    const taskTrace = (
      (taskBlock.result as { data?: Record<string, unknown> }).data?.trace ?? []
    ) as Array<Record<string, unknown>>

    expect(exploreTrace).toHaveLength(1)
    expect(exploreTrace[0].content).toBe('explore-buffered')
    expect(taskTrace).toHaveLength(1)
    expect(taskTrace[0].content).toBe('task-buffered')
  })
})

describe('chat store -> ChatMessage bash result flow', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('renders bash metadata from structured tool_result after complete', async () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const tcHandler = getWsHandler(mockWs, 'tool_call')!
    const trHandler = getWsHandler(mockWs, 'tool_result')!
    const completeHandler = getWsHandler(mockWs, 'complete')!

    tcHandler({
      type: 'tool_call',
      tool_call_id: 'tc-bash-flow',
      tool_name: 'bash',
      tool_input: { command: 'echo hi' },
    })
    trHandler({
      type: 'tool_result',
      tool_call_id: 'tc-bash-flow',
      result: {
        kind: 'bash',
        text: 'hi',
        stdout: 'hi',
        stderr: '',
        exit_code: 0,
        timed_out: false,
        truncated: false,
        duration_ms: 12,
        error: false,
      },
      is_error: false,
    })
    completeHandler({
      type: 'complete',
      message_id: 'msg-bash-flow',
      content: 'Done',
    })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].tool_calls).toContain('"kind":"bash"')

    const wrapper = mount(ChatMessage, {
      props: {
        message: store.messages[0],
        conversationId: 'conv-1',
      },
      global: { plugins: [ElementPlus] },
    })

    const header = wrapper.find('.tool-call-display .tool-header')
    expect(header.exists()).toBe(true)
    await header.trigger('click')

    const meta = wrapper.find('.tool-call-display .bash-meta')
    expect(meta.exists()).toBe(true)
    expect(meta.text()).toContain('exit_code=0')
    expect(meta.text()).toContain('duration=12ms')
  })
})

describe('chat store - error handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('saves partial content as a message on error', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const deltaHandler = getWsHandler(mockWs, 'assistant_delta')!
    const errorHandler = getWsHandler(mockWs, 'error')!

    deltaHandler({ type: 'assistant_delta', delta: 'partial response' })
    errorHandler({ type: 'error', message: 'timeout' })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('partial response')
    expect(store.isStreaming).toBe(false)
  })

  it('does not save message when no streaming content', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const errorHandler = getWsHandler(mockWs, 'error')!

    errorHandler({ type: 'error', message: 'timeout' })

    expect(store.messages).toHaveLength(0)
    expect(store.isStreaming).toBe(false)
  })
})

describe('chat store - sendMessage success path', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('adds optimistic message and sets isWaiting', () => {
    const store = useChatStore()
    store.connectWs()
    store.currentConversationId = 'conv-1'

    store.sendMessage('Hello')

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].role).toBe('user')
    expect(store.messages[0].content).toBe('Hello')
    expect(store.messages[0].id).toMatch(/^pending-/)
    expect(store.isWaiting).toBe(true)
  })

  it('does nothing without a current conversation', () => {
    const store = useChatStore()
    store.connectWs()
    store.currentConversationId = null

    store.sendMessage('Hello')

    expect(store.messages).toHaveLength(0)
  })
})

describe('chat store - selectConversation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
    vi.clearAllMocks()
  })

  it('loads messages and sends WS join', async () => {
    const mockMessages = [makeMessage({ id: 'msg-1' })]
    vi.mocked(convApi.listMessages).mockResolvedValueOnce({ messages: mockMessages, total: 1 })

    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]

    await store.selectConversation('conv-1')

    expect(store.currentConversationId).toBe('conv-1')
    expect(store.messages).toEqual(mockMessages)
    expect(store.totalMessages).toBe(1)
    expect(mockWs.send).toHaveBeenCalledWith(
      { type: 'join_conversation', conversation_id: 'conv-1' }
    )
  })
})

describe('chat store - deleteConversation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('removes conversation from list', async () => {
    vi.mocked(convApi.deleteConversation).mockResolvedValueOnce(undefined)
    const store = useChatStore()
    store.conversations = [
      { id: 'conv-1', title: 'A', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null },
      { id: 'conv-2', title: 'B', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null },
    ]

    await store.deleteConversation('conv-1')

    expect(store.conversations).toHaveLength(1)
    expect(store.conversations[0].id).toBe('conv-2')
  })

  it('clears current if deleted conversation is active', async () => {
    vi.mocked(convApi.deleteConversation).mockResolvedValueOnce(undefined)
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [makeMessage()]
    store.conversations = [
      { id: 'conv-1', title: 'A', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null },
    ]

    await store.deleteConversation('conv-1')

    expect(store.currentConversationId).toBeNull()
    expect(store.messages).toHaveLength(0)
  })

  it('keeps state unchanged when delete API fails', async () => {
    vi.mocked(convApi.deleteConversation).mockRejectedValueOnce(new Error('delete failed'))
    const store = useChatStore()
    store.currentConversationId = 'conv-1'
    store.messages = [makeMessage()]
    store.conversations = [
      { id: 'conv-1', title: 'A', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null, subagent_provider_id: null, subagent_model: null },
      { id: 'conv-2', title: 'B', provider_id: null, model_name: null, system_prompt_override: null, deep_thinking: false, thinking_budget: null, created_at: '', updated_at: '', image_provider_id: null, image_model: null, share_token: null, subagent_provider_id: null, subagent_model: null },
    ]

    await expect(store.deleteConversation('conv-1')).rejects.toThrow('delete failed')
    expect(store.currentConversationId).toBe('conv-1')
    expect(store.messages).toHaveLength(1)
    expect(store.conversations.map(c => c.id)).toEqual(['conv-1', 'conv-2'])
  })
})

describe('chat store - container_status disconnect handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('saves partial content and resets state when streaming', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const deltaHandler = getWsHandler(mockWs, 'assistant_delta')!
    const statusHandler = getWsHandler(mockWs, 'container_status')!

    // Simulate streaming in progress
    deltaHandler({ type: 'assistant_delta', delta: 'partial response' })

    statusHandler({ type: 'container_status', status: 'disconnected', message: 'Container disconnected' })

    expect(store.messages).toHaveLength(1)
    expect(store.messages[0].content).toBe('partial response')
    expect(store.messages[0].role).toBe('assistant')
    expect(store.isStreaming).toBe(false)
    expect(store.isWaiting).toBe(false)
    expect(store.streamingBlocks).toHaveLength(0)
  })

  it('resets state when isWaiting and disconnected', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const statusHandler = getWsHandler(mockWs, 'container_status')!

    store.isWaiting = true

    statusHandler({ type: 'container_status', status: 'disconnected', message: 'Container disconnected' })

    expect(store.isWaiting).toBe(false)
    expect(store.isStreaming).toBe(false)
    // No content was streaming, so no message saved
    expect(store.messages).toHaveLength(0)
  })

  it('does nothing when not streaming and disconnected', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const statusHandler = getWsHandler(mockWs, 'container_status')!

    statusHandler({ type: 'container_status', status: 'disconnected', message: 'Container disconnected' })

    expect(store.isStreaming).toBe(false)
    expect(store.isWaiting).toBe(false)
    expect(store.messages).toHaveLength(0)
  })

  it('does nothing on connected status', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const statusHandler = getWsHandler(mockWs, 'container_status')!

    store.isStreaming = true
    statusHandler({ type: 'container_status', status: 'connected', message: 'Container connected' })

    expect(store.isStreaming).toBe(true)
    expect(store.messages).toHaveLength(0)
  })

  it('ignores expected disconnected right after restart/start signal', () => {
    const store = useChatStore()
    store.connectWs()
    const mockWs = mockWsInstances[mockWsInstances.length - 1]
    const statusHandler = getWsHandler(mockWs, 'container_status')!

    store.isWaiting = true
    statusHandler({ type: 'container_status', status: 'restarting', reason: 'model_switch' })
    statusHandler({ type: 'container_status', status: 'disconnected', reason: 'unexpected_disconnect' })

    expect(store.isWaiting).toBe(true)
    expect(store.isStreaming).toBe(false)
    expect(store.messages).toHaveLength(0)
  })
})
