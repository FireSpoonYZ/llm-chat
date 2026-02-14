<template>
  <div class="mcp-server-config">
    <h3>MCP Servers</h3>
    <p class="mcp-description">
      Enable MCP servers for this conversation. The AI agent will be able to use
      tools provided by enabled servers.
    </p>

    <el-empty v-if="!servers.length" description="No MCP servers available" />

    <div v-else class="server-list">
      <div
        v-for="server in servers"
        :key="server.id"
        class="server-item"
      >
        <div class="server-info">
          <el-switch
            :model-value="isEnabled(server.id)"
            @change="(val: boolean) => toggleServer(server.id, val)"
          />
          <div class="server-details">
            <span class="server-name">{{ server.name }}</span>
            <span v-if="server.description" class="server-desc">
              {{ server.description }}
            </span>
            <el-tag size="small" type="info">{{ server.transport }}</el-tag>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { McpServer } from '../types'

const props = defineProps<{
  servers: McpServer[]
  enabledServerIds: string[]
}>()

const emit = defineEmits<{
  (e: 'update:enabledServerIds', ids: string[]): void
}>()

function isEnabled(serverId: string): boolean {
  return props.enabledServerIds.includes(serverId)
}

function toggleServer(serverId: string, enabled: boolean) {
  const ids = [...props.enabledServerIds]
  if (enabled) {
    if (!ids.includes(serverId)) {
      ids.push(serverId)
    }
  } else {
    const idx = ids.indexOf(serverId)
    if (idx >= 0) {
      ids.splice(idx, 1)
    }
  }
  emit('update:enabledServerIds', ids)
}
</script>

<style scoped>
.mcp-server-config {
  padding: 16px;
}

.mcp-description {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  margin-bottom: 16px;
}

.server-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.server-item {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 12px 16px;
}

.server-info {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.server-details {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.server-name {
  font-weight: 600;
  font-size: 14px;
}

.server-desc {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
</style>
