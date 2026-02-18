<template>
  <el-config-provider :locale="elementLocale">
    <div class="app-shell">
      <router-view />
    </div>
  </el-config-provider>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import enLocale from 'element-plus/es/locale/lang/en'
import zhCnLocale from 'element-plus/es/locale/lang/zh-cn'
import { currentLocale } from './i18n'

const elementLocale = computed(() => (
  currentLocale.value === 'zh-CN' ? zhCnLocale : enLocale
))
</script>

<style scoped>
.app-shell {
  min-height: 100vh;
}
</style>
