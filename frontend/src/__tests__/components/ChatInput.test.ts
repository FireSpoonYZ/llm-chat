import { describe, it, expect, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import ChatInput from '../../components/ChatInput.vue'

function mountInput(props = {}) {
  return mount(ChatInput, {
    props: { disabled: false, deepThinking: false, ...props },
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
})
