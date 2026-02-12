<template>
  <div class="chat-message" :class="message.role">
    <div class="message-avatar" :class="message.role">
      {{ message.role === 'user' ? 'U' : 'A' }}
    </div>
    <div class="message-body">
      <div class="message-header">
        <span class="message-sender">{{ message.role === 'user' ? 'You' : 'Assistant' }}</span>
        <span class="message-time">{{ formattedTime }}</span>
      </div>
      <template v-if="isEditing">
        <el-input
          class="edit-textarea"
          v-model="editContent"
          type="textarea"
          :rows="3"
          resize="vertical"
        />
        <div class="edit-actions">
          <el-button class="save-btn" type="primary" size="small" @click="saveEdit">Save</el-button>
          <el-button class="cancel-btn" size="small" @click="cancelEdit">Cancel</el-button>
        </div>
      </template>
      <template v-else>
        <template v-for="(block, idx) in contentBlocks" :key="idx">
          <div v-if="block.type === 'text'" class="message-content" v-html="renderMarkdown(block.content)"></div>
          <div v-else-if="block.type === 'thinking'" class="thinking-block">
            <details>
              <summary class="thinking-summary">Thinking</summary>
              <div class="thinking-content">{{ block.content }}</div>
            </details>
          </div>
          <ToolCallDisplay
            v-else-if="block.type === 'tool_call'"
            :tool-name="block.name"
            :tool-call-id="block.id"
            :tool-input="block.input"
            :tool-result="block.result"
            :is-error="block.isError"
            :is-loading="block.isLoading"
          />
        </template>
        <div class="message-footer">
          <el-button
            v-if="message.role === 'user' && !isStreaming"
            class="action-btn edit-btn"
            text
            size="small"
            title="Edit message"
            :icon="EditPen"
            @click="startEdit"
          />
          <el-button
            v-if="message.role === 'assistant' && !isStreaming"
            class="action-btn regenerate-btn"
            text
            size="small"
            title="Regenerate response"
            :icon="RefreshRight"
            @click="$emit('regenerate', message.id)"
          />
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { EditPen, RefreshRight } from '@element-plus/icons-vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import 'highlight.js/styles/github-dark.css'
import type { Message, ContentBlock } from '../types'
import ToolCallDisplay from './ToolCallDisplay.vue'

const props = defineProps<{
  message: Message
  isStreaming?: boolean
  streamingBlocks?: ContentBlock[]
  toolCalls?: Array<{
    id: string
    name: string
    input?: Record<string, unknown>
    result?: string
    isError?: boolean
    isLoading?: boolean
  }>
}>()

const emit = defineEmits<{
  edit: [messageId: string, newContent: string]
  regenerate: [messageId: string]
}>()

const isEditing = ref(false)
const editContent = ref('')

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(str: string, lang: string) {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return hljs.highlight(str, { language: lang }).value
      } catch {
        // fall through
      }
    }
    return ''
  },
})

const renderedContent = computed(() => {
  return md.render(props.message.content || '')
})

function renderMarkdown(text: string): string {
  return md.render(text || '')
}

