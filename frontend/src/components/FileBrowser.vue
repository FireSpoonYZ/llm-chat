<template>
  <div class="file-browser">
    <div class="fb-toolbar">
      <span class="fb-title">workspace</span>
      <div class="fb-toolbar-actions">
        <el-checkbox
          v-if="treeData.length > 0"
          :model-value="allSelected"
          :indeterminate="someSelected && !allSelected"
          @change="toggleSelectAll"
          data-testid="select-all"
        >All</el-checkbox>
        <el-button text :icon="Refresh" @click="loadFiles" title="Refresh" />
      </div>
    </div>

    <div v-if="loading" v-loading="true" class="fb-loading" element-loading-background="transparent" />
    <div v-else-if="error" class="fb-error">{{ error }}</div>
    <div v-else-if="treeData.length === 0" class="fb-empty">No files</div>
    <template v-else>
      <el-tree
        ref="treeRef"
        class="fb-tree"
        :data="treeData"
        :props="treeProps"
        node-key="path"
        show-checkbox
        default-expand-all
        :expand-on-click-node="false"
        @check="onCheck"
      >
        <template #default="{ node, data }">
          <div class="fb-node">
            <el-icon class="fb-icon">
              <Document v-if="!data.is_dir" />
              <FolderOpened v-else-if="node.expanded" />
              <Folder v-else />
            </el-icon>
            <span class="fb-name">{{ data.name }}</span>
            <span v-if="!data.is_dir" class="fb-size">
              {{ formatSize(data.size) }}
            </span>
            <el-button
              text
              size="small"
              :icon="Download"
              class="fb-download"
              @click.stop="handleDownload(data.path)"
            />
          </div>
        </template>
      </el-tree>

      <div v-if="checkedCount > 0" class="fb-selection-bar">
        <span class="fb-selection-count">{{ checkedCount }} selected</span>
        <el-button size="small" :disabled="batchDownloading" @click="handleBatchDownload">
          {{ batchDownloading ? 'Downloading...' : 'Download' }}
        </el-button>
        <el-button size="small" @click="clearSelection">Clear</el-button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Refresh, Download, Document, Folder, FolderOpened } from '@element-plus/icons-vue'
import { listFiles, downloadFile, downloadBatch } from '../api/conversations'
import type { FileEntry } from '../types'
import type { TreeInstance } from 'element-plus'

interface TreeNode extends FileEntry {
  path: string
  children?: TreeNode[]
}

const props = defineProps<{ conversationId: string }>()

const treeRef = ref<TreeInstance>()
const treeData = ref<TreeNode[]>([])
const loading = ref(false)
const error = ref('')
const batchDownloading = ref(false)
const checkedCount = ref(0)

const treeProps = { label: 'name', children: 'children' }

function addPaths(entries: FileEntry[], parentPath: string): TreeNode[] {
  return entries.map(e => ({
    ...e,
    path: parentPath + '/' + e.name,
    children: e.children ? addPaths(e.children, parentPath + '/' + e.name) : undefined,
  }))
}

function collectAllPaths(nodes: TreeNode[]): string[] {
  const paths: string[] = []
  for (const n of nodes) {
    paths.push(n.path)
    if (n.children) paths.push(...collectAllPaths(n.children))
  }
  return paths
}

const allPaths = computed(() => collectAllPaths(treeData.value))
const allSelected = computed(() => allPaths.value.length > 0 && checkedCount.value === allPaths.value.length)
const someSelected = computed(() => checkedCount.value > 0)

function onCheck() {
  const keys = treeRef.value?.getCheckedKeys(false) ?? []
  const halfKeys = treeRef.value?.getHalfCheckedKeys() ?? []
  checkedCount.value = keys.length + halfKeys.length
}

function toggleSelectAll(val: boolean | string | number) {
  if (val) {
    treeRef.value?.setCheckedKeys(allPaths.value, false)
  } else {
    treeRef.value?.setCheckedKeys([], false)
  }
  checkedCount.value = val ? allPaths.value.length : 0
}

function clearSelection() {
  treeRef.value?.setCheckedKeys([], false)
  checkedCount.value = 0
}

async function loadFiles() {
  loading.value = true
  error.value = ''
  try {
    const res = await listFiles(props.conversationId, '/', true)
    treeData.value = addPaths(res.entries, '')
  } catch {
    error.value = 'Failed to load files'
    treeData.value = []
  } finally {
    loading.value = false
  }
}

function getSmartCheckedPaths(): string[] {
  const checked = new Set(treeRef.value?.getCheckedKeys(false) as string[] ?? [])
  // If a parent is fully checked, skip its children
  return [...checked].filter(p => {
    const parts = p.split('/')
    for (let i = 2; i < parts.length; i++) {
      if (checked.has(parts.slice(0, i).join('/'))) return false
    }
    return true
  })
}

async function handleDownload(filePath: string) {
  await downloadFile(props.conversationId, filePath)
}

async function handleBatchDownload() {
  const paths = getSmartCheckedPaths()
  if (paths.length === 0) return
  batchDownloading.value = true
  try {
    if (paths.length === 1) {
      await downloadFile(props.conversationId, paths[0])
    } else {
      await downloadBatch(props.conversationId, paths)
    }
  } finally {
    batchDownloading.value = false
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}

watch(() => props.conversationId, () => {
  loadFiles()
}, { immediate: true })

defineExpose({ refresh: loadFiles })
</script>

<style scoped>
.file-browser {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.fb-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid var(--border-light);
  margin-bottom: 4px;
}
.fb-toolbar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.fb-title {
  font-size: 13px;
  color: var(--text-secondary);
  font-weight: 500;
}
.fb-loading {
  min-height: 80px;
}
.fb-error, .fb-empty {
  padding: 24px 0;
  text-align: center;
  color: var(--text-secondary);
  font-size: 13px;
}
.fb-error { color: #EF4444; }
.fb-tree {
  flex: 1;
  overflow-y: auto;
  --el-tree-node-content-height: 32px;
}
.fb-tree :deep(.el-tree-node__content) {
  padding-right: 8px;
}
.fb-node {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 13px;
}
.fb-icon {
  flex-shrink: 0;
  color: var(--el-color-warning);
}
.fb-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fb-size {
  color: var(--text-secondary);
  font-size: 12px;
  flex-shrink: 0;
}
.fb-download {
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s;
}
.fb-node:hover .fb-download {
  opacity: 1;
}
.fb-selection-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-top: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-light);
  font-size: 13px;
}
.fb-selection-count {
  color: var(--text-secondary);
  margin-right: auto;
}
</style>
