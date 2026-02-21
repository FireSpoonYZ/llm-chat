import { ref, watch } from 'vue'

export type Locale = 'en' | 'zh-CN'

type TranslateParams = Record<string, string | number>

const STORAGE_KEY = 'llm-chat.locale'

const messages = {
  en: {
    common: {
      language: 'Language',
      english: 'English',
      chinese: '中文',
      confirm: 'Confirm',
      cancel: 'Cancel',
      save: 'Save',
      delete: 'Delete',
      edit: 'Edit',
      actions: 'Actions',
      default: 'Default',
      copy: 'Copy',
      clear: 'Clear',
      create: 'Create',
      add: 'Add',
      update: 'Update',
      upload: 'Upload',
      refresh: 'Refresh',
      download: 'Download',
    },
    auth: {
      title: 'Claude Chat',
      username: 'Username',
      email: 'Email',
      password: 'Password',
      login: 'Login',
      register: 'Register',
      noAccount: "Don't have an account?",
      hasAccount: 'Already have an account?',
      validation: {
        usernameRequired: 'Username is required',
        emailRequired: 'Email is required',
        emailInvalid: 'Please enter a valid email',
        passwordRequired: 'Password is required',
        passwordMin: 'Password must be at least 8 characters',
      },
      messages: {
        loginFailed: 'Login failed',
        registrationFailed: 'Registration failed',
      },
    },
    chat: {
      newChat: 'New Chat',
      settings: 'Settings',
      logout: 'Logout',
      model: 'Model',
      subagent: 'Subagent',
      image: 'Image',
      selectProviderModel: 'Select provider / model',
      selectSubagentModel: 'Select subagent model',
      selectImageModel: 'Select image model',
      systemPrompt: 'System Prompt',
      files: 'Files',
      share: 'Share',
      connectionLost: 'Connection lost, reconnecting...',
      sendFailed: 'Failed to send message. Please check your connection.',
      emptyState: 'Select a conversation or start a new chat',
      dialog: {
        newChatTitle: 'New Chat',
        systemPromptPreset: 'System Prompt Preset',
        selectPreset: 'Select a preset',
        customPreset: 'Custom',
        systemPrompt: 'System Prompt',
        customPromptPlaceholder: 'Enter a custom system prompt or select a preset above',
        systemPromptTitle: 'System Prompt',
        conversationPromptPlaceholder: 'System prompt for this conversation',
        workspaceFilesTitle: 'Workspace Files',
        shareConversationTitle: 'Share Conversation',
        shareCreateHint: 'Anyone with the link can view this conversation in read-only mode. New messages will be visible when they refresh.',
        createLink: 'Create Link',
        shareActiveHint: 'This conversation is shared. Anyone with the link can view it.',
        stopSharing: 'Stop Sharing',
      },
      confirmDeleteConversation: 'Delete this conversation and its workspace files? This cannot be undone.',
      messages: {
        failedLoadConversations: 'Failed to load conversations',
        failedLoadProviders: 'Failed to load providers',
        failedLoadPromptPresets: 'Failed to load prompt presets',
        failedLoadModelDefaults: 'Failed to load default model settings',
        failedCreateChat: 'Failed to create chat',
        failedLoadConversation: 'Failed to load conversation',
        failedDeleteConversation: 'Failed to delete conversation',
        uploadedFiles: 'Uploaded {count} file(s)',
        uploadFailed: 'Upload failed',
        systemPromptUpdated: 'System prompt updated',
        failedCreateShareLink: 'Failed to create share link',
        failedRevokeShareLink: 'Failed to revoke share link',
        linkCopied: 'Link copied',
        copyFailed: 'Copy failed',
        editNotApplied: 'Edit was not applied. Please try again.',
        regenerateNotApplied: 'Regenerate request was not applied. Please try again.',
        operationTimeout: 'Operation timed out before confirmation. Synced with latest server state.',
        missingRequiredDefaults: 'Please configure default main/subagent models in Settings first.',
        failedUpdateConversation: 'Failed to update conversation settings.',
        modelUpdateFailed: 'Failed to update conversation model settings.',
        invalidConversationModelConfigRoles: 'Current model settings are invalid ({roles}). Please reselect them.',
        invalidMainModelSelection: 'Current main model setting is invalid. Please reselect it.',
        invalidSubagentModelSelection: 'Current subagent model setting is invalid. Please reselect it.',
        invalidImageModelSelection: 'Current image model setting is invalid. Please reselect it or clear it.',
        invalidImageModelPair: 'Image provider and image model must be selected together.',
      },
    },
    settings: {
      backToChat: 'Back to Chat',
      title: 'Settings',
      tabs: {
        providers: 'Providers',
        prompts: 'System Prompts',
      },
      table: {
        name: 'Name',
        apiType: 'API Type',
        models: 'Models',
        imageModels: 'Image Models',
        noModels: 'No models',
        description: 'Description',
      },
      provider: {
        addTitle: 'Add Provider',
        editTitle: 'Edit Provider',
        name: 'Name',
        apiType: 'API Type',
        apiKey: 'API Key',
        models: 'Models',
        modelsOptional: 'Models (optional)',
        imageModelsOptional: 'Image Models (optional)',
        customEndpointOptional: 'Custom Endpoint (optional)',
        setDefault: 'Set as default',
        placeholders: {
          name: 'e.g. My OpenAI, Work Anthropic',
          apiType: 'Select API type',
          apiKeyKeep: '(leave empty to keep current)',
          apiKeyNew: 'sk-...',
          model: 'Enter model name',
          imageModel: 'Enter image model name',
          endpoint: 'https://...',
        },
        validation: {
          nameRequired: 'Name is required',
          apiTypeRequired: 'API type is required',
          apiKeyRequired: 'API key is required',
          modelRequired: 'Add at least one model',
          atLeastOneModelRequired: 'Add at least one chat model or image model',
        },
        messages: {
          updated: 'Provider updated',
          saved: 'Provider saved',
          saveFailed: 'Failed to save provider',
          deleted: 'Provider deleted',
          deleteFailed: 'Failed to delete provider',
        },
        confirmDelete: 'Delete this provider? This cannot be undone.',
      },
      defaults: {
        title: 'Default Models',
        description: 'Defaults apply to newly created conversations only. Existing conversations are unchanged.',
        missingRequired: 'Main model and subagent model defaults are required.',
        chatModel: 'Main Model (required)',
        subagentModel: 'Subagent Model (required)',
        imageModelOptional: 'Image Model (optional)',
        validation: {
          chatRequired: 'Please select a default main model',
          subagentRequired: 'Please select a default subagent model',
          imagePairRequired: 'Image provider and image model must be selected together',
        },
        messages: {
          saved: 'Default models saved',
          saveFailed: 'Failed to save default models',
        },
      },
      preset: {
        addTitle: 'Add Preset',
        editTitle: 'Edit Preset',
        name: 'Name',
        description: 'Description',
        content: 'Content',
        setDefault: 'Set as default',
        placeholders: {
          name: 'Preset name',
          description: 'Short description',
          content: 'System prompt content',
        },
        validation: {
          nameRequired: 'Name is required',
          contentRequired: 'Content is required',
        },
        messages: {
          updated: 'Preset updated',
          saved: 'Preset saved',
          saveFailed: 'Failed to save preset',
          deleted: 'Preset deleted',
          deleteFailed: 'Failed to delete preset',
        },
        confirmDelete: 'Delete this preset? This cannot be undone.',
      },
    },
    shared: {
      notFoundTitle: 'Not Found',
      notFoundDescription: "This shared conversation doesn't exist or has been revoked.",
      readOnly: 'Read-only',
      loadMore: 'Load more',
      failedLoadMore: 'Failed to load more messages',
    },
    input: {
      attachFiles: 'Attach files',
      typeMessage: 'Type a message...',
      stopGeneration: 'Stop generation',
      sendMessage: 'Send message',
      deepThinking: 'Deep Thinking',
      budget: 'Budget',
      subagentBudget: 'Subagent Budget',
      defaultBudget: 'default',
    },
    conversation: {
      deleteConversation: 'Delete conversation',
    },
    message: {
      you: 'You',
      assistant: 'Assistant',
      save: 'Save',
      cancel: 'Cancel',
      thinking: 'Thinking',
      editMessage: 'Edit message',
      regenerateResponse: 'Regenerate response',
      copyMessage: 'Copy message',
      copied: 'Copied',
      copyFailed: 'Copy failed',
    },
    fileBrowser: {
      workspace: 'workspace',
      all: 'All',
      noFiles: 'No files',
      selectedCount: '{count} selected',
      workspaceSelected: 'Workspace selected',
      downloading: 'Downloading...',
      failedLoadFiles: 'Failed to load files',
      loadChildrenFailed: 'Failed to load folder contents',
      tooManySelections: 'Too many selected items. Please reduce and try again.',
      uploadedFiles: 'Uploaded {count} file(s)',
      uploadFailed: 'Upload failed',
    },
    mcp: {
      title: 'MCP Servers',
      description: 'Enable MCP servers for this conversation. The AI agent will be able to use tools provided by enabled servers.',
      empty: 'No MCP servers available',
    },
    questionnaire: {
      title: 'Clarification Questions',
      progress: 'Question {current}/{total}',
      previous: 'Previous',
      next: 'Next',
      submit: 'Submit Answers',
      freeTextPlaceholder: 'Add details for this question (optional)',
      notesPlaceholder: 'Additional notes for this question',
      selectedOptions: 'Selected options',
      freeText: 'Free text',
      notes: 'Notes',
      questionLabel: 'Question',
    },
    tool: {
      input: 'Input',
      error: 'Error',
      result: 'Result',
      subagentTrace: 'Subagent Trace',
      running: 'Running...',
      done: 'Done',
      truncated: '... [truncated]',
    },
    store: {
      toolFailed: '{tool} failed: {message}',
      unknownError: 'Unknown error',
      errorMessage: '[Error: {message}]',
      containerDisconnected: '[Container disconnected]',
      containerDisconnectedWarning: 'Container disconnected unexpectedly',
    },
  },
  'zh-CN': {
    common: {
      language: '语言',
      english: 'English',
      chinese: '中文',
      confirm: '确认',
      cancel: '取消',
      save: '保存',
      delete: '删除',
      edit: '编辑',
      actions: '操作',
      default: '默认',
      copy: '复制',
      clear: '清空',
      create: '创建',
      add: '添加',
      update: '更新',
      upload: '上传',
      refresh: '刷新',
      download: '下载',
    },
    auth: {
      title: 'Claude Chat',
      username: '用户名',
      email: '邮箱',
      password: '密码',
      login: '登录',
      register: '注册',
      noAccount: '还没有账号？',
      hasAccount: '已有账号？',
      validation: {
        usernameRequired: '请输入用户名',
        emailRequired: '请输入邮箱',
        emailInvalid: '请输入有效的邮箱地址',
        passwordRequired: '请输入密码',
        passwordMin: '密码至少需要 8 个字符',
      },
      messages: {
        loginFailed: '登录失败',
        registrationFailed: '注册失败',
      },
    },
    chat: {
      newChat: '新建会话',
      settings: '设置',
      logout: '退出登录',
      model: '模型',
      subagent: '子代理',
      image: '图像',
      selectProviderModel: '选择提供商 / 模型',
      selectSubagentModel: '选择子代理模型',
      selectImageModel: '选择图像模型',
      systemPrompt: '系统提示词',
      files: '文件',
      share: '分享',
      connectionLost: '连接已断开，正在重连...',
      sendFailed: '消息发送失败，请检查网络连接。',
      emptyState: '请选择一个会话或新建会话',
      dialog: {
        newChatTitle: '新建会话',
        systemPromptPreset: '系统提示词预设',
        selectPreset: '选择预设',
        customPreset: '自定义',
        systemPrompt: '系统提示词',
        customPromptPlaceholder: '输入自定义系统提示词，或选择上方预设',
        systemPromptTitle: '系统提示词',
        conversationPromptPlaceholder: '当前会话的系统提示词',
        workspaceFilesTitle: '工作区文件',
        shareConversationTitle: '分享会话',
        shareCreateHint: '任何拥有链接的人都可以只读查看此会话。对方刷新后可看到新消息。',
        createLink: '创建链接',
        shareActiveHint: '此会话已分享，任何拥有链接的人都可以查看。',
        stopSharing: '停止分享',
      },
      confirmDeleteConversation: '确定删除这个会话及其工作区文件吗？此操作不可撤销。',
      messages: {
        failedLoadConversations: '加载会话失败',
        failedLoadProviders: '加载提供商失败',
        failedLoadPromptPresets: '加载提示词预设失败',
        failedLoadModelDefaults: '加载默认模型配置失败',
        failedCreateChat: '创建会话失败',
        failedLoadConversation: '加载会话失败',
        failedDeleteConversation: '删除会话失败',
        uploadedFiles: '已上传 {count} 个文件',
        uploadFailed: '上传失败',
        systemPromptUpdated: '系统提示词已更新',
        failedCreateShareLink: '创建分享链接失败',
        failedRevokeShareLink: '取消分享失败',
        linkCopied: '链接已复制',
        copyFailed: '复制失败',
        editNotApplied: '编辑未生效，请重试。',
        regenerateNotApplied: '重新生成请求未生效，请重试。',
        operationTimeout: '操作确认超时，已同步为服务器最新状态。',
        missingRequiredDefaults: '请先在设置中配置默认主模型和子代理模型。',
        failedUpdateConversation: '更新会话设置失败。',
        modelUpdateFailed: '更新会话模型设置失败。',
        invalidConversationModelConfigRoles: '当前模型配置无效（{roles}），请重新选择后再保存。',
        invalidMainModelSelection: '当前主模型配置无效，请重新选择主模型。',
        invalidSubagentModelSelection: '当前子代理模型配置无效，请重新选择子代理模型。',
        invalidImageModelSelection: '当前图像模型配置无效，请重新选择或清空图像模型。',
        invalidImageModelPair: '图像提供商和图像模型需要同时选择。',
      },
    },
    settings: {
      backToChat: '返回聊天',
      title: '设置',
      tabs: {
        providers: '提供商',
        prompts: '系统提示词',
      },
      table: {
        name: '名称',
        apiType: 'API 类型',
        models: '模型',
        imageModels: '图像模型',
        noModels: '暂无模型',
        description: '描述',
      },
      provider: {
        addTitle: '添加提供商',
        editTitle: '编辑提供商',
        name: '名称',
        apiType: 'API 类型',
        apiKey: 'API Key',
        models: '模型',
        modelsOptional: '模型（可选）',
        imageModelsOptional: '图像模型（可选）',
        customEndpointOptional: '自定义地址（可选）',
        setDefault: '设为默认',
        placeholders: {
          name: '例如：我的 OpenAI、公司 Anthropic',
          apiType: '选择 API 类型',
          apiKeyKeep: '（留空则保持不变）',
          apiKeyNew: 'sk-...',
          model: '输入模型名称',
          imageModel: '输入图像模型名称',
          endpoint: 'https://...',
        },
        validation: {
          nameRequired: '请输入名称',
          apiTypeRequired: '请选择 API 类型',
          apiKeyRequired: '请输入 API Key',
          modelRequired: '请至少添加一个模型',
          atLeastOneModelRequired: '请至少添加一个对话模型或图像模型',
        },
        messages: {
          updated: '提供商已更新',
          saved: '提供商已保存',
          saveFailed: '保存提供商失败',
          deleted: '提供商已删除',
          deleteFailed: '删除提供商失败',
        },
        confirmDelete: '确定删除这个提供商吗？此操作不可撤销。',
      },
      defaults: {
        title: '默认模型',
        description: '默认值仅作用于新建会话，已有会话不会被改写。',
        missingRequired: '默认主模型和默认子代理模型为必填。',
        chatModel: '主模型（必填）',
        subagentModel: '子代理模型（必填）',
        imageModelOptional: '图像模型（可选）',
        validation: {
          chatRequired: '请选择默认主模型',
          subagentRequired: '请选择默认子代理模型',
          imagePairRequired: '图像提供商和图像模型需要同时设置',
        },
        messages: {
          saved: '默认模型已保存',
          saveFailed: '保存默认模型失败',
        },
      },
      preset: {
        addTitle: '添加预设',
        editTitle: '编辑预设',
        name: '名称',
        description: '描述',
        content: '内容',
        setDefault: '设为默认',
        placeholders: {
          name: '预设名称',
          description: '简短描述',
          content: '系统提示词内容',
        },
        validation: {
          nameRequired: '请输入名称',
          contentRequired: '请输入内容',
        },
        messages: {
          updated: '预设已更新',
          saved: '预设已保存',
          saveFailed: '保存预设失败',
          deleted: '预设已删除',
          deleteFailed: '删除预设失败',
        },
        confirmDelete: '确定删除这个预设吗？此操作不可撤销。',
      },
    },
    shared: {
      notFoundTitle: '未找到',
      notFoundDescription: '该分享会话不存在或已被撤销。',
      readOnly: '只读',
      loadMore: '加载更多',
      failedLoadMore: '加载更多消息失败',
    },
    input: {
      attachFiles: '添加文件',
      typeMessage: '输入消息...',
      stopGeneration: '停止生成',
      sendMessage: '发送消息',
      deepThinking: '深度思考',
      budget: '预算',
      subagentBudget: '子代理预算',
      defaultBudget: '默认',
    },
    conversation: {
      deleteConversation: '删除会话',
    },
    message: {
      you: '你',
      assistant: '助手',
      save: '保存',
      cancel: '取消',
      thinking: '思考中',
      editMessage: '编辑消息',
      regenerateResponse: '重新生成回复',
      copyMessage: '复制消息',
      copied: '已复制',
      copyFailed: '复制失败',
    },
    fileBrowser: {
      workspace: '工作区',
      all: '全选',
      noFiles: '暂无文件',
      selectedCount: '已选择 {count} 项',
      workspaceSelected: '已全选工作区',
      downloading: '下载中...',
      failedLoadFiles: '加载文件失败',
      loadChildrenFailed: '加载目录内容失败',
      tooManySelections: '选择项过多，请减少后重试',
      uploadedFiles: '已上传 {count} 个文件',
      uploadFailed: '上传失败',
    },
    mcp: {
      title: 'MCP 服务器',
      description: '为当前会话启用 MCP 服务器。AI 代理将可以使用已启用服务器提供的工具。',
      empty: '暂无 MCP 服务器',
    },
    questionnaire: {
      title: '澄清问题',
      progress: '问题 {current}/{total}',
      previous: '上一题',
      next: '下一题',
      submit: '提交答案',
      freeTextPlaceholder: '可补充本题细节（可选）',
      notesPlaceholder: '本题附加说明',
      selectedOptions: '已选项',
      freeText: '文本回答',
      notes: '备注',
      questionLabel: '问题',
    },
    tool: {
      input: '输入',
      error: '错误',
      result: '结果',
      subagentTrace: '子代理轨迹',
      running: '运行中...',
      done: '完成',
      truncated: '... [已截断]',
    },
    store: {
      toolFailed: '{tool} 执行失败：{message}',
      unknownError: '未知错误',
      errorMessage: '[错误：{message}]',
      containerDisconnected: '[容器已断开]',
      containerDisconnectedWarning: '容器意外断开连接',
    },
  },
} as const

