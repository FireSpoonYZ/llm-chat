<template>
  <div class="conversation-list">
    <div
      v-for="conv in conversations"
      :key="conv.id"
      class="conversation-item"
      :class="{ active: conv.id === currentId }"
      @click="$emit('select', conv.id)"
    >
      <span class="title">{{ conv.title }}</span>
      <el-button
        class="delete-btn"
        text
        size="small"
        @click.stop="$emit('delete', conv.id)"
      >
        &times;
      </el-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { Conversation } from '../types'

defineProps<{
  conversations: Conversation[]
  currentId: string | null
}>()

defineEmits<{
  select: [id: string]
  delete: [id: string]
}>()
</script>

<style scoped>
.conversation-list {
  overflow-y: auto;
}
.conversation-item {
  padding: 10px 12px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid #f0f0f0;
}
.conversation-item:hover {
  background: #ecf5ff;
}
.conversation-item.active {
  background: #e6f0ff;
}
.title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.delete-btn {
  opacity: 0;
  transition: opacity 0.2s;
}
.conversation-item:hover .delete-btn {
  opacity: 1;
}
</style>
