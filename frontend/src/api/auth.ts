import axios from 'axios'
import client from './client'
import type { AuthResponse } from '../types'

let refreshPromise: Promise<boolean> | null = null

export async function refreshSession(): Promise<boolean> {
  if (refreshPromise) return refreshPromise
  refreshPromise = (async () => {
    try {
      await axios.post('/api/auth/refresh', {}, { withCredentials: true })
      return true
    } catch {
      return false
    }
  })()
  try { return await refreshPromise } finally { refreshPromise = null }
}

export async function register(username: string, email: string, password: string): Promise<AuthResponse> {
  const { data } = await client.post<AuthResponse>('/auth/register', { username, email, password })
  return data
}

export async function login(username: string, password: string): Promise<AuthResponse> {
  const { data } = await client.post<AuthResponse>('/auth/login', { username, password })
  return data
}

export async function logout(): Promise<void> {
  await client.post('/auth/logout', {})
}
