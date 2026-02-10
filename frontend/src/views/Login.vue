<template>
  <div class="auth-container">
    <el-card class="auth-card">
      <template #header>
        <h2>Claude Chat</h2>
      </template>
      <el-form @submit.prevent="handleLogin" label-position="top">
        <el-form-item label="Username">
          <el-input v-model="username" placeholder="Username" />
        </el-form-item>
        <el-form-item label="Password">
          <el-input v-model="password" type="password" placeholder="Password" show-password />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" native-type="submit" :loading="loading" style="width: 100%">
            Login
          </el-button>
        </el-form-item>
        <p style="text-align: center">
          Don't have an account? <router-link to="/register">Register</router-link>
        </p>
      </el-form>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { useAuthStore } from '../stores/auth'

const auth = useAuthStore()
const router = useRouter()
const username = ref('')
const password = ref('')
const loading = ref(false)

async function handleLogin() {
  loading.value = true
  try {
    await auth.login(username.value, password.value)
    router.push('/')
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || 'Login failed')
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
  background: #f5f7fa;
}
.auth-card {
  width: 400px;
}
.auth-card h2 {
  margin: 0;
  text-align: center;
}
</style>
