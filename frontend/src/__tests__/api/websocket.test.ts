import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { WebSocketManager } from '../../api/websocket'

// Mock WebSocket
class MockWebSocket {
  static CONNECTING = 0
  static OPEN = 1
  static CLOSING = 2
  static CLOSED = 3

  readyState = MockWebSocket.CONNECTING
  url: string
  onopen: (() => void) | null = null
  onclose: (() => void) | null = null
  onmessage: ((event: { data: string }) => void) | null = null
  onerror: (() => void) | null = null
  send = vi.fn()
  close = vi.fn()

  constructor(url: string) {
    this.url = url
    // Auto-open after microtask
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN
      this.onopen?.()
    }, 0)
  }
}

// Assign static constants
Object.defineProperty(MockWebSocket, 'OPEN', { value: 1 })
Object.defineProperty(MockWebSocket, 'CLOSED', { value: 3 })
Object.defineProperty(MockWebSocket, 'CLOSING', { value: 2 })

vi.stubGlobal('WebSocket', MockWebSocket)

describe('WebSocketManager', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('emits ws_connected on open', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const handler = vi.fn()
    wsm.on('ws_connected', handler)
    wsm.connect()

    await vi.advanceTimersByTimeAsync(1)
    expect(handler).toHaveBeenCalled()
    const ws = (wsm as any).ws as MockWebSocket
    expect(ws.url).toBe('ws://localhost/ws')
  })

  it('send returns false when socket not open', () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const result = wsm.send({ type: 'ping' })
    expect(result).toBe(false)
  })

  it('send returns true when socket is open', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    const result = wsm.send({ type: 'ping' })
    expect(result).toBe(true)
  })

  it('emits ws_disconnected on close', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const disconnectHandler = vi.fn()
    wsm.on('ws_disconnected', disconnectHandler)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    // Simulate close
    const ws = (wsm as any).ws as MockWebSocket
    ws.readyState = MockWebSocket.CLOSED
    ws.onclose?.()

    expect(disconnectHandler).toHaveBeenCalled()
  })

  it('reconnects with exponential backoff on close', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    // First close
    const ws1 = (wsm as any).ws as MockWebSocket
    ws1.readyState = MockWebSocket.CLOSED
    ws1.onclose?.()

    // After 1000ms (initial delay), should reconnect
    await vi.advanceTimersByTimeAsync(1000)
    const ws2 = (wsm as any).ws as MockWebSocket
    expect(ws2).not.toBe(ws1)

    // Simulate second close
    ws2.readyState = MockWebSocket.CLOSED
    ws2.onclose?.()

    // After 2000ms (doubled delay), should reconnect again
    await vi.advanceTimersByTimeAsync(2000)
    const ws3 = (wsm as any).ws as MockWebSocket
    expect(ws3).not.toBe(ws2)
  })

  it('refreshes session during reconnect', async () => {
    const sessionRefresher = vi.fn().mockResolvedValue(true)
    const wsm = new WebSocketManager('ws://localhost/ws', sessionRefresher)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    // Close to trigger reconnect
    const ws1 = (wsm as any).ws as MockWebSocket
    ws1.readyState = MockWebSocket.CLOSED
    ws1.onclose?.()

    await vi.advanceTimersByTimeAsync(1000)
    expect(sessionRefresher).toHaveBeenCalled()
  })

  it('emits auth_failed when session refresh fails', async () => {
    const sessionRefresher = vi.fn().mockResolvedValue(false)
    const wsm = new WebSocketManager('ws://localhost/ws', sessionRefresher)
    const authHandler = vi.fn()
    wsm.on('auth_failed', authHandler)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    const ws1 = (wsm as any).ws as MockWebSocket
    ws1.readyState = MockWebSocket.CLOSED
    ws1.onclose?.()

    await vi.advanceTimersByTimeAsync(1000)
    expect(authHandler).toHaveBeenCalled()
  })

  it('dispatches messages to correct handlers', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const deltaHandler = vi.fn()
    wsm.on('assistant_delta', deltaHandler)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    const ws = (wsm as any).ws as MockWebSocket
    ws.onmessage?.({ data: JSON.stringify({ type: 'assistant_delta', delta: 'hello' }) })

    expect(deltaHandler).toHaveBeenCalledWith({ type: 'assistant_delta', delta: 'hello' })
  })

  it('does not reconnect after disconnect()', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const connectHandler = vi.fn()
    wsm.on('ws_connected', connectHandler)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)
    expect(connectHandler).toHaveBeenCalledTimes(1)

    wsm.disconnect()

    // Advance past any reconnect timers
    await vi.advanceTimersByTimeAsync(60000)
    expect(connectHandler).toHaveBeenCalledTimes(1)
  })

  it('off removes a handler', async () => {
    const wsm = new WebSocketManager('ws://localhost/ws')
    const handler = vi.fn()
    wsm.on('test_event', handler)
    wsm.off('test_event', handler)
    wsm.connect()
    await vi.advanceTimersByTimeAsync(1)

    const ws = (wsm as any).ws as MockWebSocket
    ws.onmessage?.({ data: JSON.stringify({ type: 'test_event' }) })

    expect(handler).not.toHaveBeenCalled()
  })
})
