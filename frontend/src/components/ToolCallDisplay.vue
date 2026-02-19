<template>
  <div class="tool-call-display">
    <div class="tool-header" @click="toggleExpanded">
      <el-icon class="tool-icon">
        <component :is="toolIcon" />
      </el-icon>
      <span class="tool-name">{{ toolName }}</span>
      <el-tag :type="statusType" size="small" class="tool-status">
        {{ statusLabel }}
      </el-tag>
      <el-icon class="expand-icon">
        <ArrowDown v-if="!expanded" />
        <ArrowUp v-else />
      </el-icon>
    </div>
    <el-collapse-transition>
      <div v-show="expanded" class="tool-body">
        <div v-if="toolInput" class="tool-section">
          <div class="section-label">{{ t('tool.input') }}</div>
          <pre class="tool-content">{{ formattedInput }}</pre>
        </div>
        <div v-if="toolResult" class="tool-section">
          <div class="section-label">
            {{ isError ? t('tool.error') : t('tool.result') }}
          </div>
          <div v-if="bashMeta" class="bash-meta">{{ bashMeta }}</div>
          <div v-if="questionAnswers.length > 0" class="question-answers">
            <div v-for="(answer, i) in questionAnswers" :key="`${answer.id || i}`" class="question-answer-item">
              <div class="question-answer-title">
                {{ answer.question || `${t('questionnaire.questionLabel')} ${i + 1}` }}
              </div>
              <div v-if="answer.selected_options.length > 0" class="question-answer-line">
                <strong>{{ t('questionnaire.selectedOptions') }}:</strong>
                {{ answer.selected_options.join(', ') }}
              </div>
              <div v-if="answer.free_text" class="question-answer-line">
                <strong>{{ t('questionnaire.freeText') }}:</strong>
                {{ answer.free_text }}
              </div>
              <div v-if="answer.notes" class="question-answer-line">
                <strong>{{ t('questionnaire.notes') }}:</strong>
                {{ answer.notes }}
              </div>
            </div>
          </div>
          <details
            v-if="taskTrace.length > 0"
            class="task-trace"
            @toggle="handleTaskTraceToggle"
          >
            <summary>{{ t('tool.subagentTrace') }}</summary>
            <div v-if="taskTraceExpanded" class="task-trace-list">
              <div v-for="(block, i) in taskTrace" :key="i" class="task-trace-item">
                <div class="task-trace-head">{{ traceBlockTitle(block, i) }}</div>
                <pre class="tool-content">{{ traceBlockContent(block) }}</pre>
              </div>
            </div>
          </details>
          <div v-if="mediaRefs.length > 0" class="tool-media">
            <template v-for="(media, i) in mediaRefs" :key="i">
              <img v-if="media.type === 'image'" :src="media.url" :alt="media.name" class="tool-media-img" loading="lazy" />
              <video v-else-if="media.type === 'video'" controls preload="metadata" :src="media.url" class="tool-media-video" />
              <audio v-else-if="media.type === 'audio'" controls preload="metadata" :src="media.url" class="tool-media-audio" />
            </template>
          </div>
          <pre v-if="cleanedResult && !(toolName === 'question' && questionAnswers.length > 0)" class="tool-content" :class="{ 'tool-error': isError }">{{
            cleanedResult
          }}</pre>
        </div>
      </div>
    </el-collapse-transition>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  ArrowDown,
  ArrowUp,
  Monitor,
  Document,
  Edit,
  Search,
  Link,
  VideoPlay,
  Picture,
  ChatLineSquare,
} from '@element-plus/icons-vue'
import { fileViewUrl, sharedFileViewUrl } from '../utils/fileUrl'
import type { ToolResult, ToolMediaRef } from '../types'
import { t } from '../i18n'

const props = defineProps<{
  toolName: string
  toolCallId: string
  toolInput?: Record<string, unknown>
  toolResult?: ToolResult | string
  isError?: boolean
  isLoading?: boolean
  conversationId?: string
  shareToken?: string
}>()

const expanded = ref(false)
const taskTraceExpanded = ref(false)
const SUBAGENT_TOOL_NAMES = new Set(['explore', 'task'])

const MAX_RESULT_LENGTH = 5000

const toolIcon = computed(() => {
  switch (props.toolName) {
    case 'bash':
      return Monitor
    case 'read':
    case 'write':
    case 'list':
      return Document
    case 'edit':
      return Edit
    case 'glob':
    case 'grep':
      return Search
    case 'web_fetch':
      return Link
    case 'code_interpreter':
      return VideoPlay
    case 'image_generation':
      return Picture
    case 'question':
      return ChatLineSquare
    default:
      return Monitor
  }
})

