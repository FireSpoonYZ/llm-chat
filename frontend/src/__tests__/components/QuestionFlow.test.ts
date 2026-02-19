import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ElementPlus from 'element-plus'
import QuestionFlow from '../../components/QuestionFlow.vue'

const questionnaire = {
  questionnaire_id: 'qq-1',
  title: 'Clarify requirements',
  questions: [
    {
      id: 'q1',
      header: 'Scope',
      question: 'Which environment?',
      options: ['prod', 'staging'],
      multiple: false,
      required: true,
      placeholder: null,
    },
    {
      id: 'q2',
      question: 'Any extra constraints?',
      options: [],
      multiple: false,
      required: false,
      placeholder: 'constraints',
    },
  ],
}

describe('QuestionFlow', () => {
  it('collects all answers and notes and submits once at the end', async () => {
    const wrapper = mount(QuestionFlow, {
      props: {
        questionnaire,
      },
      global: { plugins: [ElementPlus] },
    })

    // Q1: single choice + notes
    const radioInput = wrapper.find('input[type="radio"]')
    await radioInput.setValue(true)
    const textareas = wrapper.findAll('textarea')
    await textareas[1].setValue('note-q1')

    const nextButton = wrapper.get('[data-testid="question-next"]')
    await nextButton.trigger('click')

    // Q2: free text + notes
    const q2Textareas = wrapper.findAll('textarea')
    await q2Textareas[0].setValue('must support offline mode')
    await q2Textareas[1].setValue('note-q2')

    const submitButton = wrapper.get('[data-testid="question-submit"]')
    await submitButton.trigger('click')

    const emitted = wrapper.emitted('submit')
    expect(emitted).toBeTruthy()
    const answers = emitted![0][0] as Array<Record<string, unknown>>
    expect(answers).toHaveLength(2)
    expect(answers[0].selected_options).toEqual(['prod'])
    expect(answers[0].notes).toBe('note-q1')
    expect(answers[1].free_text).toBe('must support offline mode')
    expect(answers[1].notes).toBe('note-q2')
  })
})
