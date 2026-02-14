<template>
  <div class="tool-call-display">
    <div class="tool-header" @click="expanded = !expanded">
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
          <div class="section-label">Input</div>
          <pre class="tool-content">{{ formattedInput }}</pre>
        </div>
        <div v-if="toolResult" class="tool-section">
          <div class="section-label">
            {{ isError ? 'Error' : 'Result' }}
          </div>
          <div v-if="mediaRefs.length > 0" class="tool-media">
            <template v-for="(media, i) in mediaRefs" :key="i">
              <img v-if="media.type === 'image'" :src="media.url" :alt="media.name" class="tool-media-img" loading="lazy" />
              <video v-else-if="media.type === 'video'" controls preload="metadata" :src="media.url" class="tool-media-video" />
              <audio v-else-if="media.type === 'audio'" controls preload="metadata" :src="media.url" class="tool-media-audio" />
            </template>
          </div>
          <pre class="tool-content" :class="{ 'tool-error': isError }">{{
            truncatedResult
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
} from '@element-plus/icons-vue'
import { fileViewUrl } from '../utils/fileUrl'

const props = defineProps<{
  toolName: string
  toolCallId: string
  toolInput?: Record<string, unknown>
  toolResult?: string
  isError?: boolean
  isLoading?: boolean
  conversationId?: string
}>()

const expanded = ref(false)

const MAX_RESULT_LENGTH = 5000

const toolIcon = computed(() => {
  switch (props.toolName) {
    case 'bash':
      return Monitor
    case 'read':
    case 'write':
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
    default:
      return Monitor
  }
})

const IMAGE_EXTS = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.svg']
const VIDEO_EXTS = ['.mp4', '.webm', '.mov']
const AUDIO_EXTS = ['.mp3', '.wav', '.ogg', '.m4a']

interface MediaRef {
  type: 'image' | 'video' | 'audio'
  url: string
  name: string
}

const SANDBOX_RE = /sandbox:\/\/\/([^\s)]+)/g

const mediaRefs = computed<MediaRef[]>(() => {
  if (!props.toolResult || !props.conversationId) return []
  const refs: MediaRef[] = []
  let match: RegExpExecArray | null
  const re = new RegExp(SANDBOX_RE.source, 'g')
  while ((match = re.exec(props.toolResult)) !== null) {
    const path = match[1]
    const ext = path.substring(path.lastIndexOf('.')).toLowerCase()
    const name = path.substring(path.lastIndexOf('/') + 1)
    const url = fileViewUrl(props.conversationId, '/' + path)
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
  if (props.isLoading) return 'Running...'
  if (props.isError) return 'Error'
  return 'Done'
})

const formattedInput = computed(() => {
  if (!props.toolInput) return ''
  return JSON.stringify(props.toolInput, null, 2)
})

const truncatedResult = computed(() => {
  if (!props.toolResult) return ''
  if (props.toolResult.length > MAX_RESULT_LENGTH) {
    return props.toolResult.slice(0, MAX_RESULT_LENGTH) + '\n... [truncated]'
  }
  return props.toolResult
})
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
</style>
