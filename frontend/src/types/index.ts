export interface User {
  id: string
  username: string
  email: string
  is_admin: boolean
}

export interface AuthResponse {
  access_token: string
  refresh_token: string
  user: User
}

export interface Conversation {
  id: string
  title: string
  provider: string | null
  model_name: string | null
  system_prompt_override: string | null
  created_at: string
  updated_at: string
}

export interface Message {
  id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  tool_calls: string | null
  tool_call_id: string | null
  token_count: number | null
  created_at: string
}

export interface MessagesResponse {
  messages: Message[]
  total: number
}

export interface ProviderConfig {
  id: string
  provider: string
  endpoint_url: string | null
  model_name: string | null
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

// WebSocket message types
export interface WsMessage {
  type: string
  [key: string]: unknown
}