const contentBlocks = computed<ContentBlock[]>(() => {
  if (props.streamingBlocks && props.streamingBlocks.length > 0) {
    return props.streamingBlocks
  }
  if (props.message.tool_calls) {
    try {
      const parsed = JSON.parse(props.message.tool_calls) as unknown[]
      if (Array.isArray(parsed) && parsed.length > 0) {
        const hasTypedBlocks = parsed.some(
          (item) => typeof item === 'object' && item !== null && 'type' in (item as object)
        )

        if (hasTypedBlocks) {
          return (parsed as Array<Record<string, unknown>>)
            .filter((item) => item.type === 'text' || item.type === 'thinking' || item.type === 'tool_call')
            .map((item) => {
              if (item.type === 'thinking') {
                return { type: 'thinking' as const, content: (item.content as string) || '' }
              }
              if (item.type === 'tool_call') {
                return {
                  type: 'tool_call' as const,
                  id: (item.id as string) || '',
                  name: (item.name as string) || '',
                  input: item.input as Record<string, unknown> | undefined,
                  result: item.result as string | undefined,
                  isError: (item.isError ?? item.is_error) as boolean | undefined,
                  isLoading: false,
                }
              }
              return { type: 'text' as const, content: (item.content as string) || '' }
            })
        }

        // Legacy format (no type field): text on top, tool calls below
        const blocks: ContentBlock[] = []
        if (props.message.content) {
          blocks.push({ type: 'text', content: props.message.content })
        }
        for (const tc of parsed as Array<Record<string, unknown>>) {
          blocks.push({
            type: 'tool_call',
            id: (tc.id as string) || '',
            name: (tc.name as string) || '',
            input: tc.input as Record<string, unknown> | undefined,
            result: tc.result as string | undefined,
            isError: (tc.isError ?? tc.is_error) as boolean | undefined,
            isLoading: false,
          })
        }
        return blocks
      }
    } catch {
      // fall through
    }
  }
  if (props.toolCalls && props.toolCalls.length > 0) {
    const blocks: ContentBlock[] = []
    if (props.message.content) {
      blocks.push({ type: 'text', content: props.message.content })
    }
    for (const tc of props.toolCalls) {
      blocks.push({
        type: 'tool_call',
        id: tc.id,
        name: tc.name,
        input: tc.input,
        result: tc.result,
        isError: tc.isError,
        isLoading: tc.isLoading,
      })
    }
    return blocks
  }
  return [{ type: 'text', content: props.message.content || '' }]
})

const formattedTime = computed(() => {
  if (!props.message.created_at) return ''
  const d = new Date(props.message.created_at)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
})

function startEdit() {
  editContent.value = props.message.content || ''
  isEditing.value = true
}

function saveEdit() {
  isEditing.value = false
  emit('edit', props.message.id, editContent.value)
}

function cancelEdit() {
  isEditing.value = false
  editContent.value = ''
}
</script>

<style scoped>
.chat-message {
  display: flex;
  gap: 14px;
  margin-bottom: 24px;
  padding: 0;
}
.chat-message.user {
  flex-direction: row-reverse;
}

.message-avatar {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-full);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 600;
  flex-shrink: 0;
  margin-top: 2px;
}
.message-avatar.user {
  background: var(--accent-primary);
  color: white;
}
.message-avatar.assistant {
  background: var(--border-light);
  color: var(--text-secondary);
}

.message-body {
  flex: 1;
  min-width: 0;
}

.chat-message.user .message-body {
  background: var(--bg-user-message);
  border-radius: var(--radius-lg);
  padding: 14px 18px;
}

.message-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}
.message-sender {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-secondary);
}
.message-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}
.message-time {
  font-size: 12px;
  color: var(--text-muted);
}

.message-footer {
  display: flex;
  gap: 6px;
  margin-top: 6px;
}

.action-btn {
  opacity: 0;
  transition: opacity 0.2s;
}
.chat-message:hover .action-btn {
  opacity: 1;
}

.edit-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.message-content {
  font-size: 15px;
  line-height: 1.7;
  color: var(--text-primary);
}
.message-content :deep(p) {
  margin: 0 0 12px;
}
.message-content :deep(p:last-child) {
  margin-bottom: 0;
}
.message-content :deep(pre) {
  background: var(--bg-code-block);
  color: #d4d4d4;
  padding: 16px;
  border-radius: var(--radius-md);
  overflow-x: auto;
  margin: 12px 0;
  font-size: 13px;
  line-height: 1.5;
}
.message-content :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 0.9em;
}
.message-content :deep(pre code) {
  background: none;
  padding: 0;
  font-size: inherit;
}
.message-content :deep(ul),
.message-content :deep(ol) {
  margin: 8px 0;
  padding-left: 24px;
}
.message-content :deep(a) {
  color: var(--accent-primary);
  text-decoration: none;
}
.message-content :deep(a:hover) {
  text-decoration: underline;
}

.thinking-block {
  margin: 8px 0;
  padding: 10px 12px;
  background: var(--border-light);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--text-secondary);
}
.thinking-summary {
  cursor: pointer;
  font-style: italic;
  user-select: none;
}
.thinking-content {
  margin-top: 8px;
  white-space: pre-wrap;
  font-style: italic;
  max-height: 300px;
  overflow-y: auto;
}
</style>