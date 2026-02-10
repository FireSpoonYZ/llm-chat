<template>
  <div class="chat-message" :class="message.role">
    <div class="message-header">
      <strong>{{ message.role === 'user' ? 'You' : 'Assistant' }}</strong>
      <span class="message-time">{{ formattedTime }}</span>
    </div>
    <div class="message-content" v-html="renderedContent"></div>
    <ToolCallDisplay
      v-for="tc in toolCalls"
      :key="tc.id"
      :tool-name="tc.name"
      :tool-call-id="tc.id"
      :tool-input="tc.input"
      :tool-result="tc.result"
      :is-error="tc.isError"
      :is-loading="tc.isLoading"
    />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import 'highlight.js/styles/github-dark.css'
import type { Message } from '../types'
import ToolCallDisplay from './ToolCallDisplay.vue'

const props = defineProps<{
  message: Message
  toolCalls?: Array<{
    id: string
    name: string
    input?: Record<string, unknown>
    result?: string
    isError?: boolean
    isLoading?: boolean
  }>
}>()

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

const formattedTime = computed(() => {
  if (!props.message.created_at) return ''
  const d = new Date(props.message.created_at)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
})
</script>

<style scoped>
.chat-message {
  margin-bottom: 16px;
  padding: 12px 16px;
  border-radius: 8px;
}
.chat-message.user {
  background: #ecf5ff;
}
.chat-message.assistant {
  background: #f5f7fa;
}
.chat-message.tool {
  background: #fdf6ec;
  font-family: monospace;
  font-size: 0.9em;
}
.message-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
  font-size: 0.85em;
  color: #909399;
}
.message-time {
  font-size: 0.8em;
}
.message-content :deep(pre) {
  background: #1e1e1e;
  color: #d4d4d4;
  padding: 12px;
  border-radius: 4px;
  overflow-x: auto;
}
.message-content :deep(code) {
  background: #f0f0f0;
  padding: 2px 4px;
  border-radius: 3px;
  font-size: 0.9em;
}
.message-content :deep(pre code) {
  background: none;
  padding: 0;
}
</style>
