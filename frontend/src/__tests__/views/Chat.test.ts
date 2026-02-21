import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import Chat from '../../views/Chat.vue'

const { confirmMock, messageErrorMock, messageSuccessMock } = vi.hoisted(() => ({
  confirmMock: vi.fn(),
  messageErrorMock: vi.fn(),
  messageSuccessMock: vi.fn(),
}))

const { authStoreMock, chatStoreMock, settingsStoreMock } = vi.hoisted(() => ({
  authStoreMock: {
    isAuthenticated: false,
    logout: vi.fn(),
  },
  chatStoreMock: {
    conversations: [{ id: 'conv-1', title: 'A' }] as any[],
    currentConversationId: null as string | null,
    messages: [],
    isWaiting: false,
    isStreaming: false,
    streamingContent: '',
    streamingBlocks: [],
    wsConnected: true,
    sendFailed: false,
    loadConversations: vi.fn(),
    connectWs: vi.fn(),
    disconnectWs: vi.fn(),
    createConversation: vi.fn(),
    selectConversation: vi.fn(),
    deleteConversation: vi.fn(),
    sendMessage: vi.fn(),
    cancelGeneration: vi.fn(),
    editMessage: vi.fn(),
    regenerateMessage: vi.fn(),
    updateConversation: vi.fn(),
  },
  settingsStoreMock: {
    providers: [] as any[],
    modelDefaults: null as null | {
      chat_provider_id: string | null
      chat_model: string | null
      subagent_provider_id: string | null
      subagent_model: string | null
      image_provider_id: string | null
      image_model: string | null
    },
    presets: [],
    defaultPreset: null,
    loadProviders: vi.fn(),
    loadModelDefaults: vi.fn(),
    loadPresets: vi.fn(),
  },
}))

vi.mock('element-plus', () => ({
  ElMessageBox: {
    confirm: confirmMock,
  },
  ElMessage: {
    error: messageErrorMock,
    success: messageSuccessMock,
  },
}))

vi.mock('@element-plus/icons-vue', () => ({
  Plus: {},
  Fold: {},
  Expand: {},
  Setting: {},
  SwitchButton: {},
}))

vi.mock('../../stores/auth', () => ({
  useAuthStore: () => authStoreMock,
}))

vi.mock('../../stores/chat', () => ({
  useChatStore: () => chatStoreMock,
}))

vi.mock('../../stores/settings', () => ({
  useSettingsStore: () => settingsStoreMock,
}))

vi.mock('../../api/conversations', () => ({
  uploadFiles: vi.fn(),
}))

vi.mock('../../api/sharing', () => ({
  createShare: vi.fn(),
  revokeShare: vi.fn(),
}))

vi.mock('../../i18n', () => ({
  t: (key: string) => key,
}))

function mountView() {
  return mount(Chat, {
    global: {
      directives: {
        loading: {},
      },
      stubs: {
        ConversationList: {
          template: '<button data-testid="delete-conversation" @click="$emit(\'delete\', \'conv-1\')">delete</button>',
        },
        ChatMessage: true,
        ChatInput: true,
        QuestionFlow: true,
        FileBrowser: true,
        LocaleToggle: true,
        'el-button': true,
        'el-icon': true,
        'el-cascader': true,
        'el-drawer': true,
        'el-dialog': true,
        'el-form': true,
        'el-form-item': true,
        'el-select': true,
        'el-option': true,
        'el-input': true,
      },
      mocks: {
        $router: { push: vi.fn() },
      },
    },
  })
}

