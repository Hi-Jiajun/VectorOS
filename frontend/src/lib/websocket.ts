/**
 * WebSocket client for VectorOS real-time dashboard updates
 */

export type WsMessage =
  | {
      type: 'SystemUpdate';
      cpu_percent: number;
      cpu_count: number;
      memory_total: number;
      memory_used: number;
      memory_percent: number;
      disk_total: number;
      disk_used: number;
      disk_percent: number;
    }
  | {
      type: 'VppUpdate';
      packet_rate_rx: number;
      packet_rate_tx: number;
      nat_sessions: number;
      pppoe_status: string;
      interfaces: VppInterfaceStats[];
    }
  | {
      type: 'InterfaceUpdate';
      name: string;
      state: string;
      rx_bytes: number;
      tx_bytes: number;
      rx_packets: number;
      tx_packets: number;
    }
  | {
      type: 'AlertUpdate';
      level: 'info' | 'warning' | 'error';
      message: string;
      timestamp: string;
    }
  | {
      type: 'Connected';
      message: string;
    };

export interface VppInterfaceStats {
  name: string;
  state: string;
  rx_bytes: number;
  tx_bytes: number;
}

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export type MessageHandler = (message: WsMessage) => void;
export type StatusHandler = (status: ConnectionStatus) => void;

export class VectorOSWebSocket {
  private ws: WebSocket | null = null;
  private url: string;
  private handlers: MessageHandler[] = [];
  private statusHandlers: StatusHandler[] = [];
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private _status: ConnectionStatus = 'disconnected';

  constructor() {
    // Build WebSocket URL from current page location
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    this.url = `${protocol}//${window.location.host}/ws`;
  }

  get status(): ConnectionStatus {
    return this._status;
  }

  /**
   * Connect to the WebSocket server
   */
  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }

    this.setStatus('connecting');

    try {
      this.ws = new WebSocket(this.url);

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.setStatus('connected');
        console.log('[WS] Connected to VectorOS real-time updates');
      };

      this.ws.onmessage = (event) => {
        try {
          const message: WsMessage = JSON.parse(event.data);
          this.handlers.forEach((handler) => handler(message));
        } catch (e) {
          console.error('[WS] Failed to parse message:', e);
        }
      };

      this.ws.onclose = (event) => {
        console.log('[WS] Connection closed:', event.code, event.reason);
        this.setStatus('disconnected');
        this.scheduleReconnect();
      };

      this.ws.onerror = (error) => {
        console.error('[WS] Connection error:', error);
        this.setStatus('error');
      };
    } catch (e) {
      console.error('[WS] Failed to create WebSocket:', e);
      this.setStatus('error');
      this.scheduleReconnect();
    }
  }

  /**
   * Disconnect from the WebSocket server
   */
  disconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.reconnectAttempts = this.maxReconnectAttempts; // Prevent reconnect

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.setStatus('disconnected');
  }

  /**
   * Register a handler for incoming messages
   */
  onMessage(handler: MessageHandler): () => void {
    this.handlers.push(handler);
    return () => {
      this.handlers = this.handlers.filter((h) => h !== handler);
    };
  }

  /**
   * Register a handler for connection status changes
   */
  onStatusChange(handler: StatusHandler): () => void {
    this.statusHandlers.push(handler);
    return () => {
      this.statusHandlers = this.statusHandlers.filter((h) => h !== handler);
    };
  }

  private setStatus(status: ConnectionStatus): void {
    this._status = status;
    this.statusHandlers.forEach((handler) => handler(status));
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.log('[WS] Max reconnect attempts reached');
      return;
    }

    const delay = this.reconnectDelay * Math.pow(1.5, this.reconnectAttempts);
    this.reconnectAttempts++;

    console.log(`[WS] Reconnecting in ${Math.round(delay)}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, delay);
  }
}

// Singleton instance
let instance: VectorOSWebSocket | null = null;

export function getWebSocket(): VectorOSWebSocket {
  if (!instance) {
    instance = new VectorOSWebSocket();
  }
  return instance;
}
