import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import { nextTick } from 'vue'
import ToolCallDisplay from '../../components/ToolCallDisplay.vue'

function taskResult(traceSize = 2) {
  return {
    kind: 'task',
    text: 'done',
    success: true,
    error: null,
    data: {
      trace: Array.from({ length: traceSize }).map((_, idx) => ({
        type: idx % 2 === 0 ? 'text' : 'tool_call',
        content: `block-${idx}`,
        id: `tc-${idx}`,
        name: 'read',
        input: { file_path: 'README.md' },
        result: 'ok',
        isError: false,
      })),
    },
    meta: {},
  }
}

describe('ToolCallDisplay', () => {
  it('renders task trace blocks only after trace details is opened', async () => {
    const wrapper = mount(ToolCallDisplay, {
      props: {
        toolName: 'task',
        toolCallId: 'tc-1',
        toolResult: taskResult(3),
      },
      global: { plugins: [ElementPlus] },
    })

    await wrapper.find('.tool-header').trigger('click')
    expect(wrapper.find('details.task-trace').exists()).toBe(true)
    expect(wrapper.findAll('.task-trace-item')).toHaveLength(0)

    const details = wrapper.find('details.task-trace')
    ;(details.element as HTMLDetailsElement).open = true
    await details.trigger('toggle')
    await nextTick()

    expect(wrapper.findAll('.task-trace-item')).toHaveLength(3)
  })

  it('clears rendered trace blocks when tool panel is collapsed', async () => {
    const wrapper = mount(ToolCallDisplay, {
      props: {
        toolName: 'task',
        toolCallId: 'tc-2',
        toolResult: taskResult(2),
      },
      global: { plugins: [ElementPlus] },
    })

    await wrapper.find('.tool-header').trigger('click')
    const details = wrapper.find('details.task-trace')
    ;(details.element as HTMLDetailsElement).open = true
    await details.trigger('toggle')
    await nextTick()
    expect(wrapper.findAll('.task-trace-item')).toHaveLength(2)

    await wrapper.find('.tool-header').trigger('click')
    await nextTick()
    expect(wrapper.findAll('.task-trace-item')).toHaveLength(0)
  })

  it('does not truncate long task result text', async () => {
    const longText = 'x'.repeat(7000)
    const wrapper = mount(ToolCallDisplay, {
      props: {
        toolName: 'task',
        toolCallId: 'tc-long-task',
        toolResult: {
          kind: 'task',
          text: longText,
          success: true,
          error: null,
          data: {},
          meta: {},
        },
      },
      global: { plugins: [ElementPlus] },
    })

    await wrapper.find('.tool-header').trigger('click')
    const content = wrapper.find('.tool-content').text()
    expect(content).toContain(longText)
    expect(content).not.toContain('[truncated]')
  })

  it('keeps truncation for non-task long result text', async () => {
    const longText = 'y'.repeat(7000)
    const wrapper = mount(ToolCallDisplay, {
      props: {
        toolName: 'bash',
        toolCallId: 'tc-long-bash',
        toolResult: {
          kind: 'bash',
          text: longText,
          success: true,
          error: null,
          data: {},
          meta: {},
        },
      },
      global: { plugins: [ElementPlus] },
    })

    await wrapper.find('.tool-header').trigger('click')
    const content = wrapper.find('.tool-content').text()
    expect(content).toContain('[truncated]')
  })
})
