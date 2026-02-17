import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '../../stores/auth'

vi.mock('../../api/auth', () => ({
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
  refreshSession: vi.fn(),
}))

vi.mock('../../api/users', () => ({
  getProfile: vi.fn(),
}))

const mockPush = vi.fn()
vi.mock('../../router', () => ({
  default: { push: (...args: unknown[]) => mockPush(...args) },
}))

import * as authApi from '../../api/auth'
import * as usersApi from '../../api/users'

const mockUser = { id: 'u1', username: 'alice', email: 'alice@test.com', is_admin: false }

describe('auth store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('login stores user in state', async () => {
    vi.mocked(authApi.login).mockResolvedValue({ user: mockUser })
    const store = useAuthStore()

    await store.login('alice', 'pass')

    expect(store.user).toEqual(mockUser)
    expect(store.isAuthenticated).toBe(true)
    expect(store.sessionChecked).toBe(true)
  })

  it('register stores user in state', async () => {
    vi.mocked(authApi.register).mockResolvedValue({ user: mockUser })
    const store = useAuthStore()

    await store.register('alice', 'alice@test.com', 'pass')

    expect(store.user).toEqual(mockUser)
    expect(store.isAuthenticated).toBe(true)
    expect(store.sessionChecked).toBe(true)
  })

  it('ensureSession loads profile and marks session checked', async () => {
    vi.mocked(usersApi.getProfile).mockResolvedValue(mockUser)
    const store = useAuthStore()

    await store.ensureSession()

    expect(usersApi.getProfile).toHaveBeenCalledTimes(1)
    expect(store.user).toEqual(mockUser)
    expect(store.isAuthenticated).toBe(true)
    expect(store.sessionChecked).toBe(true)
  })

  it('ensureSession is deduplicated while pending', async () => {
    let resolveProfile!: (value: typeof mockUser) => void
    vi.mocked(usersApi.getProfile).mockImplementation(
      () => new Promise((resolve) => { resolveProfile = resolve }),
    )
    const store = useAuthStore()

    const p1 = store.ensureSession()
    const p2 = store.ensureSession()

    resolveProfile(mockUser)
    await Promise.all([p1, p2])

    expect(usersApi.getProfile).toHaveBeenCalledTimes(1)
  })

  it('ensureSession handles unauthenticated responses', async () => {
    vi.mocked(usersApi.getProfile).mockRejectedValue(new Error('401'))
    const store = useAuthStore()

    await store.ensureSession()

    expect(store.user).toBeNull()
    expect(store.isAuthenticated).toBe(false)
    expect(store.sessionChecked).toBe(true)
  })

  it('logout clears state and redirects', async () => {
    vi.mocked(authApi.logout).mockResolvedValue(undefined)
    const authStore = useAuthStore()
    authStore.user = mockUser

    await authStore.logout()

    expect(authStore.user).toBeNull()
    expect(authStore.isAuthenticated).toBe(false)
    expect(authStore.sessionChecked).toBe(true)
    expect(mockPush).toHaveBeenCalledWith('/login')
  })

  it('logout still clears state when API call fails', async () => {
    vi.mocked(authApi.logout).mockRejectedValue(new Error('network'))
    const authStore = useAuthStore()
    authStore.user = mockUser

    await authStore.logout()

    expect(authStore.user).toBeNull()
    expect(authStore.isAuthenticated).toBe(false)
    expect(mockPush).toHaveBeenCalledWith('/login')
  })
})