function normalizeLocale(rawLocale: string): Locale {
  return rawLocale.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}

function getStorage(): Storage | null {
  if (typeof window === 'undefined') return null
  try {
    const storage = window.localStorage as Storage | undefined
    if (!storage) return null
    if (typeof storage.getItem !== 'function' || typeof storage.setItem !== 'function') return null
    return storage
  } catch {
    return null
  }
}

function getStoredLocale(): Locale | null {
  const storage = getStorage()
  if (!storage) return null
  const stored = storage.getItem(STORAGE_KEY)
  if (!stored) return null
  return stored === 'zh-CN' ? 'zh-CN' : stored === 'en' ? 'en' : null
}

function detectLocale(): Locale {
  const stored = getStoredLocale()
  if (stored) return stored
  if (typeof navigator === 'undefined') return 'en'
  return normalizeLocale(navigator.language)
}

export const currentLocale = ref<Locale>(detectLocale())

function resolveMessage(locale: Locale, key: string): string | undefined {
  const parts = key.split('.')
  let cursor: unknown = messages[locale]
  for (const part of parts) {
    if (!cursor || typeof cursor !== 'object' || !(part in (cursor as Record<string, unknown>))) {
      return undefined
    }
    cursor = (cursor as Record<string, unknown>)[part]
  }
  return typeof cursor === 'string' ? cursor : undefined
}

function interpolate(template: string, params?: TranslateParams): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (_match, name: string) => String(params[name] ?? `{${name}}`))
}

export function t(key: string, params?: TranslateParams): string {
  const direct = resolveMessage(currentLocale.value, key)
  if (direct) return interpolate(direct, params)
  const fallback = resolveMessage('en', key)
  if (fallback) return interpolate(fallback, params)
  return key
}

export function setLocale(locale: Locale): void {
  currentLocale.value = locale
}

export function initI18n(): void {
  currentLocale.value = detectLocale()
}

watch(currentLocale, (locale) => {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale
  }
  const storage = getStorage()
  if (storage) {
    storage.setItem(STORAGE_KEY, locale)
  }
}, { immediate: true })
