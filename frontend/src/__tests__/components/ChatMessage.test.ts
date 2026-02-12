import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import ElementPlus from 'element-plus'
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

const globalConfig = { plugins: [ElementPlus] }

describe('ChatMessage', () => {
  it('renders edit button for user messages', () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user' }) },
      global: globalConfig,
    })
    expect(wrapper.find('.edit-btn').exists()).toBe(true)
    expect(wrapper.find('.regenerate-btn').exists()).toBe(false)
  })

  it('renders regenerate button for assistant messages', () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'assistant' }) },
      global: globalConfig,
    })
    expect(wrapper.find('.regenerate-btn').exists()).toBe(true)
    expect(wrapper.find('.edit-btn').exists()).toBe(false)
  })

  it('enters edit mode when edit button is clicked', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
      global: globalConfig,
    })
    await wrapper.find('.edit-btn').trigger('click')
    expect(wrapper.find('.edit-textarea').exists()).toBe(true)
    expect((wrapper.find('.edit-textarea textarea').element as HTMLTextAreaElement).value).toBe('Original')
  })

  it('emits edit event on save', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
      global: globalConfig,
    })
    await wrapper.find('.edit-btn').trigger('click')
    await wrapper.find('.edit-textarea textarea').setValue('Updated')
    await wrapper.find('.save-btn').trigger('click')
    expect(wrapper.emitted('edit')).toBeTruthy()
    expect(wrapper.emitted('edit')![0]).toEqual(['msg-1', 'Updated'])
  })

  it('cancels edit and restores original content', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ role: 'user', content: 'Original' }) },
      global: globalConfig,
    })
    await wrapper.find('.edit-btn').trigger('click')
    await wrapper.find('.edit-textarea textarea').setValue('Changed')
    await wrapper.find('.cancel-btn').trigger('click')
    expect(wrapper.find('.edit-textarea').exists()).toBe(false)
    expect(wrapper.find('.message-content').exists()).toBe(true)
  })

  it('emits regenerate event when regenerate button is clicked', async () => {
    const wrapper = mount(ChatMessage, {
      props: { message: makeMessage({ id: 'msg-2', role: 'assistant' }) },
      global: globalConfig,
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
      global: globalConfig,
    })
    expect(wrapper.find('.regenerate-btn').exists()).toBe(false)
  })
})

describe('ChatMessage - contentBlocks parsing', () => {
  it('renders new-format blocks in interleaved order (thinking → text → tool_call)', () => {
    const blocks = [
      { type: 'thinking', content: 'Let me think...' },
      { type: 'text', content: 'Here is the answer' },
      { type: 'tool_call', id: 'tc-1', name: 'search', input: { q: 'test' }, result: 'found', isError: false },
      { type: 'text', content: 'Based on the search...' },
    ]
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({
          role: 'assistant',
          content: '',
          tool_calls: JSON.stringify(blocks),
        }),
      },
      global: globalConfig,
    })
    const children = wrapper.findAll('.thinking-block, .message-content, .tool-call-display')
    expect(children.length).toBe(4)
    expect(children[0].classes()).toContain('thinking-block')
    expect(children[1].classes()).toContain('message-content')
    expect(children[2].classes()).toContain('tool-call-display')
    expect(children[3].classes()).toContain('message-content')
  })

  it('normalizes is_error to isError in new-format tool_call blocks', () => {
    const blocks = [
      { type: 'tool_call', id: 'tc-1', name: 'run', input: {}, result: 'fail', is_error: true },
    ]
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({
          role: 'assistant',
          content: '',
          tool_calls: JSON.stringify(blocks),
        }),
      },
      global: globalConfig,
    })
    // ToolCallDisplay should receive isError=true
    const toolDisplay = wrapper.findComponent({ name: 'ToolCallDisplay' })
    expect(toolDisplay.exists()).toBe(true)
    expect(toolDisplay.props('isError')).toBe(true)
  })

  it('renders legacy format (no type field) with text on top and tool calls below', () => {
    const legacyToolCalls = [
      { id: 'tc-1', name: 'search', input: { q: 'test' }, result: 'found', is_error: false },
    ]
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({
          role: 'assistant',
          content: 'Some text content',
          tool_calls: JSON.stringify(legacyToolCalls),
        }),
      },
      global: globalConfig,
    })
    const children = wrapper.findAll('.message-content, .tool-call-display')
    expect(children.length).toBe(2)
    expect(children[0].classes()).toContain('message-content')
    expect(children[1].classes()).toContain('tool-call-display')
  })

  it('renders plain text when no tool_calls present', () => {
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({ role: 'assistant', content: 'Just text' }),
      },
      global: globalConfig,
    })
    expect(wrapper.findAll('.message-content').length).toBe(1)
    expect(wrapper.find('.thinking-block').exists()).toBe(false)
    expect(wrapper.find('.tool-call-display').exists()).toBe(false)
  })

  it('prefers streamingBlocks over stored tool_calls', () => {
    const storedBlocks = [
      { type: 'text', content: 'stored' },
    ]
    const wrapper = mount(ChatMessage, {
      props: {
        message: makeMessage({
          role: 'assistant',
          content: '',
          tool_calls: JSON.stringify(storedBlocks),
        }),
        streamingBlocks: [{ type: 'text' as const, content: 'streaming' }],
      },
      global: globalConfig,
    })
    expect(wrapper.find('.message-content').text()).toContain('streaming')
  })
})
