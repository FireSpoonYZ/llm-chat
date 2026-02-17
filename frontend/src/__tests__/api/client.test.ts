import { describe, it, expect, beforeEach, vi } from 'vitest'

let responseFulfilled: (response: any) => any
let responseRejected: (error: any) => any
let mockClientInstance: any

vi.mock('axios', () => {
  mockClientInstance = vi.fn()
  mockClientInstance.interceptors = {
    request: {
      use: vi.fn(),
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
  refreshSession: vi.fn(),
}))

import { refreshSession } from '../../api/auth'

await import('../../api/client')

describe('axios client interceptors', () => {
  beforeEach(() => {
    mockClientInstance.mockReset()
    vi.mocked(refreshSession).mockReset()
  })

  it('creates client with cookie credentials enabled', async () => {
    const { default: axios } = await import('axios')
    expect(vi.mocked(axios.create)).toHaveBeenCalledWith(
      expect.objectContaining({ withCredentials: true }),
    )
  })

  it('passes through successful responses', () => {
    const response = { data: 'ok', status: 200 }
    expect(responseFulfilled(response)).toBe(response)
  })

  it('refreshes session on 401 and retries', async () => {
    vi.mocked(refreshSession).mockResolvedValue(true)
    mockClientInstance.mockResolvedValue({ data: 'retried' })

    const originalRequest = {
      url: '/users/me',
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await responseRejected(error)

    expect(refreshSession).toHaveBeenCalled()
    expect(originalRequest._retry).toBe(true)
    expect(mockClientInstance).toHaveBeenCalledWith(originalRequest)
  })

  it('does not retry /auth/refresh requests', async () => {
    const originalRequest = {
      url: '/auth/refresh',
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(refreshSession).not.toHaveBeenCalled()
  })

  it('does not retry when _retry is already true', async () => {
    const originalRequest = {
      url: '/users/me',
      headers: {} as Record<string, string>,
      _retry: true,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(refreshSession).not.toHaveBeenCalled()
  })

  it('redirects to /login when session refresh fails', async () => {
    vi.mocked(refreshSession).mockResolvedValue(false)
    const replaceSpy = vi.fn()
    Object.defineProperty(window, 'location', {
      value: {
        pathname: '/settings',
        replace: replaceSpy,
      },
      writable: true,
      configurable: true,
    })

    const originalRequest = {
      url: '/users/me',
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(replaceSpy).toHaveBeenCalledWith('/login')
  })

  it('does not redirect again when already on login page', async () => {
    vi.mocked(refreshSession).mockResolvedValue(false)
    const replaceSpy = vi.fn()
    Object.defineProperty(window, 'location', {
      value: {
        pathname: '/login',
        replace: replaceSpy,
      },
      writable: true,
      configurable: true,
    })

    const originalRequest = {
      url: '/users/me',
      headers: {} as Record<string, string>,
      _retry: false,
    }
    const error = {
      config: originalRequest,
      response: { status: 401 },
    }

    await expect(responseRejected(error)).rejects.toBe(error)
    expect(replaceSpy).not.toHaveBeenCalled()
  })
})
