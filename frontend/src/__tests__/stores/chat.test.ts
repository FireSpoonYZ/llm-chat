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

// Mock auth
vi.mock('../../api/auth', () => ({
  refreshAccessToken: vi.fn().mockResolvedValue('refreshed-token'),
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
    constructor(_url: string, _tokenRefresher?: () => Promise<string | null>) { mockWsInstances.push(this) }
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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

  it('should update content and truncate messages after the edited one', () => {
    const store = useChatStore()
    store.connectWs('test-token')
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
    mockWsInstances.length = 0
  })

  it('should delete assistant message and subsequent messages', () => {
    const store = useChatStore()
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
      system_prompt_override: 'Be helpful', deep_thinking: false, created_at: '', updated_at: '',
      image_provider: null, image_model: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation('Test', 'Be helpful')
    expect(convApi.createConversation).toHaveBeenCalledWith('Test', 'Be helpful', undefined, undefined, undefined, undefined)
    expect(result.id).toBe('conv-1')
    expect(store.conversations).toHaveLength(1)
  })

  it('should pass provider and model to API', async () => {
    const mockConv = {
      id: 'conv-2', title: 'New Conversation', provider: 'openai', model_name: 'gpt-4o',
      system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '',
      image_provider: null, image_model: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    const result = await store.createConversation(undefined, 'prompt', 'openai', 'gpt-4o')
    expect(convApi.createConversation).toHaveBeenCalledWith(undefined, 'prompt', 'openai', 'gpt-4o', undefined, undefined)
    expect(result.provider).toBe('openai')
    expect(result.model_name).toBe('gpt-4o')
  })

  it('should prepend new conversation to list', async () => {
    const mockConv = {
      id: 'conv-3', title: 'Third', provider: null, model_name: null,
      system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '',
      image_provider: null, image_model: null,
    }
    vi.mocked(convApi.createConversation).mockResolvedValueOnce(mockConv)
    const store = useChatStore()
    store.conversations = [
      { id: 'existing', title: 'Old', provider: null, model_name: null, system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '', image_provider: null, image_model: null },
    ]
    await store.createConversation()
    expect(store.conversations).toHaveLength(2)
    expect(store.conversations[0].id).toBe('conv-3')
  })
})

describe('chat store - sendMessage failure', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('should remove optimistic message and set sendFailed when ws is disconnected', () => {
    const store = useChatStore()
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    expect(store.sendFailed).toBe(true)
  })

  it('should rollback regenerateMessage when send fails', () => {
    const store = useChatStore()
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
      expect(block1.result).toBe('found it')
      expect(block1.isLoading).toBe(false)
      expect(block1.isError).toBe(false)
    }
    if (block2.type === 'tool_call') {
      expect(block2.result).toBeUndefined()
      expect(block2.isLoading).toBe(true)
    }
  })
})

describe('chat store - error handler', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockWsInstances.length = 0
  })

  it('saves partial content as a message on error', () => {
    const store = useChatStore()
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
    store.connectWs('test-token')
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
      { id: 'conv-1', title: 'A', provider: null, model_name: null, system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '', image_provider: null, image_model: null },
      { id: 'conv-2', title: 'B', provider: null, model_name: null, system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '', image_provider: null, image_model: null },
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
      { id: 'conv-1', title: 'A', provider: null, model_name: null, system_prompt_override: null, deep_thinking: false, created_at: '', updated_at: '', image_provider: null, image_model: null },
    ]

    await store.deleteConversation('conv-1')

    expect(store.currentConversationId).toBeNull()
    expect(store.messages).toHaveLength(0)
  })
})
