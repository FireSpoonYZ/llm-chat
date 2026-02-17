import axios from 'axios'
import { refreshSession } from './auth'

const client = axios.create({
  baseURL: '/api',
  headers: { 'Content-Type': 'application/json' },
  withCredentials: true,
})

function isAuthPage(pathname: string): boolean {
  return pathname === '/login' || pathname === '/register'
}

// Response interceptor: auto-refresh on 401
client.interceptors.response.use(
  (response) => response,
  async (error) => {
    const originalRequest = error.config
    const requestUrl = String(originalRequest?.url || '')
    const isRefreshCall = requestUrl.includes('/auth/refresh')
    if (error.response?.status === 401 && originalRequest && !originalRequest._retry && !isRefreshCall) {
      originalRequest._retry = true
      const refreshed = await refreshSession()
      if (refreshed) {
        return client(originalRequest)
      }
      const currentPath = window.location.pathname
      if (!isAuthPage(currentPath)) {
        window.location.replace('/login')
      }
    }
    return Promise.reject(error)
  }
)

export default client
