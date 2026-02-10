import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { User } from '../types'
import * as authApi from '../api/auth'
import router from '../router'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null)
  const accessToken = ref(localStorage.getItem('access_token') || '')
  const isAuthenticated = computed(() => !!accessToken.value)

  async function login(username: string, password: string) {
    const data = await authApi.login(username, password)
    accessToken.value = data.access_token
    localStorage.setItem('access_token', data.access_token)
    localStorage.setItem('refresh_token', data.refresh_token)
    user.value = data.user
  }

  async function register(username: string, email: string, password: string) {
    const data = await authApi.register(username, email, password)
    accessToken.value = data.access_token
    localStorage.setItem('access_token', data.access_token)
    localStorage.setItem('refresh_token', data.refresh_token)
    user.value = data.user
  }

  async function logout() {
    const refreshToken = localStorage.getItem('refresh_token')
    if (refreshToken) {
      try { await authApi.logout(refreshToken) } catch { /* ignore */ }
    }
    accessToken.value = ''
    user.value = null
    localStorage.removeItem('access_token')
    localStorage.removeItem('refresh_token')
    router.push('/login')
  }

  return { user, accessToken, isAuthenticated, login, register, logout }
})
