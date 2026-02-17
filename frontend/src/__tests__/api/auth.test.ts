import { describe, it, expect, beforeEach, vi } from 'vitest'
import axios from 'axios'

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

import { refreshSession } from '../../api/auth'

describe('refreshSession', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns true when refresh succeeds', async () => {
    vi.mocked(axios.post).mockResolvedValueOnce({ data: {} })

    const result = await refreshSession()

    expect(result).toBe(true)
    expect(axios.post).toHaveBeenCalledWith('/api/auth/refresh', {}, { withCredentials: true })
  })

  it('returns false when refresh fails', async () => {
    vi.mocked(axios.post).mockRejectedValueOnce(new Error('network'))

    const result = await refreshSession()

    expect(result).toBe(false)
  })

  it('deduplicates concurrent refresh calls', async () => {
    vi.mocked(axios.post).mockResolvedValue({ data: {} })

    const [r1, r2] = await Promise.all([refreshSession(), refreshSession()])
    expect(r1).toBe(true)
    expect(r2).toBe(true)
    expect(axios.post).toHaveBeenCalledTimes(1)
  })
})