describe('Chat view - delete conversation', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    chatStoreMock.conversations = [{ id: 'conv-1', title: 'A' }]
    chatStoreMock.currentConversationId = null
    chatStoreMock.updateConversation.mockResolvedValue(undefined)
    settingsStoreMock.modelDefaults = null
    settingsStoreMock.providers = []
    chatStoreMock.loadConversations.mockResolvedValue(undefined)
    settingsStoreMock.loadProviders.mockResolvedValue(undefined)
    settingsStoreMock.loadModelDefaults.mockResolvedValue(undefined)
    settingsStoreMock.loadPresets.mockResolvedValue(undefined)
    chatStoreMock.deleteConversation.mockResolvedValue(undefined)
  })

  it('does not call delete when confirmation is cancelled', async () => {
    confirmMock.mockRejectedValueOnce(new Error('cancel'))
    const wrapper = mountView()
    await flushPromises()

    await wrapper.get('[data-testid="delete-conversation"]').trigger('click')
    await flushPromises()

    expect(confirmMock).toHaveBeenCalledOnce()
    expect(chatStoreMock.deleteConversation).not.toHaveBeenCalled()
  })

  it('shows error message when delete request fails after confirmation', async () => {
    confirmMock.mockResolvedValueOnce('ok')
    chatStoreMock.deleteConversation.mockRejectedValueOnce(new Error('delete failed'))
    const wrapper = mountView()
    await flushPromises()

    await wrapper.get('[data-testid="delete-conversation"]').trigger('click')
    await flushPromises()

    expect(chatStoreMock.deleteConversation).toHaveBeenCalledWith('conv-1')
    expect(messageErrorMock).toHaveBeenCalledWith('chat.messages.failedDeleteConversation')
  })

  it('does not create chat when required model defaults are missing', async () => {
    settingsStoreMock.modelDefaults = null
    const wrapper = mountView()
    await flushPromises()

    await (wrapper.vm as any).handleCreateChat()
    await flushPromises()

    expect(chatStoreMock.createConversation).not.toHaveBeenCalled()
    expect(messageErrorMock).toHaveBeenCalledWith('chat.messages.missingRequiredDefaults')
  })

  it('creates chat with three-role model defaults', async () => {
    settingsStoreMock.modelDefaults = {
      chat_provider_id: 'prov-main',
      chat_model: 'gpt-4o',
      subagent_provider_id: 'prov-sub',
      subagent_model: 'claude-3-opus',
      image_provider_id: 'prov-img',
      image_model: 'gemini-image-v1',
    }
    chatStoreMock.createConversation.mockResolvedValueOnce({ id: 'conv-new' })
    chatStoreMock.selectConversation.mockResolvedValueOnce(undefined)

    const wrapper = mountView()
    await flushPromises()

    await (wrapper.vm as any).handleCreateChat()
    await flushPromises()

    expect(chatStoreMock.createConversation).toHaveBeenCalledWith(
      undefined,
      undefined,
      'prov-main',
      'gpt-4o',
      'prov-img',
      'gemini-image-v1',
      'prov-sub',
      'claude-3-opus',
    )
    expect(chatStoreMock.selectConversation).toHaveBeenCalledWith('conv-new')
  })

  it('shows model warning message when conversation model config contains invalid roles', async () => {
    chatStoreMock.currentConversationId = 'conv-1'
    chatStoreMock.conversations = [{
      id: 'conv-1',
      title: 'A',
      provider_id: 'openai',
      model_name: 'gpt-4o',
      subagent_provider_id: 'openai',
      subagent_model: 'removed-subagent-model',
      image_provider_id: null,
      image_model: null,
      deep_thinking: true,
      thinking_budget: 128000,
      subagent_thinking_budget: 128000,
      share_token: null,
    }]
    settingsStoreMock.providers = [{
      id: 'openai',
      name: 'OpenAI',
      provider: 'openai',
      endpoint_url: null,
      models: ['gpt-4o', 'gpt-5.3-codex'],
      image_models: [],
      is_default: true,
      has_api_key: true,
    }]

    const wrapper = mountView()
    await flushPromises()

    expect((wrapper.vm as any).modelDraftWarningMessage).toBe(
      'chat.messages.invalidConversationModelConfigRoles',
    )
  })

  it('persists model draft across intermediate invalid state and submits full payload once valid', async () => {
    chatStoreMock.currentConversationId = 'conv-1'
    chatStoreMock.conversations = [{
      id: 'conv-1',
      title: 'A',
      provider_id: 'openai',
      model_name: 'gpt-4o',
      subagent_provider_id: 'openai',
      subagent_model: 'removed-subagent-model',
      image_provider_id: null,
      image_model: null,
      deep_thinking: true,
      thinking_budget: 128000,
      subagent_thinking_budget: 128000,
      share_token: null,
    }]
    settingsStoreMock.providers = [{
      id: 'openai',
      name: 'OpenAI',
      provider: 'openai',
      endpoint_url: null,
      models: ['gpt-4o', 'gpt-5.3-codex'],
      image_models: ['gpt-image-1'],
      is_default: true,
      has_api_key: true,
    }]

    const wrapper = mountView()
    await flushPromises()

    await (wrapper.vm as any).handleCascaderChange(['openai', 'gpt-5.3-codex'])
    await flushPromises()

    expect(chatStoreMock.updateConversation).not.toHaveBeenCalled()
    expect(messageErrorMock).toHaveBeenCalledWith('chat.messages.invalidSubagentModelSelection')

    await (wrapper.vm as any).handleSubagentCascaderChange(['openai', 'gpt-4o'])
    await flushPromises()

    expect(chatStoreMock.updateConversation).toHaveBeenCalledTimes(1)
    expect(chatStoreMock.updateConversation).toHaveBeenCalledWith('conv-1', {
      provider_id: 'openai',
      model_name: 'gpt-5.3-codex',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-4o',
      image_provider_id: '',
      image_model: '',
    })
  })

  it('catches updateConversation errors and surfaces backend message', async () => {
    chatStoreMock.currentConversationId = 'conv-1'
    chatStoreMock.conversations = [{
      id: 'conv-1',
      title: 'A',
      provider_id: 'openai',
      model_name: 'gpt-4o',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-4o',
      image_provider_id: null,
      image_model: null,
      deep_thinking: true,
      thinking_budget: 128000,
      subagent_thinking_budget: 128000,
      share_token: null,
    }]
    chatStoreMock.updateConversation.mockRejectedValueOnce({
      response: { data: { message: 'backend rejected update' } },
    })

    const wrapper = mountView()
    await flushPromises()

    await expect((wrapper.vm as any).toggleDeepThinking()).resolves.toBeUndefined()
    expect(messageErrorMock).toHaveBeenCalledWith('backend rejected update')
  })

  it('rolls back model draft to persisted conversation after failed model update', async () => {
    chatStoreMock.currentConversationId = 'conv-1'
    chatStoreMock.conversations = [{
      id: 'conv-1',
      title: 'A',
      provider_id: 'openai',
      model_name: 'gpt-4o',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-4o',
      image_provider_id: null,
      image_model: null,
      deep_thinking: true,
      thinking_budget: 128000,
      subagent_thinking_budget: 128000,
      share_token: null,
    }]
    settingsStoreMock.providers = [{
      id: 'openai',
      name: 'OpenAI',
      provider: 'openai',
      endpoint_url: null,
      models: ['gpt-4o', 'gpt-5.3-codex'],
      image_models: [],
      is_default: true,
      has_api_key: true,
    }]
    chatStoreMock.updateConversation.mockRejectedValueOnce({
      response: { status: 500, data: { message: 'model save failed' } },
    })

    const wrapper = mountView()
    await flushPromises()

    await (wrapper.vm as any).handleCascaderChange(['openai', 'gpt-5.3-codex'])
    await flushPromises()

    expect(messageErrorMock).toHaveBeenCalledWith('model save failed')
    expect((wrapper.vm as any).cascaderValue).toEqual(['openai', 'gpt-4o'])
  })

  it('serializes model draft submissions and applies the latest draft after in-flight request', async () => {
    chatStoreMock.currentConversationId = 'conv-1'
    chatStoreMock.conversations = [{
      id: 'conv-1',
      title: 'A',
      provider_id: 'openai',
      model_name: 'gpt-4o',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-4o',
      image_provider_id: null,
      image_model: null,
      deep_thinking: true,
      thinking_budget: 128000,
      subagent_thinking_budget: 128000,
      share_token: null,
    }]
    settingsStoreMock.providers = [{
      id: 'openai',
      name: 'OpenAI',
      provider: 'openai',
      endpoint_url: null,
      models: ['gpt-4o', 'gpt-5.3-codex'],
      image_models: [],
      is_default: true,
      has_api_key: true,
    }]

    let resolveFirstUpdate!: () => void
    chatStoreMock.updateConversation
      .mockImplementationOnce(() => new Promise<void>((resolve) => { resolveFirstUpdate = resolve }))
      .mockResolvedValueOnce(undefined)

    const wrapper = mountView()
    await flushPromises()

    const firstSubmit = (wrapper.vm as any).handleCascaderChange(['openai', 'gpt-5.3-codex'])
    await Promise.resolve()
    await (wrapper.vm as any).handleSubagentCascaderChange(['openai', 'gpt-5.3-codex'])

    expect(chatStoreMock.updateConversation).toHaveBeenCalledTimes(1)
    expect(chatStoreMock.updateConversation).toHaveBeenNthCalledWith(1, 'conv-1', {
      provider_id: 'openai',
      model_name: 'gpt-5.3-codex',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-4o',
      image_provider_id: '',
      image_model: '',
    })

    resolveFirstUpdate()
    await firstSubmit
    await flushPromises()

    expect(chatStoreMock.updateConversation).toHaveBeenCalledTimes(2)
    expect(chatStoreMock.updateConversation).toHaveBeenNthCalledWith(2, 'conv-1', {
      provider_id: 'openai',
      model_name: 'gpt-5.3-codex',
      subagent_provider_id: 'openai',
      subagent_model: 'gpt-5.3-codex',
      image_provider_id: '',
      image_model: '',
    })
  })
})
