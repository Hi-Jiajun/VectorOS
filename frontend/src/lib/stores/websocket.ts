/**
 * Svelte store for WebSocket connection state and real-time data
 */
import { writable, derived } from 'svelte/store';
import { getWebSocket, type WsMessage, type ConnectionStatus, type VppInterfaceStats } from '../websocket';

// Connection status
export const wsStatus = writable<ConnectionStatus>('disconnected');

// System stats (updated via WebSocket)
export interface SystemStats {
  cpu_percent: number;
  cpu_count: number;
  memory_total: number;
  memory_used: number;
  memory_percent: number;
  disk_total: number;
  disk_used: number;
  disk_percent: number;
}

export const systemStats = writable<SystemStats>({
  cpu_percent: 0,
  cpu_count: 0,
  memory_total: 0,
  memory_used: 0,
  memory_percent: 0,
  disk_total: 0,
  disk_used: 0,
  disk_percent: 0,
});

// VPP stats (updated via WebSocket)
export interface VppStats {
  packet_rate_rx: number;
  packet_rate_tx: number;
  nat_sessions: number;
  pppoe_status: string;
  interfaces: VppInterfaceStats[];
}

export const vppStats = writable<VppStats>({
  packet_rate_rx: 0,
  packet_rate_tx: 0,
  nat_sessions: 0,
  pppoe_status: 'unknown',
  interfaces: [],
});

// Connection status derived store for UI display
export const isConnected = derived(wsStatus, ($status) => $status === 'connected');

// Initialize WebSocket connection and subscribe to updates
let initialized = false;

export function initWebSocket(): void {
  if (initialized) return;
  initialized = true;

  const ws = getWebSocket();

  // Subscribe to status changes
  ws.onStatusChange((status) => {
    wsStatus.set(status);
  });

  // Subscribe to messages
  ws.onMessage((message: WsMessage) => {
    switch (message.type) {
      case 'SystemUpdate':
        systemStats.set({
          cpu_percent: message.cpu_percent,
          cpu_count: message.cpu_count,
          memory_total: message.memory_total,
          memory_used: message.memory_used,
          memory_percent: message.memory_percent,
          disk_total: message.disk_total,
          disk_used: message.disk_used,
          disk_percent: message.disk_percent,
        });
        break;

      case 'VppUpdate':
        vppStats.set({
          packet_rate_rx: message.packet_rate_rx,
          packet_rate_tx: message.packet_rate_tx,
          nat_sessions: message.nat_sessions,
          pppoe_status: message.pppoe_status,
          interfaces: message.interfaces,
        });
        break;

      case 'Connected':
        console.log('[Store] WebSocket connected:', message.message);
        break;

      case 'AlertUpdate':
        console.log(`[Alert] ${message.level}: ${message.message}`);
        break;
    }
  });

  // Connect
  ws.connect();
}

// Helper functions for formatting
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

export function formatRate(packetsPerSec: number): string {
  if (packetsPerSec >= 1000000) {
    return (packetsPerSec / 1000000).toFixed(2) + ' Mpps';
  } else if (packetsPerSec >= 1000) {
    return (packetsPerSec / 1000).toFixed(2) + ' Kpps';
  }
  return packetsPerSec.toFixed(0) + ' pps';
}
