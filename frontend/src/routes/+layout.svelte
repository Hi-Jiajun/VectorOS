<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { initWebSocket, wsStatus } from '$lib/stores/websocket';
  import type { ConnectionStatus } from '$lib/websocket';

  let connStatus: ConnectionStatus = 'disconnected';
  let unsubStatus: (() => void) | null = null;

  onMount(() => {
    initWebSocket();
    unsubStatus = wsStatus.subscribe((val) => { connStatus = val; });
  });

  onDestroy(() => {
    unsubStatus?.();
  });

  function statusColor(status: ConnectionStatus): string {
    switch (status) {
      case 'connected': return '#00ff88';
      case 'connecting': return '#ffaa00';
      case 'disconnected': return '#666';
      case 'error': return '#ff4444';
      default: return '#666';
    }
  }
</script>

<svelte:head>
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <meta name="theme-color" content="#0f0f23" />
</svelte:head>

<div class="app">
  <nav>
    <a href="/">Dashboard</a>
    <a href="/monitor">Monitor</a>
    <a href="/services">Services</a>
    <a href="/pppoe">PPPoE</a>
    <a href="/interfaces">Interfaces</a>
    <a href="/frr">FRRouting</a>
    <a href="/ipv6">IPv6</a>
    <a href="/dhcp">DHCP</a>
    <a href="/dns">DNS</a>
    <a href="/firewall">Firewall</a>
    <a href="/vpn">VPN</a>
    <a href="/conntrack">ConnTrack</a>
    <a href="/flow">Flow Monitor</a>
    <a href="/qos">QoS</a>
    <a href="/traffic">Traffic Control</a>
    <a href="/config">Configuration</a>
    <a href="/logs">Logs</a>
    <a href="/diag">Diagnostics</a>
    <a href="/settings">Settings</a>
    <a href="/swagger-ui/" target="_blank" rel="noopener noreferrer">API Docs</a>
    <div class="nav-spacer"></div>
    <div class="nav-status">
      <span class="status-dot" style="background: {statusColor(connStatus)}"></span>
      <span class="status-text">Real-time: {connStatus}</span>
    </div>
  </nav>

  <main>
    <slot />
  </main>
</div>

<style>
  .app {
    display: flex;
    min-height: 100vh;
  }

  nav {
    width: 200px;
    background: #1a1a2e;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    will-change: transform;
  }

  nav a {
    color: #e0e0e0;
    text-decoration: none;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    transition: background 0.15s ease-out;
    contain: layout style;
  }

  nav a:hover {
    background: #16213e;
  }

  .nav-spacer {
    flex: 1;
  }

  .nav-status {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    font-size: 0.8rem;
    color: #888;
    border-top: 1px solid #333;
    margin-top: 0.5rem;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
    animation: pulse 2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .status-text {
    white-space: nowrap;
  }

  main {
    flex: 1;
    padding: 2rem;
    background: #0f0f23;
    color: #e0e0e0;
    contain: layout;
  }
</style>
