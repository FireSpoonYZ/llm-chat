import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import ChatMessage from '../../components/ChatMessage.vue'

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

function makeMessage(overrides: Record<string, unknown> = {}) {
  return {
    id: 'msg-1',
    role: 'user' as const,
    content: 'Hello world',
    tool_calls: null,
    tool_call_id: null,
    token_count: null,
    created_at: '2025-01-01T12:00:00Z',
    ...overrides,
  }
}

describe('ChatMessage', () => {
  it('renders edit button for user messages', () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user' }) },
    })
    expect(wrapper.find('.edit-btn').exists()).toBe(true)
    expect(wrapper.find('.regenerate-btn').exists()).toBe(false)
  })

  it('renders regenerate button for assistant messages', () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'assistant' }) },
    })
    expect(wrapper.find('.regenerate-btn').exists()).toBe(true)
    expect(wrapper.find('.edit-btn').exists()).toBe(false)
  })

  it('enters edit mode when edit button is clicked', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
    })
    await wrapper.find('.edit-btn').trigger('click')
    expect(wrapper.find('.edit-textarea').exists()).toBe(true)
    expect((wrapper.find('.edit-textarea').element as HTMLTextAreaElement).value).toBe('Original')
  })

  it('emits edit event on save', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
    })
    await wrapper.find('.edit-btn').trigger('click')
    await wrapper.find('.edit-textarea').setValue('Updated')
    await wrapper.find('.save-btn').trigger('click')
    expect(wrapper.emitted('edit')).toBeTruthy()
    expect(wrapper.emitted('edit')![0]).toEqual(['msg-1', 'Updated'])
  })

  it('cancels edit and restores original content', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
    })
    await wrapper.find('.edit-btn').trigger('click')
    await wrapper.find('.edit-textarea').setValue('Changed')
    await wrapper.find('.cancel-btn').trigger('click')
    expect(wrapper.find('.edit-textarea').exists()).toBe(false)
    expect(wrapper.find('.message-content').exists()).toBe(true)
  })

  it('emits regenerate event when regenerate button is clicked', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ id: 'msg-2', role: 'assistant' }) },
    })
    await wrapper.find('.regenerate-btn').trigger('click')
    expect(wrapper.emitted('regenerate')).toBeTruthy()
    expect(wrapper.emitted('regenerate')![0]).toEqual(['msg-2'])
  })

  it('does not show action buttons when isStreaming is true', () => {
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({ role: 'assistant' }),
        isStreaming: true,
      },
    })
    expect(wrapper.find('.regenerate-btn').exists()).toBe(false)
  })
})
