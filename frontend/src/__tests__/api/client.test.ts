import { describe, it, expect, beforeEach, vi } from 'vitest'

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

// Capture interceptors registered by client.ts
let requestFulfilled: (config: any) => any
let responseFulfilled: (response: any) => any
let responseRejected: (error: any) => any
let mockClientInstance: any

vi.mock('axios', () => {
  mockClientInstance = vi.fn() // callable for retries
  mockClientInstance.interceptors = {
    request: {
      use: vi.fn((fulfilled: any) => { requestFulfilled = fulfilled }),
    },
    response: {
      use: vi.fn((fulfilled: any, rejected: any) => {
        responseFulfilled = fulfilled
        responseRejected = rejected
      }),
    },
  }
  return {
    default: {
      create: vi.fn(() => mockClientInstance),
    },
  }
})

vi.mock('../../api/auth', () => ({
  refreshAccessToken: vi.fn(),
}))

import { refreshAccessToken } from '../../api/auth'

// Force client.ts to execute and register interceptors
await import('../../api/client')

describe('axios client interceptors', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorageMock.clear()
  })

  it('attaches Bearer token from localStorage', () => {
    storage['access_token'] = 'my-token'
    const config = { headers: {} as Record<string, string> }
    const result = requestFulfilled(config)
    expect(result.headers.Authorization).toBe('Bearer my-token')
  })

  it('does not attach token when none exists', () => {
    const config = { headers: {} as Record<string, string> }
    const result = requestFulfilled(config)
    expect(result.headers.Authorization).toBeUndefined()
  })

  it('passes through successful responses', () => {
    const response = { data: 'ok', status: 200 }
    expect(responseFulfilled(response)).toBe(response)
  })

  it('refreshes token on 401 and retries', async () => {
    vi.mocked(refreshAccessToken).mockResolvedValue('new-token')
    mockClientInstance.mockResolvedValue({ data: 'retried' })

    const originalRequest = {
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await responseRejected(error)

    expect(refreshAccessToken).toHaveBeenCalled()
    expect(originalRequest.headers.Authorization).toBe('Bearer new-token')
    expect(originalRequest._retry).toBe(true)
    expect(mockClientInstance).toHaveBeenCalledWith(originalRequest)
  })

  it('does not retry on second 401 (_retry flag)', async () => {
    const originalRequest = {
      headers: {} as Record<string, string>,
      _retry: true,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(refreshAccessToken).not.toHaveBeenCalled()
  })

  it('redirects to /login when refresh fails', async () => {
    vi.mocked(refreshAccessToken).mockResolvedValue(null)
    const hrefSetter = vi.fn()
    Object.defineProperty(window, 'location', {
      value: { get href() { return '' }, set href(v: string) { hrefSetter(v) } },
      writable: true,
      configurable: true,
    })

    const originalRequest = {
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(hrefSetter).toHaveBeenCalledWith('/login')
  })
})