<template>
  <div class="file-browser">
    <div class="fb-toolbar">
      <span class="fb-title">{{ t('fileBrowser.workspace') }}</span>
      <div class="fb-toolbar-actions">
        <el-checkbox
          v-if="treeData.length > 0"
          :model-value="allSelected"
          :indeterminate="someSelected && !allSelected"
          @change="toggleSelectAll"
          data-testid="select-all"
        >{{ t('fileBrowser.all') }}</el-checkbox>
        <el-button text :icon="Upload" @click="triggerUpload" :title="t('common.upload')" data-testid="upload-btn" />
        <el-button text :icon="Refresh" @click="loadFiles" :title="t('common.refresh')" />
        <input
          ref="fileInputRef"
          type="file"
          multiple
          style="display: none"
          data-testid="file-input"
          @change="handleFileSelect"
        />
      </div>
    </div>

    <el-progress
      v-if="uploading"
      :percentage="uploadProgress"
      :stroke-width="3"
      class="fb-upload-progress"
      data-testid="upload-progress"
    />

    <div v-if="loading" v-loading="true" class="fb-loading" element-loading-background="transparent" />
    <div v-else-if="error" class="fb-error">{{ error }}</div>
    <div v-else-if="treeData.length === 0" class="fb-empty">{{ t('fileBrowser.noFiles') }}</div>
    <template v-else>
      <el-tree
        ref="treeRef"
        class="fb-tree"
        :data="treeData"
        :props="treeProps"
        node-key="path"
        show-checkbox
        lazy
        :load="loadNode"
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

      <div v-if="hasSelection" class="fb-selection-bar">
        <span class="fb-selection-count">{{ selectionLabel }}</span>
        <el-button size="small" :disabled="batchDownloading" @click="handleBatchDownload">
          {{ batchDownloading ? t('fileBrowser.downloading') : t('common.download') }}
        </el-button>
        <el-button size="small" @click="clearSelection">{{ t('common.clear') }}</el-button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Refresh, Download, Upload, Document, Folder, FolderOpened } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { listFiles, downloadFile, downloadBatch, uploadFiles } from '../api/conversations'
import type { FileEntry } from '../types'
import type { TreeInstance } from 'element-plus'
import { t } from '../i18n'

interface TreeNode extends FileEntry {
  path: string
  leaf: boolean
  loaded?: boolean
  children?: TreeNode[]
}

interface TreeLoadNode {
  level: number
  data?: TreeNode
}

const MAX_BATCH_DOWNLOAD_PATHS = 100

const props = defineProps<{ conversationId: string }>()

const treeRef = ref<TreeInstance>()
const treeData = ref<TreeNode[]>([])
const loading = ref(false)
const error = ref('')
const batchDownloading = ref(false)
const checkedCount = ref(0)
const fileInputRef = ref<HTMLInputElement>()
const uploading = ref(false)
const uploadProgress = ref(0)

const treeProps = { label: 'name', children: 'children', isLeaf: 'leaf' }
const workspaceAllSelected = ref(false)
const suppressCheckUpdate = ref(false)
const childrenCache = new Map<string, TreeNode[]>()
const childrenInFlight = new Map<string, Promise<TreeNode[]>>()

function joinPath(parentPath: string, name: string): string {
  if (!parentPath || parentPath === '/') return `/${name}`
  return `${parentPath}/${name}`
}

function addPaths(entries: FileEntry[], parentPath: string): TreeNode[] {
  return entries.map((entry) => {
    const path = joinPath(parentPath, entry.name)
    const children = entry.children ? addPaths(entry.children, path) : undefined
    return {
      ...entry,
      path,
      leaf: !entry.is_dir,
      loaded: !!children,
      children,
    }
  })
}

function collectLoadedPaths(nodes: TreeNode[]): string[] {
  const paths: string[] = []
  for (const node of nodes) {
    paths.push(node.path)
    if (node.children && node.children.length > 0) {
      paths.push(...collectLoadedPaths(node.children))
    }
  }
  return paths
}

