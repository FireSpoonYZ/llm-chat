import { describe, it, expect, beforeEach, vi } from 'vitest'
import axios from 'axios'

// localStorage mock
const storage: Record<string, string> = {}
const localStorageMock = {
  getItem: vi.fn((key: string) => storage[key] ?? null),
  setItem: vi.fn((key: string, value: string) => { storage[key] = value }),
  removeItem: vi.fn((key: string) => { delete storage[key] }),
  clear: vi.fn(() => { Object.keys(storage).forEach(k => delete storage[k]) }),
  get length() { return Object.keys(storage).length },
  key: vi.fn((i: number) => Object.keys(storage)[i] ?? null),
}
vi.stubGlobal('localStorage', localStorageMock)

vi.mock('axios', async () => {
  const actual = await vi.importActual<typeof import('axios')>('axios')
  return {
    ...actual,
    default: {
      ...actual.default,
      post: vi.fn(),
      create: vi.fn(() => ({
        interceptors: {
          request: { use: vi.fn() },
          response: { use: vi.fn() },
        },
      })),
    },
  }
})

// Must import after mock setup
import { refreshAccessToken } from '../../api/auth'

describe('refreshAccessToken', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorageMock.clear()
  })

  it('returns null when no refresh_token in localStorage', async () => {
    const result = await refreshAccessToken()
    expect(result).toBeNull()
  })

  it('refreshes token and updates localStorage', async () => {
    storage['refresh_token'] = 'old-rt'
    vi.mocked(axios.post).mockResolvedValueOnce({
      data: { access_token: 'new-at', refresh_token: 'new-rt' },
    })

    const result = await refreshAccessToken()

    expect(result).toBe('new-at')
    expect(localStorageMock.setItem).toHaveBeenCalledWith('access_token', 'new-at')
    expect(localStorageMock.setItem).toHaveBeenCalledWith('refresh_token', 'new-rt')
  })

  it('clears tokens on failure', async () => {
    storage['refresh_token'] = 'old-rt'
    storage['access_token'] = 'old-at'
    vi.mocked(axios.post).mockRejectedValueOnce(new Error('network'))

    const result = await refreshAccessToken()

    expect(result).toBeNull()
    expect(localStorageMock.removeItem).toHaveBeenCalledWith('access_token')
    expect(localStorageMock.removeItem).toHaveBeenCalledWith('refresh_token')
  })

  it('deduplicates concurrent refresh calls', async () => {
    storage['refresh_token'] = 'rt'
    vi.mocked(axios.post).mockResolvedValue({
      data: { access_token: 'at', refresh_token: 'rt2' },
    })

    const [r1, r2] = await Promise.all([refreshAccessToken(), refreshAccessToken()])
    expect(r1).toBe('at')
    expect(r2).toBe('at')
    // Only one API call made despite two concurrent calls
    expect(axios.post).toHaveBeenCalledTimes(1)
  })
})
