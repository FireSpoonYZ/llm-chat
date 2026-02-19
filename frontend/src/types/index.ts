export interface User {
  id: string
  username: string
  email: string
  is_admin: boolean
}

export interface AuthResponse {
  access_token?: string
  refresh_token?: string
  user: User
}

export interface Conversation {
  id: string
  title: string
  provider: string | null
  model_name: string | null
  subagent_provider?: string | null
  subagent_model?: string | null
  system_prompt_override: string | null
  deep_thinking: boolean
  thinking_budget: number | null
  subagent_thinking_budget?: number | null
  created_at: string
  updated_at: string
  image_provider: string | null
  image_model: string | null
  share_token: string | null
}

export interface SharedConversation {
  title: string
  model_name: string | null
  created_at: string
  updated_at: string
}

export interface Message {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  parts?: MessagePart[]
  tool_calls: string | null
  tool_call_id: string | null
  token_count: number | null
  created_at: string
}

export interface MessagePart {
  type: 'text' | 'reasoning' | 'tool_call' | 'tool_result' | string
  text: string | null
  json_payload: unknown | null
  tool_call_id: string | null
  seq: number | null
}

export interface MessagesResponse {
  messages: Message[]
  total: number
}

export interface ProviderConfig {
  id: string
  name: string
  provider: string
  endpoint_url: string | null
  models: string[]
  image_models: string[]
  is_default: boolean
  has_api_key: boolean
}

export interface McpServer {
  id: string
  name: string
  description: string | null
  transport: string
  is_enabled: boolean
}

export interface SystemPromptPreset {
  id: string
  name: string
  description: string
  content: string
  is_default: boolean
  created_at: string
  updated_at: string
}

export interface ToolCallInfo {
  id: string
  name: string
  input?: Record<string, unknown>
  result?: ToolResult
  isError?: boolean
  isLoading?: boolean
}

export interface ToolMediaRef {
  type: 'image' | 'video' | 'audio'
  name: string
  url: string
  mime?: string
  size?: number
}

export interface ToolResult {
  kind: string
  text: string
  success: boolean
  error?: string | null
  data?: Record<string, unknown> & {
    media?: ToolMediaRef[]
  }
  meta?: Record<string, unknown>
}

export interface QuestionItem {
  id: string
  header?: string | null
  question: string
  options?: string[]
  placeholder?: string | null
  multiple?: boolean
  required?: boolean
}

export interface QuestionAnswer {
  id: string
  question: string
  selected_options: string[]
  free_text: string
  notes: string
}

export interface ActiveQuestionnaire {
  questionnaire_id: string
  title?: string | null
  questions: QuestionItem[]
}

export type ContentBlock =
  | { type: 'text'; content: string }
  | { type: 'thinking'; content: string }
  | { type: 'tool_call'; id: string; name: string; input?: Record<string, unknown>;
      result?: ToolResult | string; isError?: boolean; isLoading?: boolean }

// WebSocket message — index-signature for backward compat with existing handlers
export interface WsMessage {
  type: string
  [key: string]: unknown
}

// Discriminated union for type-safe WS message handling (use with type narrowing)
export type WsMessageEvent =
  | { type: 'ws_connected' }
  | { type: 'ws_disconnected' }
  | { type: 'auth_failed' }
  | { type: 'message_saved'; message_id: string }
  | { type: 'assistant_delta'; delta: string }
  | { type: 'thinking_delta'; delta: string }
  | { type: 'tool_call'; tool_call_id: string; tool_name: string; tool_input?: Record<string, unknown> }
  | { type: 'tool_result'; tool_call_id: string; result: ToolResult | string; is_error: boolean }
  | {
      type: 'question'
      tool_call_id?: string
      questionnaire_id: string
      title?: string
      questions: QuestionItem[]
    }
  | {
      type: 'subagent_trace_delta'
      tool_call_id: string
      event_type: 'assistant_delta' | 'thinking_delta' | 'tool_call' | 'tool_result' | 'complete' | 'error' | string
      payload: Record<string, unknown>
    }
  | {
      type: 'task_trace_delta'
      // Legacy compatibility path for historical task tool runs.
      tool_call_id: string
      event_type: 'assistant_delta' | 'thinking_delta' | 'tool_call' | 'tool_result' | 'complete' | 'error' | string
      payload: Record<string, unknown>
    }
  | { type: 'complete'; message_id: string; content: string; tool_calls?: unknown[] }
  | { type: 'error'; message: string; code?: string }
  | { type: 'container_status'; status: string; reason?: string; message?: string }
  | {
      type: 'messages_truncated'
      after_message_id: string
      updated_content?: string
      updated_parts?: MessagePart[]
    }
  | { type: 'pong' }

export interface FileEntry {
  name: string
  is_dir: boolean
  size: number
  modified: string | null
  children?: FileEntry[]
}

export interface ListFilesResponse {
  path: string
  entries: FileEntry[]
}

export interface UploadedFile {
  name: string
  size: number
  path: string
}

export interface UploadResponse {
  uploaded: UploadedFile[]
}