const IMAGE_EXTS = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg']
const VIDEO_EXTS = ['.mp4', '.webm', '.mov']
const AUDIO_EXTS = ['.mp3', '.wav', '.ogg', '.m4a']

type MediaRef = ToolMediaRef

const SANDBOX_RE = /sandbox:\/\/\/([^\s)]+)/g

function normalizeToolResult(raw: ToolResult | string | undefined): ToolResult {
  if (typeof raw === 'string') {
    return { kind: 'text', text: raw, success: true, error: null, data: {}, meta: {} }
  }
  if (raw && typeof raw.kind === 'string' && typeof raw.text === 'string' && typeof raw.success === 'boolean') {
    return raw
  }
  // Legacy structured fallback
  if (raw && typeof raw.text === 'string') {
    return { kind: raw.kind || 'text', text: raw.text, success: true, error: null, data: {}, meta: {} }
  }
  return { kind: 'text', text: '', success: true, error: null, data: {}, meta: {} }
}

const normalizedResult = computed(() => normalizeToolResult(props.toolResult))

const mediaRefs = computed<MediaRef[]>(() => {
  if (!props.conversationId && !props.shareToken) return []

  const dataMedia = normalizedResult.value.data?.media
  if (Array.isArray(dataMedia) && dataMedia.length > 0) {
    return dataMedia
      .filter((m): m is MediaRef =>
        typeof m === 'object' && m !== null &&
        (m.type === 'image' || m.type === 'video' || m.type === 'audio') &&
        typeof m.url === 'string' &&
        typeof m.name === 'string'
      )
      .map((m) => {
        if (m.url.startsWith('sandbox:///')) {
          const path = m.url.replace('sandbox:///', '/')
          const url = props.shareToken
            ? sharedFileViewUrl(props.shareToken, path)
            : fileViewUrl(props.conversationId!, path)
          return { ...m, url }
        }
        return m
      })
  }

  if (!normalizedResult.value.text) return []
  const refs: MediaRef[] = []
  let match: RegExpExecArray | null
  const re = new RegExp(SANDBOX_RE.source, 'g')
  while ((match = re.exec(normalizedResult.value.text)) !== null) {
    const path = match[1]
    const ext = path.substring(path.lastIndexOf('.')).toLowerCase()
    const name = path.substring(path.lastIndexOf('/') + 1)
    const url = props.shareToken
      ? sharedFileViewUrl(props.shareToken, '/' + path)
      : fileViewUrl(props.conversationId!, '/' + path)
    if (IMAGE_EXTS.includes(ext)) {
      refs.push({ type: 'image', url, name })
    } else if (VIDEO_EXTS.includes(ext)) {
      refs.push({ type: 'video', url, name })
    } else if (AUDIO_EXTS.includes(ext)) {
      refs.push({ type: 'audio', url, name })
    }
  }
  return refs
})

const statusType = computed(() => {
  if (props.isLoading) return 'warning'
  if (props.isError) return 'danger'
  return 'success'
})

const statusLabel = computed(() => {
  if (props.isLoading) return t('tool.running')
  if (props.isError) return t('tool.error')
  return t('tool.done')
})

const formattedInput = computed(() => {
  if (!props.toolInput) return ''
  return JSON.stringify(props.toolInput, null, 2)
})

const truncatedResult = computed(() => {
  const text = normalizedResult.value.text || ''
  if (SUBAGENT_TOOL_NAMES.has(props.toolName)) {
    return text
  }
  if (text.length > MAX_RESULT_LENGTH) {
    return text.slice(0, MAX_RESULT_LENGTH) + `\n${t('tool.truncated')}`
  }
  return text
})

const SANDBOX_MARKDOWN_RE = /!?\[[^\]]*\]\(sandbox:\/\/\/[^)]+\)\n?/g

const cleanedResult = computed(() => {
  const text = truncatedResult.value
  if (mediaRefs.value.length === 0) return text
  return text.replace(SANDBOX_MARKDOWN_RE, '').trim()
})

