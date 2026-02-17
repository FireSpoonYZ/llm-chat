<template>
  <div class="shared-chat">
    <div v-if="loading" v-loading="true" class="loading-state" element-loading-background="transparent" />
    <div v-else-if="error" class="error-state">
      <h2>{{ t('shared.notFoundTitle') }}</h2>
      <p>{{ t('shared.notFoundDescription') }}</p>
    </div>
    <template v-else>
      <div class="shared-header">
        <div class="shared-header-inner">
          <h1 class="shared-title">{{ conversation?.title }}</h1>
          <el-tag type="info" size="small">{{ t('shared.readOnly') }}</el-tag>
        </div>
        <div class="shared-meta">
          <span v-if="conversation?.model_name">{{ conversation.model_name }}</span>
          <span>{{ formattedDate }}</span>
        </div>
      </div>
      <div class="shared-messages">
        <div class="messages-inner">
          <ChatMessage
            v-for="msg in messages"
            :key="msg.id"
            :message="msg"
            :read-only="true"
            :share-token="shareToken"
          />
          <div v-if="hasMore" class="load-more">
            <el-button @click="loadMore" :loading="loadingMore">{{ t('shared.loadMore') }}</el-button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import ChatMessage from '../components/ChatMessage.vue'
import { getSharedConversation, getSharedMessages } from '../api/sharing'
import type { SharedConversation, Message } from '../types'
import { currentLocale, t } from '../i18n'

const props = defineProps<{ shareToken: string }>()

const conversation = ref<SharedConversation | null>(null)
const messages = ref<Message[]>([])
const total = ref(0)
const loading = ref(true)
const loadingMore = ref(false)
const error = ref(false)

const PAGE_SIZE = 50

const hasMore = computed(() => messages.value.length < total.value)

const formattedDate = computed(() => {
  if (!conversation.value?.created_at) return ''
  return new Date(conversation.value.created_at).toLocaleDateString(currentLocale.value)
})

onMounted(async () => {
  try {
    const [conv, msgResp] = await Promise.all([
      getSharedConversation(props.shareToken),
      getSharedMessages(props.shareToken, PAGE_SIZE, 0),
    ])
    conversation.value = conv
    messages.value = msgResp.messages
    total.value = msgResp.total
  } catch {
    error.value = true
  } finally {
    loading.value = false
  }
})

async function loadMore() {
  loadingMore.value = true
  try {
    const resp = await getSharedMessages(props.shareToken, PAGE_SIZE, messages.value.length)
    messages.value.push(...resp.messages)
    total.value = resp.total
  } catch {
    ElMessage.error(t('shared.failedLoadMore'))
  } finally {
    loadingMore.value = false
  }
}
</script>

<style scoped>
.shared-chat {
  min-height: 100vh;
  background: var(--bg-main);
}

.loading-state {
  height: 200px;
}

.error-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 60vh;
  color: var(--text-secondary);
}
.error-state h2 {
  margin-bottom: 8px;
  color: var(--text-primary);
}

.shared-header {
  padding: 24px 16px 16px;
  border-bottom: 1px solid var(--border-light);
}
.shared-header-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
  display: flex;
  align-items: center;
  gap: 12px;
}
.shared-title {
  font-size: 18px;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary);
}
.shared-meta {
  max-width: var(--max-width-chat);
  margin: 6px auto 0;
  font-size: 13px;
  color: var(--text-secondary);
  display: flex;
  gap: 12px;
}

.shared-messages {
  padding: 24px 16px;
}
.messages-inner {
  max-width: var(--max-width-chat);
  margin: 0 auto;
}

.load-more {
  text-align: center;
  padding: 16px 0;
}
</style>
