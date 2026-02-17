import type { WsMessage } from '../types'

type MessageHandler = (msg: WsMessage) => void

const HEARTBEAT_INTERVAL = 25_000
const HEARTBEAT_TIMEOUT = 10_000

export class WebSocketManager {
  private ws: WebSocket | null = null
  private url: string
  private handlers: Map<string, MessageHandler[]> = new Map()
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private reconnectDelay = 1000
  private maxReconnectDelay = 30000
  private shouldReconnect = true
  private sessionRefresher: (() => Promise<boolean>) | null

  // Heartbeat
  private heartbeatInterval: ReturnType<typeof setInterval> | null = null
  private heartbeatTimeout: ReturnType<typeof setTimeout> | null = null

  // Bound lifecycle handlers (for cleanup)
  private boundOnVisibilityChange = this.onVisibilityChange.bind(this)
  private boundOnOnline = this.onOnline.bind(this)

  constructor(url: string, sessionRefresher?: () => Promise<boolean>) {
    this.url = url
    this.sessionRefresher = sessionRefresher ?? null
  }

  connect() {
    this.shouldReconnect = true
    this.addLifecycleListeners()
    this.doConnect()
  }

  private async ensureSession(): Promise<boolean> {
    if (this.sessionRefresher) {
      return this.sessionRefresher()
    }
    return true
  }

  private doConnect() {
    this.ws = new WebSocket(this.url)

    this.ws.onopen = () => {
      this.reconnectDelay = 1000
      this.startHeartbeat()
      this.emit({ type: 'ws_connected' })
    }

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as WsMessage
        if (msg.type === 'pong') {
          this.clearHeartbeatTimeout()
          return
        }
        this.emit(msg)
      } catch { /* ignore parse errors */ }
    }

    this.ws.onclose = () => {
      this.stopHeartbeat()
      this.emit({ type: 'ws_disconnected' })
      if (this.shouldReconnect) {
        this.reconnectTimer = setTimeout(async () => {
          this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay)
          const hasSession = await this.ensureSession()
          if (hasSession) {
            this.doConnect()
          } else {
            this.emit({ type: 'auth_failed' })
          }
        }, this.reconnectDelay)
      }
    }

    this.ws.onerror = () => {
      this.ws?.close()
    }
  }

  disconnect() {
    this.shouldReconnect = false
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
    this.stopHeartbeat()
    this.removeLifecycleListeners()
    this.ws?.close()
    this.ws = null
  }

  send(msg: WsMessage): boolean {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg))
      return true
    }
    return false
  }

  on(type: string, handler: MessageHandler) {
    if (!this.handlers.has(type)) this.handlers.set(type, [])
    this.handlers.get(type)!.push(handler)
  }

  off(type: string, handler: MessageHandler) {
    const handlers = this.handlers.get(type)
    if (handlers) {
      const idx = handlers.indexOf(handler)
      if (idx >= 0) handlers.splice(idx, 1)
    }
  }

  private emit(msg: WsMessage) {
    const handlers = this.handlers.get(msg.type)
    if (handlers) handlers.forEach(h => h(msg))
    // Also emit to wildcard handlers
    const wildcardHandlers = this.handlers.get('*')
    if (wildcardHandlers) wildcardHandlers.forEach(h => h(msg))
  }

  // --- Heartbeat ---

  private startHeartbeat() {
    this.stopHeartbeat()
    this.heartbeatInterval = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'ping' }))
        this.heartbeatTimeout = setTimeout(() => {
          // No pong received — force close to trigger reconnect
          this.ws?.close()
        }, HEARTBEAT_TIMEOUT)
      }
    }, HEARTBEAT_INTERVAL)
  }

  private stopHeartbeat() {
    if (this.heartbeatInterval) { clearInterval(this.heartbeatInterval); this.heartbeatInterval = null }
    this.clearHeartbeatTimeout()
  }

  private clearHeartbeatTimeout() {
    if (this.heartbeatTimeout) { clearTimeout(this.heartbeatTimeout); this.heartbeatTimeout = null }
  }

  // --- Lifecycle listeners ---

  private addLifecycleListeners() {
    document.addEventListener('visibilitychange', this.boundOnVisibilityChange)
    window.addEventListener('online', this.boundOnOnline)
  }

  private removeLifecycleListeners() {
    document.removeEventListener('visibilitychange', this.boundOnVisibilityChange)
    window.removeEventListener('online', this.boundOnOnline)
  }

  private async onVisibilityChange() {
    if (document.visibilityState !== 'visible' || !this.shouldReconnect) return
    if (!this.ws || this.ws.readyState === WebSocket.CLOSED || this.ws.readyState === WebSocket.CLOSING) {
      // Dead connection — reconnect immediately
      if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
      this.reconnectDelay = 1000
      const hasSession = await this.ensureSession()
      if (hasSession) {
        this.doConnect()
      } else {
        this.emit({ type: 'auth_failed' })
      }
    } else if (this.ws.readyState === WebSocket.OPEN) {
      // Looks alive — send a ping to verify
      this.ws.send(JSON.stringify({ type: 'ping' }))
    }
  }

  private async onOnline() {
    if (!this.shouldReconnect) return
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      // Network is back — reconnect immediately (skip backoff)
      if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
      this.reconnectDelay = 1000
      const hasSession = await this.ensureSession()
      if (hasSession) {
        this.doConnect()
      } else {
        this.emit({ type: 'auth_failed' })
      }
    }
  }
}
