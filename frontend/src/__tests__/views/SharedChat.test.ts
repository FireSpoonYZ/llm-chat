import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import SharedChat from '../../views/SharedChat.vue'

// Mock markdown-it
vi.mock('markdown-it', () => ({
  default: class {
    render(str: string) { return `<p>${str}</p>` }
  },
}))

// Mock highlight.js
vi.mock('highlight.js', () => ({
  default: {
    getLanguage: vi.fn(),
    highlight: vi.fn(),
  },
}))
vi.mock('highlight.js/styles/github-dark.css', () => ({}))

// Mock sharing API
const mockGetSharedConversation = vi.fn()
const mockGetSharedMessages = vi.fn()
vi.mock('../../api/sharing', () => ({
  getSharedConversation: (...args: any[]) => mockGetSharedConversation(...args),
  getSharedMessages: (...args: any[]) => mockGetSharedMessages(...args),
}))

const globalConfig = { plugins: [ElementPlus] }

describe('SharedChat', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders conversation title and messages', async () => {
    mockGetSharedConversation.mockResolvedValueOnce({
      title: 'Shared Chat',
      model_name: 'gpt-4',
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    })
    mockGetSharedMessages.mockResolvedValueOnce({
      messages: [
        { id: 'msg-1', role: 'user', content: 'Hello', tool_calls: null, tool_call_id: null, token_count: null, created_at: '2025-01-01T00:00:00Z' },
        { id: 'msg-2', role: 'assistant', content: 'Hi there', tool_calls: null, tool_call_id: null, token_count: null, created_at: '2025-01-01T00:00:00Z' },
      ],
      total: 2,
    })

    const wrapper = mount(SharedChat, {
      props: { shareToken: 'abc123' },
      global: globalConfig,
    })

    await flushPromises()

    expect(wrapper.find('.shared-title').text()).toBe('Shared Chat')
    expect(wrapper.findAll('.chat-message').length).toBe(2)
  })

  it('shows error state for invalid token', async () => {
    mockGetSharedConversation.mockRejectedValueOnce(new Error('Not found'))
    mockGetSharedMessages.mockRejectedValueOnce(new Error('Not found'))

    const wrapper = mount(SharedChat, {
      props: { shareToken: 'invalid' },
      global: globalConfig,
    })

    await flushPromises()

    expect(wrapper.find('.error-state').exists()).toBe(true)
    expect(wrapper.find('.shared-title').exists()).toBe(false)
  })

  it('does not show action buttons (readOnly)', async () => {
    mockGetSharedConversation.mockResolvedValueOnce({
      title: 'Test',
      model_name: null,
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    })
    mockGetSharedMessages.mockResolvedValueOnce({
      messages: [
        { id: 'msg-1', role: 'user', content: 'Hello', tool_calls: null, tool_call_id: null, token_count: null, created_at: '2025-01-01T00:00:00Z' },
      ],
      total: 1,
    })

    const wrapper = mount(SharedChat, {
      props: { shareToken: 'abc123' },
      global: globalConfig,
    })

    await flushPromises()

    expect(wrapper.find('.edit-btn').exists()).toBe(false)
    expect(wrapper.find('.regenerate-btn').exists()).toBe(false)
    expect(wrapper.find('.message-footer').exists()).toBe(false)
  })

  it('shows read-only tag', async () => {
    mockGetSharedConversation.mockResolvedValueOnce({
      title: 'Test',
      model_name: null,
      created_at: '2025-01-01T00:00:00Z',
      updated_at: '2025-01-01T00:00:00Z',
    })
    mockGetSharedMessages.mockResolvedValueOnce({
      messages: [],
      total: 0,
    })

    const wrapper = mount(SharedChat, {
      props: { shareToken: 'abc123' },
      global: globalConfig,
    })

    await flushPromises()

    expect(wrapper.find('.el-tag').text()).toBe('Read-only')
  })
})
