<template>
  <div class="auth-container">
    <el-card class="auth-card">
      <template #header>
        <div class="auth-card-header">
          <h2>{{ t('auth.title') }}</h2>
          <LocaleToggle variant="header" />
        </div>
      </template>
      <el-form ref="formRef" :model="form" :rules="rules" @submit.prevent="handleRegister" label-position="top">
        <el-form-item :label="t('auth.username')" prop="username">
          <el-input v-model="form.username" :placeholder="t('auth.username')" />
        </el-form-item>
        <el-form-item :label="t('auth.email')" prop="email">
          <el-input v-model="form.email" type="email" :placeholder="t('auth.email')" />
        </el-form-item>
        <el-form-item :label="t('auth.password')" prop="password">
          <el-input v-model="form.password" type="password" :placeholder="t('auth.password')" show-password />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" native-type="submit" :loading="loading" style="width: 100%">
            {{ t('auth.register') }}
          </el-button>
        </el-form-item>
        <p style="text-align: center">
          {{ t('auth.hasAccount') }} <router-link to="/login">{{ t('auth.login') }}</router-link>
        </p>
      </el-form>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import type { FormInstance, FormRules } from 'element-plus'
import { useAuthStore } from '../stores/auth'
import LocaleToggle from '../components/LocaleToggle.vue'
import { t } from '../i18n'

const auth = useAuthStore()
const router = useRouter()
const formRef = ref<FormInstance>()
const form = reactive({ username: '', email: '', password: '' })
const loading = ref(false)

const rules = computed<FormRules>(() => ({
  username: [{ required: true, message: t('auth.validation.usernameRequired'), trigger: 'blur' }],
  email: [
    { required: true, message: t('auth.validation.emailRequired'), trigger: 'blur' },
    { type: 'email', message: t('auth.validation.emailInvalid'), trigger: 'blur' },
  ],
  password: [
    { required: true, message: t('auth.validation.passwordRequired'), trigger: 'blur' },
    { min: 8, message: t('auth.validation.passwordMin'), trigger: 'blur' },
  ],
}))

async function handleRegister() {
  if (!formRef.value) return
  const valid = await formRef.value.validate().catch(() => false)
  if (!valid) return
  loading.value = true
  try {
    await auth.register(form.username, form.email, form.password)
    router.push('/')
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || t('auth.messages.registrationFailed'))
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.auth-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  background: var(--bg-main);
}
.auth-card {
  width: 400px;
  border-color: var(--border-light);
}

.auth-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.auth-card h2 {
  margin: 0;
  color: var(--text-primary);
}

@media (max-width: 768px) {
  .auth-container {
    padding: 0 12px;
  }

  .auth-card {
    width: 100%;
    max-width: 400px;
  }
}
</style>
