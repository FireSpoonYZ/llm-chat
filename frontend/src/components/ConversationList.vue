<template>
  <div class="conversation-list">
    <div
      v-for="conv in conversations"
      :key="conv.id"
      class="conversation-item"
      :class="{ active: conv.id === currentId }"
    >
      <button
        type="button"
        class="conversation-select"
        :aria-current="conv.id === currentId ? 'true' : undefined"
        @click="$emit('select', conv.id)"
      >
        <span class="title">{{ conv.title }}</span>
      </button>
      <el-button
        class="delete-btn"
        text
        size="small"
        :icon="Close"
        @click.stop="$emit('delete', conv.id)"
        :aria-label="t('conversation.deleteConversation')"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Close } from '@element-plus/icons-vue'
import type { Conversation } from '../types'
import { t } from '../i18n'

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
  padding: 2px 6px 2px 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-radius: var(--radius-md);
  margin-bottom: 2px;
  transition: background var(--transition-fast);
}
.conversation-item.active {
  background: var(--bg-sidebar-active);
}
.conversation-item:hover {
  background: var(--bg-sidebar-hover);
}
.conversation-select {
  border: none;
  background: transparent;
  text-align: left;
  color: inherit;
  width: 100%;
  cursor: pointer;
  min-height: 36px;
  padding: 8px 0;
}
.conversation-select:focus-visible {
  outline: 2px solid rgba(217, 119, 6, 0.6);
  outline-offset: 2px;
  border-radius: 6px;
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