const bashMeta = computed(() => {
  if (props.toolName !== 'bash') return ''
  const bits: string[] = []

  const exitCodeRaw = normalizedResult.value.data?.exit_code
  const durationRaw = normalizedResult.value.meta?.duration_ms
  const timedOutRaw = normalizedResult.value.meta?.timed_out
  const truncatedRaw = normalizedResult.value.meta?.truncated

  if (typeof exitCodeRaw === 'number') {
    bits.push(`exit_code=${exitCodeRaw}`)
  }
  if (typeof durationRaw === 'number') {
    bits.push(`duration=${durationRaw}ms`)
  }
  if (Boolean(timedOutRaw)) {
    bits.push('timed_out=true')
  }
  if (Boolean(truncatedRaw)) {
    bits.push('truncated=true')
  }
  return bits.join('  ')
})

const taskTrace = computed<Record<string, unknown>[]>(() => {
  if (!SUBAGENT_TOOL_NAMES.has(props.toolName)) return []
  const trace = normalizedResult.value.data?.trace
  if (!Array.isArray(trace)) return []
  return trace.filter((x): x is Record<string, unknown> => typeof x === 'object' && x !== null)
})

type QuestionAnswerView = {
  id: string
  question: string
  selected_options: string[]
  free_text: string
  notes: string
}

const questionAnswers = computed<QuestionAnswerView[]>(() => {
  if (props.toolName !== 'question') return []
  const answers = normalizedResult.value.data?.answers
  if (!Array.isArray(answers)) return []
  return answers
    .filter((item): item is Record<string, unknown> => typeof item === 'object' && item !== null)
    .map((item) => ({
      id: typeof item.id === 'string' ? item.id : '',
      question: typeof item.question === 'string' ? item.question : '',
      selected_options: Array.isArray(item.selected_options)
        ? item.selected_options.map(x => String(x))
        : [],
      free_text: typeof item.free_text === 'string' ? item.free_text : '',
      notes: typeof item.notes === 'string' ? item.notes : '',
    }))
})

function handleTaskTraceToggle(event: Event): void {
  const details = event.target as HTMLDetailsElement | null
  taskTraceExpanded.value = Boolean(details?.open)
}

function toggleExpanded(): void {
  expanded.value = !expanded.value
  if (!expanded.value) {
    taskTraceExpanded.value = false
  }
}

function traceBlockTitle(block: Record<string, unknown>, idx: number): string {
  const type = String(block.type || 'block')
  if (type === 'tool_call') {
    const name = String(block.name || 'tool')
    return `#${idx + 1} tool_call: ${name}`
  }
  return `#${idx + 1} ${type}`
}

function traceBlockContent(block: Record<string, unknown>): string {
  const type = String(block.type || '')
  if (type === 'text' || type === 'thinking') {
    return String(block.content || '')
  }
  if (type === 'tool_call') {
    return JSON.stringify({
      input: block.input,
      result: block.result,
      isError: block.isError,
    }, null, 2)
  }
  return JSON.stringify(block, null, 2)
}
</script>

<style scoped>
.tool-call-display {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-md);
  margin: 8px 0;
  overflow: hidden;
}

.tool-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-user-message);
  cursor: pointer;
  user-select: none;
  transition: background var(--transition-fast);
}

.tool-header:hover {
  background: var(--border-light);
}

.tool-icon {
  font-size: 16px;
  color: var(--accent-primary);
}

.tool-name {
  font-family: monospace;
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
}

.tool-status {
  margin-left: auto;
}

.expand-icon {
  font-size: 12px;
  color: var(--text-secondary);
}

.tool-body {
  padding: 12px;
}

.tool-section {
  margin-bottom: 8px;
}

.tool-section:last-child {
  margin-bottom: 0;
}

.section-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  color: var(--text-secondary);
  margin-bottom: 4px;
}

.tool-content {
  background: var(--bg-user-message);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  font-family: monospace;
  font-size: 12px;
  line-height: 1.5;
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
  max-height: 300px;
  overflow-y: auto;
  color: var(--text-primary);
}

.tool-error {
  color: #DC2626;
}

.bash-meta {
  font-family: monospace;
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.tool-media {
  margin: 8px 0;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.tool-media-img {
  max-width: 100%;
  max-height: 400px;
  border-radius: var(--radius-md);
  object-fit: contain;
}
.tool-media-video {
  max-width: 100%;
  border-radius: var(--radius-md);
}
.tool-media-audio {
  width: 100%;
}

.question-answers {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 8px 0;
}

.question-answer-item {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 8px;
  background: var(--bg-user-message);
}

.question-answer-title {
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 6px;
}

.question-answer-line {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 3px;
}

.task-trace {
  margin: 8px 0;
}
.task-trace-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 6px;
}
.task-trace-item {
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  padding: 8px;
}
.task-trace-head {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 4px;
  font-family: monospace;
}
</style>
