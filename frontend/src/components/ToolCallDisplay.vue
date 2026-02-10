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
} from '@element-plus/icons-vue'

const props = defineProps<{
  toolName: string
  toolCallId: string
  toolInput?: Record<string, unknown>
  toolResult?: string
  isError?: boolean
  isLoading?: boolean
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
    default:
      return Monitor
  }
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
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  margin: 8px 0;
  overflow: hidden;
}

.tool-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--el-fill-color-lighter);
  cursor: pointer;
  user-select: none;
}

.tool-header:hover {
  background: var(--el-fill-color-light);
}

.tool-icon {
  font-size: 16px;
  color: var(--el-color-primary);
}

.tool-name {
  font-family: monospace;
  font-weight: 600;
  font-size: 13px;
}

.tool-status {
  margin-left: auto;
}

.expand-icon {
  font-size: 12px;
  color: var(--el-text-color-secondary);
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
  color: var(--el-text-color-secondary);
  margin-bottom: 4px;
}

.tool-content {
  background: var(--el-fill-color);
  border-radius: 4px;
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
}

.tool-error {
  color: var(--el-color-danger);
}
</style>
