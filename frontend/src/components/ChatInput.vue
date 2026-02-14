<template>
  <div class="chat-input-wrapper">
    <div class="chat-input-inner">
      <div class="input-container">
        <button
          class="attach-btn"
          @click="triggerFileInput"
          aria-label="Attach files"
          data-testid="attach-btn"
        >
          <el-icon :size="18"><Paperclip /></el-icon>
        </button>
        <input
          ref="fileInputRef"
          type="file"
          multiple
          style="display: none"
          data-testid="attach-file-input"
          @change="handleAttach"
        />
        <textarea
          ref="textareaRef"
          v-model="content"
          class="chat-textarea"
          placeholder="Type a message..."
          :disabled="disabled"
          rows="1"
          @keydown.enter.exact.prevent="handleSend"
          @input="autoGrow"
        />
        <button
          class="send-btn"
          :disabled="disabled || !content.trim()"
          @click="handleSend"
          aria-label="Send message"
        >
          <el-icon :size="18"><Promotion /></el-icon>
        </button>
      </div>
      <div class="input-options">
        <button
          class="chip-toggle"
          :class="{ active: deepThinking }"
          @click="$emit('update:deepThinking', !deepThinking)"
        >
          Deep Thinking
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { Promotion, Paperclip } from '@element-plus/icons-vue'

defineProps<{ disabled: boolean; deepThinking: boolean }>()
const emit = defineEmits<{
  send: [content: string]
  'update:deepThinking': [value: boolean]
  'attach-files': [files: File[]]
}>()

const content = ref('')
const textareaRef = ref<HTMLTextAreaElement>()
const fileInputRef = ref<HTMLInputElement>()

function handleSend() {
  if (!content.value.trim()) return
  emit('send', content.value)
  content.value = ''
  nextTick(() => {
    if (textareaRef.value) {
      textareaRef.value.style.height = 'auto'
    }
  })
}

function autoGrow() {
  const el = textareaRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 200) + 'px'
}

function triggerFileInput() {
  fileInputRef.value?.click()
}

function handleAttach(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files || [])
  if (files.length > 0) {
    emit('attach-files', files)
  }
  input.value = ''
}
</script>

<style scoped>
.chat-input-wrapper {
  padding: 12px 16px 20px;
  flex-shrink: 0;
}
.chat-input-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
}
.input-container {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-input);
  border-radius: var(--radius-lg);
  padding: 10px 12px 10px 18px;
  box-shadow: var(--shadow-sm);
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}
.input-container:focus-within {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.12);
}
.chat-textarea {
  flex: 1;
  border: none;
  outline: none;
  resize: none;
  font-family: inherit;
  font-size: 15px;
  line-height: 1.5;
  color: var(--text-primary);
  background: transparent;
  padding: 5px 0;
  max-height: 200px;
}
.attach-btn {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-full);
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: color var(--transition-fast), background var(--transition-fast);
}
.attach-btn:hover {
  color: var(--text-primary);
  background: var(--border-light);
}
.chat-textarea::placeholder {
  color: var(--text-muted);
}
.chat-textarea:disabled {
  opacity: 0.5;
}
.send-btn {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-full);
  border: none;
  background: var(--accent-primary);
  color: white;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: background var(--transition-fast), opacity var(--transition-fast);
}
.send-btn:hover:not(:disabled) {
  background: var(--accent-primary-hover);
}
.send-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.input-options {
  display: flex;
  align-items: center;
  padding-top: 8px;
}
.chip-toggle {
  background: none;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-full);
  padding: 3px 12px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
}
.chip-toggle:hover {
  background: var(--border-light);
  color: var(--text-primary);
}
.chip-toggle.active {
  background: rgba(217, 119, 6, 0.12);
  color: var(--accent-primary);
  border-color: var(--accent-primary);
}
</style>