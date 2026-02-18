<template>
  <div
    class="locale-toggle"
    :class="`is-${variant}`"
    role="group"
    :aria-label="t('common.language')"
  >
    <button
      type="button"
      class="locale-btn"
      data-testid="locale-en"
      :class="{ active: currentLocale === 'en' }"
      :aria-pressed="currentLocale === 'en'"
      :title="t('common.english')"
      @click="applyLocale('en')"
    >
      EN
    </button>
    <button
      type="button"
      class="locale-btn"
      data-testid="locale-zh"
      :class="{ active: currentLocale === 'zh-CN' }"
      :aria-pressed="currentLocale === 'zh-CN'"
      :title="t('common.chinese')"
      @click="applyLocale('zh-CN')"
    >
      中
    </button>
  </div>
</template>

<script setup lang="ts">
import { currentLocale, setLocale, t, type Locale } from '../i18n'

withDefaults(defineProps<{
  variant?: 'toolbar' | 'header'
}>(), {
  variant: 'header',
})

function applyLocale(locale: Locale) {
  if (currentLocale.value !== locale) {
    setLocale(locale)
  }
}
</script>

<style scoped>
.locale-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0;
  padding: 2px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-full);
  background: rgba(255, 255, 255, 0.92);
  box-shadow: var(--shadow-sm);
}

.locale-btn {
  border: 0;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
  line-height: 1;
  min-width: 42px;
  height: 30px;
  padding: 0 12px;
  border-radius: var(--radius-full);
  cursor: pointer;
  transition: color var(--transition-fast), background-color var(--transition-fast), box-shadow var(--transition-fast);
}

.locale-btn:hover {
  color: var(--text-primary);
  background: rgba(0, 0, 0, 0.03);
}

.locale-btn.active {
  color: var(--text-primary);
  background: var(--bg-input);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
}

.locale-btn:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px rgba(217, 119, 6, 0.24);
}

.is-toolbar {
  background: rgba(250, 249, 246, 0.96);
}

.is-toolbar .locale-btn {
  min-width: 38px;
  height: 28px;
  padding: 0 10px;
  font-size: 11px;
}

@media (max-width: 768px) {
  .locale-toggle {
    padding: 1px;
  }

  .locale-btn {
    min-width: 36px;
    height: 28px;
    padding: 0 9px;
    font-size: 11px;
  }
}
</style>
