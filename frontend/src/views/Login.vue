<template>
  <div class="auth-container">
    <el-card class="auth-card">
      <template #header>
        <h2>{{ t('auth.title') }}</h2>
      </template>
      <el-form ref="formRef" :model="form" :rules="rules" @submit.prevent="handleLogin" label-position="top">
        <el-form-item :label="t('auth.username')" prop="username">
          <el-input v-model="form.username" :placeholder="t('auth.username')" />
        </el-form-item>
        <el-form-item :label="t('auth.password')" prop="password">
          <el-input v-model="form.password" type="password" :placeholder="t('auth.password')" show-password />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" native-type="submit" :loading="loading" style="width: 100%">
            {{ t('auth.login') }}
          </el-button>
        </el-form-item>
        <p style="text-align: center">
          {{ t('auth.noAccount') }} <router-link to="/register">{{ t('auth.register') }}</router-link>
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
import { t } from '../i18n'

const auth = useAuthStore()
const router = useRouter()
const formRef = ref<FormInstance>()
const form = reactive({ username: '', password: '' })
const loading = ref(false)

const rules = computed<FormRules>(() => ({
  username: [{ required: true, message: t('auth.validation.usernameRequired'), trigger: 'blur' }],
  password: [{ required: true, message: t('auth.validation.passwordRequired'), trigger: 'blur' }],
}))

async function handleLogin() {
  if (!formRef.value) return
  const valid = await formRef.value.validate().catch(() => false)
  if (!valid) return
  loading.value = true
  try {
    await auth.login(form.username, form.password)
    router.push('/')
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || t('auth.messages.loginFailed'))
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
.auth-card h2 {
  margin: 0;
  text-align: center;
  color: var(--text-primary);
}
</style>
