import type { WsMessage } from '../types'

type MessageHandler = (msg: WsMessage) => void

export class WebSocketManager {
  private ws: WebSocket | null = null
  private url: string
  private handlers: Map<string, MessageHandler[]> = new Map()
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private reconnectDelay = 1000
  private maxReconnectDelay = 30000
  private shouldReconnect = true

  constructor(url: string) {
    this.url = url
  }

  connect(token: string) {
    this.shouldReconnect = true
    this.doConnect(token)
  }

  private doConnect(token: string) {
    const wsUrl = `${this.url}?token=${token}`
    this.ws = new WebSocket(wsUrl)

    this.ws.onopen = () => {
      this.reconnectDelay = 1000
      this.emit({ type: 'ws_connected' })
    }

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as WsMessage
        this.emit(msg)
      } catch { /* ignore parse errors */ }
    }

    this.ws.onclose = () => {
      this.emit({ type: 'ws_disconnected' })
      if (this.shouldReconnect) {
        this.reconnectTimer = setTimeout(() => {
          this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay)
          this.doConnect(token)
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
    this.ws?.close()
    this.ws = null
  }

  send(msg: WsMessage) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg))
    }
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
}