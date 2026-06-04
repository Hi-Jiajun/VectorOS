<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { initWebSocket, systemStats, vppStats, wsStatus, formatBytes, formatRate } from '$lib/stores/websocket';
  import type { ConnectionStatus } from '$lib/websocket';

  let health: any = null;
  let interfaces: any[] = [];
  let routes: any[] = [];
  let ipv6Status: any = null;
  let services: any[] = [];

  // Unsubscribe functions
  let unsubSystem: (() => void) | null = null;
  let unsubVpp: (() => void) | null = null;
  let unsubStatus: (() => void) | null = null;

  // Local reactive state from stores
  let sysStats = {
    cpu_percent: 0,
    cpu_count: 0,
    memory_total: 0,
    memory_used: 0,
    memory_percent: 0,
    disk_total: 0,
    disk_used: 0,
    disk_percent: 0,
  };
  let vpp = {
    packet_rate_rx: 0,
    packet_rate_tx: 0,
    nat_sessions: 0,
    pppoe_status: 'unknown',
    interfaces: [] as any[],
  };
  let connStatus: ConnectionStatus = 'disconnected';

  onMount(async () => {
    // Initialize WebSocket for real-time updates
    initWebSocket();

    // Subscribe to stores
    unsubSystem = systemStats.subscribe((val) => { sysStats = val; });
    unsubVpp = vppStats.subscribe((val) => { vpp = val; });
    unsubStatus = wsStatus.subscribe((val) => { connStatus = val; });

    // Initial data load via REST
    const [healthRes, ifacesRes, routesRes, ipv6Res, servicesRes] = await Promise.all([
      fetch('/api/health').then(r => r.json()),
      fetch('/api/interfaces').then(r => r.json()),
      fetch('/api/routes').then(r => r.json()),
      fetch('/api/ipv6/status').then(r => r.json()).catch(() => null),
      fetch('/api/services').then(r => r.json()).catch(() => ({ services: [] }))
    ]);

    health = healthRes;
    interfaces = ifacesRes.interfaces;
    routes = routesRes.routes;
    ipv6Status = ipv6Res;
    services = servicesRes.services || [];
  });

  onDestroy(() => {
    unsubSystem?.();
    unsubVpp?.();
    unsubStatus?.();
  });

  function stateColor(state: string): string {
    switch (state) {
      case 'running': return '#00ff88';
      case 'stopped': return '#666';
      case 'starting': return '#ffaa00';
      case 'stopping': return '#ffaa00';
      case 'failed': return '#ff4444';
      default: return '#666';
    }
  }

  function statusDot(status: ConnectionStatus): string {
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
  <title>VectorOS - Dashboard</title>
</svelte:head>

<div class="dashboard">
  <div class="header-row">
    <h1>VectorOS Dashboard</h1>
    <div class="ws-indicator" title="WebSocket connection status">
      <span class="ws-dot" style="background: {statusDot(connStatus)}"></span>
      <span class="ws-label">{connStatus}</span>
    </div>
  </div>

  <!-- Real-time System Stats (from WebSocket) -->
  <div class="status-card">
    <h2>System Status</h2>
    {#if health}
      <div class="status-row">
        <span>Status: <span class="status-ok">{health.status}</span></span>
        <span class="status-sep">|</span>
        <span>Version: {health.version}</span>
        <span class="status-sep">|</span>
        <span>CPU: {sysStats.cpu_percent.toFixed(1)}% ({sysStats.cpu_count} cores)</span>
      </div>
      <div class="progress-bars">
        <div class="progress-item">
          <span class="progress-label">Memory</span>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {sysStats.memory_percent}%"></div>
          </div>
          <span class="progress-value">{formatBytes(sysStats.memory_used)} / {formatBytes(sysStats.memory_total)} ({sysStats.memory_percent.toFixed(1)}%)</span>
        </div>
        <div class="progress-item">
          <span class="progress-label">Disk</span>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {sysStats.disk_percent}%"></div>
          </div>
          <span class="progress-value">{formatBytes(sysStats.disk_used)} / {formatBytes(sysStats.disk_total)} ({sysStats.disk_percent.toFixed(1)}%)</span>
        </div>
      </div>
    {:else}
      <p>Loading...</p>
    {/if}
  </div>

  <!-- Real-time VPP Stats (from WebSocket) -->
  <div class="status-card">
    <h2>VPP Performance</h2>
    <div class="vpp-stats">
      <div class="vpp-stat">
        <span class="vpp-label">Packet Rate (RX)</span>
        <span class="vpp-value">{formatRate(vpp.packet_rate_rx)}</span>
      </div>
      <div class="vpp-stat">
        <span class="vpp-label">Packet Rate (TX)</span>
        <span class="vpp-value">{formatRate(vpp.packet_rate_tx)}</span>
      </div>
      <div class="vpp-stat">
        <span class="vpp-label">NAT Sessions</span>
        <span class="vpp-value">{vpp.nat_sessions}</span>
      </div>
      <div class="vpp-stat">
        <span class="vpp-label">PPPoE Status</span>
        <span class="vpp-value" style="color: {vpp.pppoe_status === 'disconnected' ? '#666' : '#00ff88'}">{vpp.pppoe_status}</span>
      </div>
    </div>
    {#if vpp.interfaces.length > 0}
      <div class="vpp-interfaces">
        <h3>Interfaces</h3>
        <div class="vpp-iface-list">
          {#each vpp.interfaces as iface}
            <div class="vpp-iface">
              <span class="iface-name">{iface.name}</span>
              <span class="iface-stats">RX: {formatBytes(iface.rx_bytes)} | TX: {formatBytes(iface.tx_bytes)}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <div class="grid">
    <div class="card">
      <h2>Interfaces</h2>
      <p class="count">{interfaces.length}</p>
      <p>active interfaces</p>
    </div>

    <div class="card">
      <h2>Routes</h2>
      <p class="count">{routes.length}</p>
      <p>routing entries</p>
    </div>

    <div class="card">
      <h2>IPv6</h2>
      <p class="count ipv6-status">{ipv6Status ? 'Active' : 'Inactive'}</p>
      <p>IPv6 connectivity</p>
    </div>

    <div class="card">
      <h2>IPv6 Neighbors</h2>
      <p class="count">{ipv6Status?.neighbors?.length || 0}</p>
      <p>NDP entries</p>
    </div>
  </div>

  {#if services.length > 0}
    <div class="status-card">
      <h2><a href="/services" class="services-link">Services</a></h2>
      <div class="services-overview">
        {#each services as svc}
          <div class="svc-item">
            <span class="svc-dot" style="color: {stateColor(svc.state)}">●</span>
            <span class="svc-name">{svc.display_name}</span>
            <span class="svc-state" style="color: {stateColor(svc.state)}">{svc.state}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .dashboard {
    max-width: 1200px;
  }

  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 2rem;
  }

  h1 {
    color: #00ff88;
    margin: 0;
  }

  .ws-indicator {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background: #1a1a2e;
    border-radius: 0.5rem;
    font-size: 0.85rem;
  }

  .ws-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    display: inline-block;
    animation: pulse 2s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .ws-label {
    color: #888;
    text-transform: capitalize;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 2rem;
    contain: layout style;
  }

  .status-card h2 {
    margin-bottom: 1rem;
  }

  .status-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 1rem;
  }

  .status-sep {
    color: #444;
  }

  .status-ok {
    color: #00ff88;
    font-weight: bold;
  }

  .progress-bars {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .progress-item {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .progress-label {
    width: 70px;
    color: #888;
    font-size: 0.9rem;
  }

  .progress-bar {
    flex: 1;
    height: 8px;
    background: #333;
    border-radius: 4px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #00ff88, #00cc6a);
    border-radius: 4px;
    transition: width 0.5s ease-out;
  }

  .progress-value {
    width: 250px;
    font-size: 0.85rem;
    color: #aaa;
    text-align: right;
  }

  .vpp-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .vpp-stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .vpp-label {
    font-size: 0.85rem;
    color: #888;
  }

  .vpp-value {
    font-size: 1.5rem;
    font-weight: bold;
    color: #00ff88;
  }

  .vpp-interfaces h3 {
    margin-top: 1rem;
    margin-bottom: 0.5rem;
    font-size: 0.95rem;
    color: #888;
  }

  .vpp-iface-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .vpp-iface {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .iface-name {
    font-weight: 600;
    color: #e0e0e0;
  }

  .iface-stats {
    font-size: 0.85rem;
    color: #888;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
  }

  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    text-align: center;
    contain: layout style;
  }

  .count {
    font-size: 3rem;
    font-weight: bold;
    color: #00ff88;
    margin: 0.5rem 0;
  }

  .ipv6-status {
    font-size: 2rem;
  }

  .services-link {
    color: #00ff88;
    text-decoration: none;
  }

  .services-link:hover {
    text-decoration: underline;
  }

  .services-overview {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem 1.5rem;
    margin-top: 0.75rem;
  }

  .svc-item {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.9rem;
  }

  .svc-dot {
    font-size: 0.8rem;
  }

  .svc-name {
    color: #ccc;
  }

  .svc-state {
    font-size: 0.8rem;
    text-transform: uppercase;
    font-weight: 600;
  }
</style>
