<template>
  <div class="auth-container">
    <el-card class="auth-card">
      <template #header>
        <h2>Claude Chat</h2>
      </template>
      <el-form @submit.prevent="handleRegister" label-position="top">
        <el-form-item label="Username">
          <el-input v-model="username" placeholder="Username" />
        </el-form-item>
        <el-form-item label="Email">
          <el-input v-model="email" type="email" placeholder="Email" />
        </el-form-item>
        <el-form-item label="Password">
          <el-input v-model="password" type="password" placeholder="Password" show-password />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" native-type="submit" :loading="loading" style="width: 100%">
            Register
          </el-button>
        </el-form-item>
        <p style="text-align: center">
          Already have an account? <router-link to="/login">Login</router-link>
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
const email = ref('')
const password = ref('')
const loading = ref(false)

async function handleRegister() {
  loading.value = true
  try {
    await auth.register(username.value, email.value, password.value)
    router.push('/')
  } catch (err: unknown) {
    const error = err as { response?: { data?: { message?: string } } }
    ElMessage.error(error.response?.data?.message || 'Registration failed')
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
