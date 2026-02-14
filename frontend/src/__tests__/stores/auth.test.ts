import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '../../stores/auth'

// localStorage mock
const store: Record<string, string> = {}
const localStorageMock = {
  getItem: vi.fn((key: string) => store[key] ?? null),
  setItem: vi.fn((key: string, value: string) => { store[key] = value }),
  removeItem: vi.fn((key: string) => { delete store[key] }),
  clear: vi.fn(() => { Object.keys(store).forEach(k => delete store[k]) }),
  get length() { return Object.keys(store).length },
  key: vi.fn((i: number) => Object.keys(store)[i] ?? null),
}
vi.stubGlobal('localStorage', localStorageMock)

vi.mock('../../api/auth', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
  refreshAccessToken: vi.fn(),
}))

const mockPush = vi.fn()
vi.mock('../../router', () => ({
  default: { push: (...args: unknown[]) => mockPush(...args) },
}))

import * as authApi from '../../api/auth'

const mockUser = { id: 'u1', username: 'alice', email: 'alice@test.com', is_admin: false }

describe('auth store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    localStorageMock.clear()
  })

  it('login stores tokens and user in state + localStorage', async () => {
    vi.mocked(authApi.login).mockResolvedValue({
      access_token: 'at',
      refresh_token: 'rt',
      user: mockUser,
    })
    const store = useAuthStore()
    await store.login('alice', 'pass')
    expect(localStorageMock.setItem).toHaveBeenCalledWith('access_token', 'at')
    expect(localStorageMock.setItem).toHaveBeenCalledWith('refresh_token', 'rt')
    expect(store.accessToken).toBe('at')
    expect(store.user).toEqual(mockUser)
    expect(store.isAuthenticated).toBe(true)
  })

  it('register stores tokens and user', async () => {
    vi.mocked(authApi.register).mockResolvedValue({
      access_token: 'at2',
      refresh_token: 'rt2',
      user: mockUser,
    })
    const store = useAuthStore()
    await store.register('alice', 'alice@test.com', 'pass')
    expect(localStorageMock.setItem).toHaveBeenCalledWith('access_token', 'at2')
    expect(store.isAuthenticated).toBe(true)
    expect(store.user).toEqual(mockUser)
  })

  it('logout clears state and redirects', async () => {
    store['access_token'] = 'at'
    store['refresh_token'] = 'rt'
    vi.mocked(authApi.logout).mockResolvedValue(undefined)

    const authStore = useAuthStore()
    authStore.accessToken = 'at'
    authStore.user = mockUser

    await authStore.logout()

    expect(authStore.accessToken).toBe('')
    expect(authStore.user).toBeNull()
    expect(authStore.isAuthenticated).toBe(false)
    expect(localStorageMock.removeItem).toHaveBeenCalledWith('access_token')
    expect(localStorageMock.removeItem).toHaveBeenCalledWith('refresh_token')
    expect(mockPush).toHaveBeenCalledWith('/login')
  })

  it('logout still clears state when API call fails', async () => {
    store['refresh_token'] = 'rt'
    vi.mocked(authApi.logout).mockRejectedValue(new Error('network'))

    const authStore = useAuthStore()
    authStore.accessToken = 'at'

    await authStore.logout()

    expect(authStore.accessToken).toBe('')
    expect(authStore.isAuthenticated).toBe(false)
    expect(mockPush).toHaveBeenCalledWith('/login')
  })

  it('initializes accessToken from localStorage', () => {
    store['access_token'] = 'persisted-token'
    const authStore = useAuthStore()
    expect(authStore.accessToken).toBe('persisted-token')
    expect(authStore.isAuthenticated).toBe(true)
  })

  it('isAuthenticated is false when no token', () => {
    const authStore = useAuthStore()
    expect(authStore.isAuthenticated).toBe(false)
  })
})