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
    presets: [],
    defaultPreset: null,
    loadProviders: vi.fn(),
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
    chatStoreMock.loadConversations.mockResolvedValue(undefined)
    settingsStoreMock.loadProviders.mockResolvedValue(undefined)
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
})
