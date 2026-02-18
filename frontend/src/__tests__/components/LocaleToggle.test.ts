import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import LocaleToggle from '../../components/LocaleToggle.vue'
import { currentLocale, setLocale } from '../../i18n'

describe('LocaleToggle', () => {
  beforeEach(() => {
    setLocale('en')
  })

  it('renders EN and Chinese buttons', () => {
    const wrapper = mount(LocaleToggle)
    expect(wrapper.get('[data-testid="locale-en"]').text()).toBe('EN')
    expect(wrapper.get('[data-testid="locale-zh"]').text()).toBe('中')
  })

  it('switches to zh-CN when clicking Chinese button', async () => {
    const wrapper = mount(LocaleToggle)
    await wrapper.get('[data-testid="locale-zh"]').trigger('click')
    expect(currentLocale.value).toBe('zh-CN')
  })

  it('marks active locale with aria-pressed', async () => {
    const wrapper = mount(LocaleToggle)
    expect(wrapper.get('[data-testid="locale-en"]').attributes('aria-pressed')).toBe('true')
    expect(wrapper.get('[data-testid="locale-zh"]').attributes('aria-pressed')).toBe('false')

    await wrapper.get('[data-testid="locale-zh"]').trigger('click')

    expect(wrapper.get('[data-testid="locale-en"]').attributes('aria-pressed')).toBe('false')
    expect(wrapper.get('[data-testid="locale-zh"]').attributes('aria-pressed')).toBe('true')
  })

  it('does not call setLocale when clicking current locale', async () => {
    const wrapper = mount(LocaleToggle)
    await wrapper.get('[data-testid="locale-en"]').trigger('click')
    expect(currentLocale.value).toBe('en')
  })
})
