import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import Settings from '../../views/Settings.vue'

const { messageErrorMock, messageSuccessMock } = vi.hoisted(() => ({
  messageErrorMock: vi.fn(),
  messageSuccessMock: vi.fn(),
}))

const settingsStoreMock = vi.hoisted(() => ({
  providers: [
    {
      id: 'prov-1',
      name: 'OpenAI',
      provider: 'openai',
      endpoint_url: null,
      models: ['gpt-4o'],
      image_models: [],
      is_default: false,
      has_api_key: true,
    },
  ],
  modelDefaults: {
    chat_provider_id: null,
    chat_model: null,
    subagent_provider_id: null,
    subagent_model: null,
    image_provider_id: null,
    image_model: null,
  },
  presets: [],
  loadProviders: vi.fn(),
  loadPresets: vi.fn(),
  loadModelDefaults: vi.fn(),
  saveProvider: vi.fn(),
  removeProvider: vi.fn(),
  saveModelDefaults: vi.fn(),
  editPreset: vi.fn(),
  savePreset: vi.fn(),
  removePreset: vi.fn(),
}))

vi.mock('element-plus', () => ({
  ElMessageBox: {
    confirm: vi.fn(),
  },
  ElMessage: {
    error: messageErrorMock,
    success: messageSuccessMock,
  },
}))

vi.mock('../../stores/settings', () => ({
  useSettingsStore: () => settingsStoreMock,
}))

vi.mock('../../i18n', () => ({
  t: (key: string) => key,
}))

function mountView() {
  return mount(Settings, {
    global: {
      stubs: {
        LocaleToggle: true,
        'el-tabs': { template: '<div><slot /></div>' },
        'el-tab-pane': { template: '<div><slot /></div>' },
        'el-table': { template: '<div><slot /></div>' },
        'el-table-column': {
          template: '<div><slot :row="{ name: \'OpenAI\', provider: \'openai\', models: [\'gpt-4o\'], image_models: [], is_default: false, endpoint_url: null, id: \'preset-1\', description: \'desc\', content: \'content\' }" /></div>',
        },
        'el-tag': true,
        'el-divider': true,
        'el-form': { template: '<form><slot /></form>' },
        'el-form-item': { template: '<div><slot /></div>' },
        'el-input': true,
        'el-select': true,
        'el-option': true,
        'el-button': { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        'el-cascader': true,
        'el-checkbox': true,
        'el-alert': true,
      },
      mocks: {
        $router: { push: vi.fn() },
      },
    },
  })
}

describe('Settings view', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    settingsStoreMock.loadProviders.mockResolvedValue(undefined)
    settingsStoreMock.loadPresets.mockResolvedValue(undefined)
    settingsStoreMock.loadModelDefaults.mockResolvedValue(undefined)
    settingsStoreMock.saveModelDefaults.mockResolvedValue(undefined)
  })

  it('loads model defaults on mount', async () => {
    mountView()
    await flushPromises()

    expect(settingsStoreMock.loadProviders).toHaveBeenCalled()
    expect(settingsStoreMock.loadModelDefaults).toHaveBeenCalled()
  })

  it('shows validation error when required defaults are missing', async () => {
    const wrapper = mountView()
    await flushPromises()

    await (wrapper.vm as any).handleSaveModelDefaults()

    expect(messageErrorMock).toHaveBeenCalledWith('settings.defaults.validation.chatRequired')
    expect(settingsStoreMock.saveModelDefaults).not.toHaveBeenCalled()
  })

  it('renders single-line action wrapper in providers table', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.find('.row-actions').exists()).toBe(true)
  })
})
