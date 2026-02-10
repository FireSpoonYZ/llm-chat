<template>
  <div class="chat-input">
    <el-input
      v-model="content"
      type="textarea"
      :rows="3"
      placeholder="Type a message..."
      :disabled="disabled"
      @keydown.enter.exact.prevent="handleSend"
    />
    <el-button
      type="primary"
      :disabled="disabled || !content.trim()"
      @click="handleSend"
      style="margin-top: 8px"
    >
      Send
    </el-button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

defineProps<{ disabled: boolean }>()
const emit = defineEmits<{ send: [content: string] }>()

const content = ref('')

function handleSend() {
  if (!content.value.trim()) return
  emit('send', content.value)
  content.value = ''
}
</script>

<style scoped>
.chat-input {
  padding: 16px;
  border-top: 1px solid #e4e7ed;
}
</style>
