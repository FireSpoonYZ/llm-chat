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
        :icon="Close"
        @click.stop="$emit('delete', conv.id)"
        aria-label="Delete conversation"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Close } from '@element-plus/icons-vue'
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
  flex: 1;
  overflow-y: auto;
  padding: 4px 8px;
}
.conversation-item {
  padding: 10px 12px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-radius: var(--radius-md);
  margin-bottom: 2px;
  transition: background var(--transition-fast);
}
.conversation-item:hover {
  background: var(--bg-sidebar-hover);
}
.conversation-item.active {
  background: var(--bg-sidebar-active);
}
.title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  color: var(--text-sidebar);
  font-size: 14px;
}
.delete-btn {
  color: var(--text-sidebar-muted) !important;
  opacity: 0;
  transition: opacity 0.2s, color var(--transition-fast);
}
.conversation-item:hover .delete-btn {
  opacity: 1;
}
.delete-btn:hover {
  color: #F87171 !important;
}
</style>