const loadedPaths = computed(() => collectLoadedPaths(treeData.value))
const allSelected = computed(() =>
  workspaceAllSelected.value || (loadedPaths.value.length > 0 && checkedCount.value === loadedPaths.value.length),
)
const someSelected = computed(() => workspaceAllSelected.value || checkedCount.value > 0)
const hasSelection = computed(() => workspaceAllSelected.value || checkedCount.value > 0)
const selectionLabel = computed(() =>
  workspaceAllSelected.value
    ? t('fileBrowser.workspaceSelected')
    : t('fileBrowser.selectedCount', { count: checkedCount.value }),
)

function setCheckedCountFromTree() {
  const keys = treeRef.value?.getCheckedKeys(false) ?? []
  const halfKeys = treeRef.value?.getHalfCheckedKeys() ?? []
  checkedCount.value = keys.length + halfKeys.length
}

function onCheck() {
  if (suppressCheckUpdate.value) return
  setCheckedCountFromTree()
  // User changed selection manually while in global mode; fall back to normal explicit selection.
  if (workspaceAllSelected.value) {
    workspaceAllSelected.value = false
  }
}

function setCheckedKeysSafely(paths: string[]) {
  suppressCheckUpdate.value = true
  treeRef.value?.setCheckedKeys(paths, false)
  suppressCheckUpdate.value = false
}

function toggleSelectAll(val: boolean | string | number) {
  if (val) {
    workspaceAllSelected.value = true
    setCheckedKeysSafely(loadedPaths.value)
    checkedCount.value = loadedPaths.value.length
    return
  }
  workspaceAllSelected.value = false
  setCheckedKeysSafely([])
  checkedCount.value = 0
}

function clearSelection() {
  workspaceAllSelected.value = false
  setCheckedKeysSafely([])
  checkedCount.value = 0
}

async function fetchChildren(path: string): Promise<TreeNode[]> {
  const normalized = path || '/'
  const cached = childrenCache.get(normalized)
  if (cached) return cached

  const inFlight = childrenInFlight.get(normalized)
  if (inFlight) return inFlight

  const request = (async () => {
    const res = await listFiles(props.conversationId, normalized, false)
    const parentPath = normalized === '/' ? '' : normalized
    const children = addPaths(res.entries, parentPath)
    childrenCache.set(normalized, children)
    return children
  })()

  childrenInFlight.set(normalized, request)
  try {
    return await request
  } finally {
    childrenInFlight.delete(normalized)
  }
}

async function loadNode(node: TreeLoadNode, resolve: (children: TreeNode[]) => void) {
  if (node.level === 0) {
    resolve(treeData.value)
    return
  }

  const data = node.data
  if (!data || !data.is_dir) {
    resolve([])
    return
  }

  try {
    const children = await fetchChildren(data.path)
    data.children = children
    data.loaded = true
    resolve(children)
  } catch {
    ElMessage.error(t('fileBrowser.loadChildrenFailed'))
    resolve([])
  }
}

function resetTreeState() {
  clearSelection()
  childrenCache.clear()
  childrenInFlight.clear()
}

async function loadFiles() {
  loading.value = true
  error.value = ''
  resetTreeState()
  try {
    const res = await listFiles(props.conversationId, '/', false)
    treeData.value = addPaths(res.entries, '')
  } catch {
    error.value = t('fileBrowser.failedLoadFiles')
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
  if (workspaceAllSelected.value) {
    batchDownloading.value = true
    try {
      await downloadFile(props.conversationId, '/')
    } finally {
      batchDownloading.value = false
    }
    return
  }

  const paths = getSmartCheckedPaths()
  if (paths.length === 0) return
  if (paths.length > MAX_BATCH_DOWNLOAD_PATHS) {
    ElMessage.error(t('fileBrowser.tooManySelections'))
    return
  }

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

function triggerUpload() {
  fileInputRef.value?.click()
}

async function handleFileSelect(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files || [])
  if (files.length === 0) return

  uploading.value = true
  uploadProgress.value = 0
  try {
    await uploadFiles(props.conversationId, files, '', (pct) => {
      uploadProgress.value = pct
    })
    ElMessage.success(t('fileBrowser.uploadedFiles', { count: files.length }))
    await loadFiles()
  } catch {
    ElMessage.error(t('fileBrowser.uploadFailed'))
  } finally {
    uploading.value = false
    uploadProgress.value = 0
    input.value = ''
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
.fb-upload-progress {
  padding: 4px 0;
}
</style>
