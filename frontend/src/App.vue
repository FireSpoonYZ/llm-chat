<template>
  <el-config-provider :locale="elementLocale">
    <div class="app-shell">
      <div class="locale-switcher">
        <span class="locale-label">{{ t('common.language') }}</span>
        <el-select
          :model-value="currentLocale"
          size="small"
          :aria-label="t('common.language')"
          class="locale-select"
          @update:model-value="onLocaleChange"
        >
          <el-option value="en" :label="t('common.english')" />
          <el-option value="zh-CN" :label="t('common.chinese')" />
        </el-select>
      </div>
      <router-view />
    </div>
  </el-config-provider>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import enLocale from 'element-plus/es/locale/lang/en'
import zhCnLocale from 'element-plus/es/locale/lang/zh-cn'
import { currentLocale, setLocale, t, type Locale } from './i18n'

const elementLocale = computed(() => (
  currentLocale.value === 'zh-CN' ? zhCnLocale : enLocale
))

function onLocaleChange(locale: string) {
  if (locale === 'en' || locale === 'zh-CN') {
    setLocale(locale as Locale)
  }
}
</script>

<style scoped>
.app-shell {
  min-height: 100vh;
}

.locale-switcher {
  position: fixed;
  top: max(12px, env(safe-area-inset-top));
  right: max(12px, env(safe-area-inset-right));
  z-index: 40;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px 6px 10px;
  border-radius: 999px;
  border: 1px solid var(--border-light);
  background: rgba(250, 249, 246, 0.92);
  box-shadow: var(--shadow-sm);
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast), background-color var(--transition-fast);
}

.locale-switcher:focus-within {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 3px rgba(217, 119, 6, 0.16);
}

.locale-label {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.locale-select {
  width: 112px;
}

.locale-select :deep(.el-select__wrapper) {
  min-height: 30px;
  border-radius: 999px;
  border: 1px solid var(--border-light);
  box-shadow: none;
  background: var(--bg-input);
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}

.locale-select :deep(.el-select__wrapper:hover) {
  border-color: var(--border-input);
}

.locale-select :deep(.is-focused .el-select__wrapper),
.locale-select :deep(.el-select__wrapper.is-focused) {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.12);
}

@media (max-width: 768px) {
  .locale-switcher {
    top: max(8px, env(safe-area-inset-top));
    right: max(8px, env(safe-area-inset-right));
    padding: 5px 6px;
    gap: 6px;
  }

  .locale-label {
    display: none;
  }

  .locale-select {
    width: 96px;
  }
}
</style>
