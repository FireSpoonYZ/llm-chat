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
    conversations: [{ id: 'conv-1', title: 'A' }],
    currentConversationId: null,
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
    providers: [],
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
    settingsStoreMock.modelDefaults = null
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
})
