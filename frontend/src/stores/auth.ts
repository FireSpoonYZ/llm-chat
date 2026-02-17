import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { User } from '../types'
import * as authApi from '../api/auth'
import * as usersApi from '../api/users'
import router from '../router'

export const useAuthStore = defineStore('auth', () => {
  const user = ref<User | null>(null)
  const sessionChecked = ref(false)
  const isAuthenticated = computed(() => !!user.value)
  let ensureSessionPromise: Promise<void> | null = null

  async function login(username: string, password: string) {
    const data = await authApi.login(username, password)
    user.value = data.user
    sessionChecked.value = true
  }

  async function register(username: string, email: string, password: string) {
    const data = await authApi.register(username, email, password)
    user.value = data.user
    sessionChecked.value = true
  }

  async function ensureSession() {
    if (sessionChecked.value) return
    if (ensureSessionPromise) return ensureSessionPromise
    ensureSessionPromise = (async () => {
      try {
        user.value = await usersApi.getProfile()
      } catch {
        user.value = null
      } finally {
        sessionChecked.value = true
        ensureSessionPromise = null
      }
    })()
    await ensureSessionPromise
  }

  async function logout() {
    try { await authApi.logout() } catch { /* ignore */ }
    user.value = null
    sessionChecked.value = true
    router.push('/login')
  }

  return { user, isAuthenticated, sessionChecked, login, register, ensureSession, logout }
})
