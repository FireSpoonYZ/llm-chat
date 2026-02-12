import axios from 'axios'
import client from './client'
import type { AuthResponse } from '../types'

let refreshPromise: Promise<string | null> | null = null

export async function refreshAccessToken(): Promise<string | null> {
  if (refreshPromise) return refreshPromise
  refreshPromise = (async () => {
    const rt = localStorage.getItem('refresh_token')
    if (!rt) return null
    try {
      const { data } = await axios.post('/api/auth/refresh', { refresh_token: rt })
      localStorage.setItem('access_token', data.access_token)
      localStorage.setItem('refresh_token', data.refresh_token)
      return data.access_token as string
    } catch {
      localStorage.removeItem('access_token')
      localStorage.removeItem('refresh_token')
      return null
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

export async function logout(refreshToken: string): Promise<void> {
  await client.post('/auth/logout', { refresh_token: refreshToken })
}
