import { describe, it, expect, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import ChatInput from '../../components/ChatInput.vue'

function mountInput(props = {}) {
  return mount(ChatInput, {
    props: {
      disabled: false,
      deepThinking: false,
      streaming: false,
      thinkingBudget: null,
      subagentThinkingBudget: null,
      ...props,
    },
    global: { plugins: [ElementPlus] },
  })
}

describe('ChatInput', () => {
  it('renders attach button', () => {
    const wrapper = mountInput()
    expect(wrapper.find('[data-testid="attach-btn"]').exists()).toBe(true)
  })

  it('emits attach-files when files are selected', async () => {
    const wrapper = mountInput()
    const fileInput = wrapper.find('[data-testid="attach-file-input"]')
    const file = new File(['data'], 'doc.pdf', { type: 'application/pdf' })

    Object.defineProperty(fileInput.element, 'files', { value: [file] })
    await fileInput.trigger('change')
    await flushPromises()

    expect(wrapper.emitted('attach-files')).toBeTruthy()
    expect(wrapper.emitted('attach-files')![0]).toEqual([[file]])
  })

  it('does not emit attach-files when no files selected', async () => {
    const wrapper = mountInput()
    const fileInput = wrapper.find('[data-testid="attach-file-input"]')

    Object.defineProperty(fileInput.element, 'files', { value: [] })
    await fileInput.trigger('change')
    await flushPromises()

    expect(wrapper.emitted('attach-files')).toBeFalsy()
  })

  it('shows budget inputs even when deep thinking is off', () => {
    const wrapper = mountInput({ deepThinking: false })
    expect(wrapper.find('[data-testid="thinking-budget-input"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="subagent-thinking-budget-input"]').exists()).toBe(true)
  })

  it('emits budget updates from both budget inputs', async () => {
    const wrapper = mountInput({ deepThinking: false })
    const thinking = wrapper.find('[data-testid="thinking-budget-input"]')
    const subagent = wrapper.find('[data-testid="subagent-thinking-budget-input"]')

    await thinking.setValue('262144')
    await thinking.trigger('change')
    await subagent.setValue('131072')
    await subagent.trigger('change')

    expect(wrapper.emitted('update:thinkingBudget')?.[0]).toEqual([262144])
    expect(wrapper.emitted('update:subagentThinkingBudget')?.[0]).toEqual([131072])
  })

  it('emits null budget when budget input is cleared', async () => {
    const wrapper = mountInput({ thinkingBudget: 4096 })
    const thinking = wrapper.find('[data-testid="thinking-budget-input"]')
    await thinking.setValue('')
    await thinking.trigger('change')
    expect(wrapper.emitted('update:thinkingBudget')?.[0]).toEqual([null])
  })

  it('does not emit budget updates for invalid thinking budget values', async () => {
    const wrapper = mountInput()
    const thinking = wrapper.find('[data-testid="thinking-budget-input"]')

    await thinking.setValue('1234.5')
    await thinking.trigger('change')
    await thinking.setValue('1000001')
    await thinking.trigger('change')

    expect(wrapper.emitted('update:thinkingBudget')).toBeFalsy()
  })

  it('does not emit budget updates for invalid subagent budget values', async () => {
    const wrapper = mountInput()
    const subagent = wrapper.find('[data-testid="subagent-thinking-budget-input"]')

    await subagent.setValue('1023')
    await subagent.trigger('change')
    await subagent.setValue('1000001')
    await subagent.trigger('change')

    expect(wrapper.emitted('update:subagentThinkingBudget')).toBeFalsy()
  })

  it('shows send button when not streaming', () => {
    const wrapper = mountInput({ streaming: false })
    expect(wrapper.find('.send-btn').attributes('aria-label')).toBe('Send message')
  })

  it('shows stop button when streaming', () => {
    const wrapper = mountInput({ streaming: true })
    expect(wrapper.find('.stop-btn').exists()).toBe(true)
    expect(wrapper.find('.stop-btn').attributes('aria-label')).toBe('Stop generation')
    expect(wrapper.find('.stop-btn .stop-icon-square').exists()).toBe(true)
  })

  it('emits stop when stop button is clicked', async () => {
    const wrapper = mountInput({ streaming: true })
    await wrapper.find('.stop-btn').trigger('click')
    expect(wrapper.emitted('stop')).toBeTruthy()
  })

  it('does not show send button when streaming', () => {
    const wrapper = mountInput({ streaming: true })
    const sendBtn = wrapper.findAll('.send-btn').filter(w => !w.classes().includes('stop-btn'))
    expect(sendBtn.length).toBe(0)
  })

  it('does not show stop button when not streaming', () => {
    const wrapper = mountInput({ streaming: false })
    expect(wrapper.find('.stop-btn').exists()).toBe(false)
  })

  it('stop button is not disabled even when disabled prop is true', () => {
    const wrapper = mountInput({ streaming: true, disabled: true })
    const btn = wrapper.find('.stop-btn')
    expect(btn.attributes('disabled')).toBeUndefined()
  })

  it('does not emit send on enter key when disabled', async () => {
    const wrapper = mountInput({ disabled: true, streaming: false })
    const textarea = wrapper.find('.chat-textarea')
    await wrapper.find('.chat-textarea').setValue('hello')
    await textarea.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('send')).toBeFalsy()
  })
})
