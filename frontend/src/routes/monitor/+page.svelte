<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // -----------------------------------------------------------------------
  // Types
  // -----------------------------------------------------------------------
  interface CoreMetric { core: number; percent: number; }
  interface MemoryMetric { total: number; used: number; free: number; buffers: number; cached: number; available: number; percent: number; swap_total: number; swap_used: number; swap_free: number; swap_percent: number; }
  interface DiskMetric { device: string; fstype: string; total: number; used: number; available: number; percent: number; mountpoint: string; health: string; }
  interface DiskIoMetric { total_read_bytes_per_sec: number; total_write_bytes_per_sec: number; devices: { name: string; read_bytes_per_sec: number; write_bytes_per_sec: number }[]; }
  interface NetworkMetric { name: string; state: string; rx_bytes: number; tx_bytes: number; rx_packets: number; tx_packets: number; rx_errors: number; tx_errors: number; rx_drops: number; tx_drops: number; rx_bps: number; tx_bps: number; rx_pps: number; tx_pps: number; }
  interface VppMetric { available: boolean; version: string; nat_sessions: number; pppoe_active: number; pppoe_discovery: number; pppoe_total: number; memory_total_mb: number; memory_used_mb: number; memory_percent: number; errors_total: number; worker_threads: number; packet_rate_rx: number; packet_rate_tx: number; }
  interface ProcessMetric { name: string; running: boolean; pid: number | null; mem_rss: number; cpu_percent: number; }
  interface TemperatureMetric { sensor: string; temp_celsius: number; }
  interface LoadAverage { load_1m: number; load_5m: number; load_15m: number; }
  interface SystemMetrics {
    timestamp: string; cpu_percent: number; cpu_count: number; cpu_cores: CoreMetric[];
    memory: MemoryMetric; disk_usage: DiskMetric[]; disk_io: DiskIoMetric;
    network: NetworkMetric[]; vpp: VppMetric; processes: ProcessMetric[];
    temperatures: TemperatureMetric[]; load_average: LoadAverage; uptime: number;
  }
  interface Alert {
    id: number; severity: string; category: string; message: string;
    value: string; threshold: string; first_seen: string; last_seen: string;
    count: number; acknowledged: boolean;
  }
  interface ConntrackStatus { active_sessions: number; max_sessions: number; tcp_established: number; tcp_syn_sent: number; tcp_time_wait: number; udp: number; icmp: number; other: number; }
  interface VppPerformance {
    packet_rate?: { rx: number; tx: number };
    threads?: { name: string; cpu_time: number; state: string }[];
    errors?: { total: number; counters: { count: number; node: string; reason: string }[] };
  }

  // -----------------------------------------------------------------------
  // State
  // -----------------------------------------------------------------------
  let metrics: SystemMetrics | null = null;
  let healthScore = 0;
  let alerts: Alert[] = [];
  let history: SystemMetrics[] = [];
  let historyHours = 1;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let historyInterval: ReturnType<typeof setInterval> | null = null;
  let activeTab: 'overview' | 'cpu' | 'memory' | 'network' | 'disk' | 'processes' | 'vpp' | 'alerts' = 'overview';

  // Network sub-data
  let conntrack: ConntrackStatus | null = null;
  let networkLatency: { target: string; latency_ms: number | null; timestamp: string } | null = null;
  let vppPerf: VppPerformance | null = null;
  let latencyTarget = '8.8.8.8';

  // -----------------------------------------------------------------------
  // Helpers
  // -----------------------------------------------------------------------
  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(2) + ' ' + sizes[i];
  }

  function formatBitsPerSec(bps: number): string {
    if (bps >= 1e9) return (bps / 1e9).toFixed(2) + ' Gbps';
    if (bps >= 1e6) return (bps / 1e6).toFixed(2) + ' Mbps';
    if (bps >= 1e3) return (bps / 1e3).toFixed(2) + ' Kbps';
    return bps.toFixed(0) + ' bps';
  }

  function formatUptime(seconds: number): string {
    const d = Math.floor(seconds / 86400);
    const h = Math.floor((seconds % 86400) / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function healthColor(score: number): string {
    if (score >= 80) return '#00ff88';
    if (score >= 60) return '#ffaa00';
    if (score >= 40) return '#ff8800';
    return '#ff4444';
  }

  function healthLabel(score: number): string {
    if (score >= 80) return 'Healthy';
    if (score >= 60) return 'Fair';
    if (score >= 40) return 'Degraded';
    return 'Critical';
  }

  function barColor(percent: number): string {
    if (percent > 90) return '#ff4444';
    if (percent > 75) return '#ff8800';
    if (percent > 50) return '#ffaa00';
    return '#00ff88';
  }

  function severityColor(severity: string): string {
    return severity === 'critical' ? '#ff4444' : '#ffaa00';
  }

  function formatTime(iso: string): string {
    try { return new Date(iso).toLocaleTimeString(); } catch { return iso; }
  }

  function healthBadgeColor(health: string): string {
    switch (health) {
      case 'healthy': return '#00ff88';
      case 'failing': return '#ff4444';
      case 'no-smart': return '#888';
      case 'unknown': return '#666';
      default: return '#666';
    }
  }

  // -----------------------------------------------------------------------
  // Chart helpers (pure CSS bar charts using history data)
  // -----------------------------------------------------------------------
  function getHistoryValues(key: string, subkey?: string): number[] {
    return history.map(h => {
      const obj: any = h;
      if (subkey) return obj[key]?.[subkey] ?? 0;
      return obj[key] ?? 0;
    }).reverse();
  }

  function buildBarData(values: number[], maxBars = 60): { height: number; label: string }[] {
    if (values.length === 0) return [];
    // Downsample to maxBars
    const step = Math.max(1, Math.floor(values.length / maxBars));
    const sampled: number[] = [];
    for (let i = 0; i < values.length; i += step) {
      sampled.push(values[i]);
    }
    const maxVal = Math.max(...sampled, 1);
    return sampled.map(v => ({
      height: (v / maxVal) * 100,
      label: v.toFixed(1),
    }));
  }

  // -----------------------------------------------------------------------
  // API calls
  // -----------------------------------------------------------------------
  async function fetchMetrics() {
    try {
      const res = await fetch('/api/monitor/metrics');
      const data = await res.json();
      if (data.status === 'ok' && data.metrics) {
        metrics = data.metrics;
        healthScore = data.health_score ?? 0;
      }
    } catch (e) {
      console.error('Failed to fetch metrics', e);
    }
  }

  async function fetchAlerts() {
    try {
      const res = await fetch('/api/monitor/alerts');
      const data = await res.json();
      if (data.status === 'ok') {
        alerts = data.alerts ?? [];
      }
    } catch (e) {
      console.error('Failed to fetch alerts', e);
    }
  }

  async function fetchHistory() {
    try {
      const res = await fetch(`/api/monitor/history?hours=${historyHours}&limit=300`);
      const data = await res.json();
      if (data.status === 'ok') {
        history = data.history ?? [];
      }
    } catch (e) {
      console.error('Failed to fetch history', e);
    }
  }

  async function ackAlert(alertId: number) {
    try {
      await fetch('/api/monitor/alerts/ack', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ alert_id: alertId, acked_by: 'admin' }),
      });
      await fetchAlerts();
    } catch (e) {
      console.error('Failed to ack alert', e);
    }
  }

  async function fetchConntrack() {
    try {
      const res = await fetch('/api/conntrack/status');
      const data = await res.json();
      if (data.status === 'ok') {
        conntrack = data;
      }
    } catch (e) {
      console.error('Failed to fetch conntrack', e);
    }
  }

  async function fetchVppPerf() {
    try {
      const res = await fetch('/api/system/vpp-performance');
      const data = await res.json();
      if (data.performance) {
        vppPerf = data.performance;
      }
    } catch (e) {
      console.error('Failed to fetch VPP performance', e);
    }
  }

  async function measureLatency() {
    try {
      const res = await fetch('/api/diag/ping', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ host: latencyTarget, count: 3 }),
      });
      const data = await res.json();
      if (data.status === 'ok' && data.avg_ms !== undefined) {
        networkLatency = { target: latencyTarget, latency_ms: data.avg_ms, timestamp: new Date().toISOString() };
      } else if (data.average_latency !== undefined) {
        networkLatency = { target: latencyTarget, latency_ms: data.average_latency, timestamp: new Date().toISOString() };
      } else {
        networkLatency = { target: latencyTarget, latency_ms: null, timestamp: new Date().toISOString() };
      }
    } catch (e) {
      console.error('Failed to measure latency', e);
      networkLatency = { target: latencyTarget, latency_ms: null, timestamp: new Date().toISOString() };
    }
  }

  // -----------------------------------------------------------------------
  // Lifecycle
  // -----------------------------------------------------------------------
  onMount(() => {
    fetchMetrics();
    fetchAlerts();
    fetchHistory();
    fetchConntrack();
    fetchVppPerf();
    measureLatency();
    refreshInterval = setInterval(() => {
      fetchMetrics();
      fetchAlerts();
      fetchConntrack();
      fetchVppPerf();
    }, 5000);
    historyInterval = setInterval(fetchHistory, 30000);
    // Measure latency every 30 seconds
    setInterval(measureLatency, 30000);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
    if (historyInterval) clearInterval(historyInterval);
  });
</script>

<svelte:head>
  <title>VectorOS - System Monitor</title>
</svelte:head>

<div class="monitor">
  <div class="header-row">
    <h1>System Monitor</h1>
    {#if metrics}
      <div class="health-badge" style="border-color: {healthColor(healthScore)}">
        <span class="health-score" style="color: {healthColor(healthScore)}">{healthScore}</span>
        <span class="health-label">{healthLabel(healthScore)}</span>
      </div>
    {/if}
  </div>

  {#if !metrics}
    <div class="loading">Collecting metrics... The first data point arrives within 5 seconds.</div>
  {:else}
    <!-- Tab Navigation -->
    <div class="tabs">
      {#each [
        { id: 'overview', label: 'Overview' },
        { id: 'cpu', label: 'CPU' },
        { id: 'memory', label: 'Memory' },
        { id: 'network', label: 'Network' },
        { id: 'disk', label: 'Disk' },
        { id: 'processes', label: 'Processes' },
        { id: 'vpp', label: 'VPP' },
        { id: 'alerts', label: `Alerts (${alerts.length})` },
      ] as tab}
        <button
          class="tab"
          class:active={activeTab === tab.id}
          on:click={() => activeTab = tab.id}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <!-- Overview Tab -->
    {#if activeTab === 'overview'}
      <div class="overview-grid">
        <!-- CPU Card -->
        <div class="card">
          <h3>CPU</h3>
          <div class="big-value" style="color: {barColor(metrics.cpu_percent)}">{metrics.cpu_percent.toFixed(1)}%</div>
          <div class="sub-text">{metrics.cpu_count} cores</div>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {metrics.cpu_percent}%; background: {barColor(metrics.cpu_percent)}"></div>
          </div>
          <div class="sub-text">Load: {metrics.load_average.load_1m} / {metrics.load_average.load_5m} / {metrics.load_average.load_15m}</div>
        </div>

        <!-- Memory Card -->
        <div class="card">
          <h3>Memory</h3>
          <div class="big-value" style="color: {barColor(metrics.memory.percent)}">{metrics.memory.percent.toFixed(1)}%</div>
          <div class="sub-text">{formatBytes(metrics.memory.used)} / {formatBytes(metrics.memory.total)}</div>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {metrics.memory.percent}%; background: {barColor(metrics.memory.percent)}"></div>
          </div>
          <div class="sub-text">Cached: {formatBytes(metrics.memory.cached)}</div>
          {#if metrics.memory.swap_total > 0}
            <div class="sub-text">Swap: {formatBytes(metrics.memory.swap_used)} / {formatBytes(metrics.memory.swap_total)} ({metrics.memory.swap_percent.toFixed(1)}%)</div>
          {/if}
        </div>

        <!-- Disk Card -->
        <div class="card">
          <h3>Disk</h3>
          {#each metrics.disk_usage.slice(0, 2) as disk}
            <div class="disk-row">
              <span class="disk-mount">{disk.mountpoint}</span>
              <span class="disk-value" style="color: {barColor(disk.percent)}">{disk.percent.toFixed(1)}%</span>
            </div>
            <div class="progress-bar small">
              <div class="progress-fill" style="width: {disk.percent}%; background: {barColor(disk.percent)}"></div>
            </div>
            <div class="sub-text">{formatBytes(disk.used)} / {formatBytes(disk.total)}</div>
          {/each}
        </div>

        <!-- Network Card -->
        <div class="card">
          <h3>Network I/O</h3>
          {#each metrics.network.slice(0, 3) as iface}
            <div class="net-row">
              <span class="net-name">{iface.name}</span>
              <span class="net-state" class:up={iface.state === 'up'}>{iface.state}</span>
            </div>
            <div class="net-throughput">
              <span class="rx">RX {formatBitsPerSec(iface.rx_bps)}</span>
              <span class="tx">TX {formatBitsPerSec(iface.tx_bps)}</span>
            </div>
          {/each}
          {#if networkLatency}
            <div class="net-latency">
              Latency to {networkLatency.target}: {networkLatency.latency_ms !== null ? networkLatency.latency_ms.toFixed(1) + ' ms' : 'N/A'}
            </div>
          {/if}
        </div>

        <!-- VPP Card -->
        <div class="card">
          <h3>VPP</h3>
          {#if metrics.vpp.available}
            <div class="big-value" style="color: #00ff88">Active</div>
            <div class="sub-text">v{metrics.vpp.version}</div>
            <div class="vpp-detail">NAT Sessions: {metrics.vpp.nat_sessions}</div>
            <div class="vpp-detail">PPPoE: {metrics.vpp.pppoe_active} active / {metrics.vpp.pppoe_total} total</div>
            <div class="vpp-detail">Memory: {metrics.vpp.memory_used_mb.toFixed(1)} / {metrics.vpp.memory_total_mb.toFixed(1)} MB</div>
            {#if metrics.vpp.worker_threads > 0}
              <div class="vpp-detail">Workers: {metrics.vpp.worker_threads}</div>
            {/if}
          {:else}
            <div class="big-value" style="color: #ff4444">Unavailable</div>
          {/if}
        </div>

        <!-- System Card -->
        <div class="card">
          <h3>System</h3>
          <div class="sys-detail">Uptime: {formatUptime(metrics.uptime)}</div>
          <div class="sys-detail">Timestamp: {formatTime(metrics.timestamp)}</div>
          {#if metrics.temperatures.length > 0}
            {#each metrics.temperatures as temp}
              <div class="sys-detail">{temp.sensor}: {temp.temp_celsius.toFixed(1)}C</div>
            {/each}
          {/if}
          {#if conntrack}
            <div class="sys-detail">ConnTrack: {conntrack.active_sessions} / {conntrack.max_sessions} sessions</div>
          {/if}
        </div>
      </div>

      <!-- CPU Cores Bar Chart -->
      <div class="card wide">
        <h3>CPU Usage Per Core</h3>
        <div class="core-chart">
          {#each metrics.cpu_cores as core}
            <div class="core-bar-wrap">
              <div class="core-bar" style="height: {core.percent}%; background: {barColor(core.percent)}"></div>
              <span class="core-label">C{core.core}</span>
              <span class="core-value">{core.percent.toFixed(0)}%</span>
            </div>
          {/each}
        </div>
      </div>

      <!-- Processes -->
      <div class="card wide">
        <h3>Process Status</h3>
        <div class="process-grid">
          {#each metrics.processes as proc}
            <div class="proc-item" class:running={proc.running} class:stopped={!proc.running}>
              <span class="proc-dot" style="background: {proc.running ? '#00ff88' : '#ff4444'}"></span>
              <span class="proc-name">{proc.name}</span>
              {#if proc.running}
                <span class="proc-pid">PID {proc.pid}</span>
                <span class="proc-mem">{formatBytes(proc.mem_rss)}</span>
              {:else}
                <span class="proc-pid" style="color: #ff4444">stopped</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- CPU Tab -->
    {#if activeTab === 'cpu'}
      <div class="card wide">
        <h3>CPU Usage Per Core</h3>
        <div class="core-chart tall">
          {#each metrics.cpu_cores as core}
            <div class="core-bar-wrap">
              <div class="core-bar" style="height: {core.percent}%; background: {barColor(core.percent)}"></div>
              <span class="core-label">C{core.core}</span>
              <span class="core-value">{core.percent.toFixed(1)}%</span>
            </div>
          {/each}
        </div>
      </div>

      <div class="overview-grid">
        <div class="card">
          <h3>Load Average</h3>
          <div class="load-grid">
            <div class="load-item">
              <span class="load-value">{metrics.load_average.load_1m.toFixed(2)}</span>
              <span class="load-label">1 min</span>
            </div>
            <div class="load-item">
              <span class="load-value">{metrics.load_average.load_5m.toFixed(2)}</span>
              <span class="load-label">5 min</span>
            </div>
            <div class="load-item">
              <span class="load-value">{metrics.load_average.load_15m.toFixed(2)}</span>
              <span class="load-label">15 min</span>
            </div>
          </div>
        </div>

        {#if metrics.temperatures.length > 0}
          <div class="card">
            <h3>CPU Temperature</h3>
            {#each metrics.temperatures as temp}
              <div class="temp-row">
                <span class="temp-name">{temp.sensor}</span>
                <span class="temp-value" style="color: {temp.temp_celsius > 80 ? '#ff4444' : temp.temp_celsius > 60 ? '#ffaa00' : '#00ff88'}">{temp.temp_celsius.toFixed(1)}C</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      {#if history.length > 0}
        <div class="card wide">
          <h3>CPU History ({historyHours}h)</h3>
          <div class="mini-chart">
            {#each buildBarData(getHistoryValues('cpu_percent')) as bar}
              <div class="mini-bar" style="height: {bar.height}%; background: {barColor(parseFloat(bar.label))}" title="{bar.label}%"></div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}

    <!-- Memory Tab -->
    {#if activeTab === 'memory'}
      <div class="overview-grid">
        <div class="card">
          <h3>Physical Memory</h3>
          <div class="big-value" style="color: {barColor(metrics.memory.percent)}">{metrics.memory.percent.toFixed(1)}%</div>
          <div class="sub-text">{formatBytes(metrics.memory.used)} / {formatBytes(metrics.memory.total)}</div>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {metrics.memory.percent}%; background: {barColor(metrics.memory.percent)}"></div>
          </div>
        </div>

        {#if metrics.memory.swap_total > 0}
          <div class="card">
            <h3>Swap</h3>
            <div class="big-value" style="color: {barColor(metrics.memory.swap_percent)}">{metrics.memory.swap_percent.toFixed(1)}%</div>
            <div class="sub-text">{formatBytes(metrics.memory.swap_used)} / {formatBytes(metrics.memory.swap_total)}</div>
            <div class="progress-bar">
              <div class="progress-fill" style="width: {metrics.memory.swap_percent}%; background: {barColor(metrics.memory.swap_percent)}"></div>
            </div>
          </div>
        {:else}
          <div class="card">
            <h3>Swap</h3>
            <div class="big-value" style="color: #666">Not configured</div>
            <div class="sub-text">No swap space allocated</div>
          </div>
        {/if}
      </div>

      <!-- Memory Breakdown Chart -->
      <div class="card wide">
        <h3>Memory Breakdown</h3>
        <div class="mem-breakdown">
          <div class="mem-bar-container">
            <div class="mem-bar">
              <div class="mem-segment used" style="width: {(metrics.memory.used / metrics.memory.total * 100)}%" title="Used: {formatBytes(metrics.memory.used)}"></div>
              <div class="mem-segment cached" style="width: {(metrics.memory.cached / metrics.memory.total * 100)}%" title="Cached: {formatBytes(metrics.memory.cached)}"></div>
              <div class="mem-segment buffers" style="width: {(metrics.memory.buffers / metrics.memory.total * 100)}%" title="Buffers: {formatBytes(metrics.memory.buffers)}"></div>
              <div class="mem-segment free" style="width: {(metrics.memory.free / metrics.memory.total * 100)}%" title="Free: {formatBytes(metrics.memory.free)}"></div>
            </div>
          </div>
          <div class="mem-legend">
            <div class="legend-item"><span class="legend-dot" style="background: #ff6b6b"></span>Used: {formatBytes(metrics.memory.used)}</div>
            <div class="legend-item"><span class="legend-dot" style="background: #ffd93d"></span>Cached: {formatBytes(metrics.memory.cached)}</div>
            <div class="legend-item"><span class="legend-dot" style="background: #6bcb77"></span>Buffers: {formatBytes(metrics.memory.buffers)}</div>
            <div class="legend-item"><span class="legend-dot" style="background: #4d96ff"></span>Free: {formatBytes(metrics.memory.free)}</div>
            <div class="legend-item"><span class="legend-dot" style="background: #888"></span>Available: {formatBytes(metrics.memory.available)}</div>
          </div>
        </div>
      </div>

      {#if history.length > 0}
        <div class="card wide">
          <h3>Memory Usage History ({historyHours}h)</h3>
          <div class="mini-chart">
            {#each buildBarData(getHistoryValues('memory', 'percent')) as bar}
              <div class="mini-bar" style="height: {bar.height}%; background: {barColor(parseFloat(bar.label))}" title="{bar.label}%"></div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}

    <!-- Network Tab -->
    {#if activeTab === 'network'}
      <div class="card wide">
        <h3>Network Interfaces</h3>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Interface</th>
                <th>State</th>
                <th>RX Rate</th>
                <th>TX Rate</th>
                <th>RX Packets</th>
                <th>TX Packets</th>
                <th>RX pps</th>
                <th>TX pps</th>
                <th>Errors</th>
                <th>Drops</th>
              </tr>
            </thead>
            <tbody>
              {#each metrics.network as iface}
                <tr>
                  <td class="iface-name">{iface.name}</td>
                  <td><span class="state-badge" class:up={iface.state === 'up'}>{iface.state}</span></td>
                  <td class="rx">{formatBitsPerSec(iface.rx_bps)}</td>
                  <td class="tx">{formatBitsPerSec(iface.tx_bps)}</td>
                  <td>{iface.rx_packets.toLocaleString()}</td>
                  <td>{iface.tx_packets.toLocaleString()}</td>
                  <td>{iface.rx_pps.toLocaleString()}</td>
                  <td>{iface.tx_pps.toLocaleString()}</td>
                  <td class:error={iface.rx_errors + iface.tx_errors > 0}>{iface.rx_errors + iface.tx_errors}</td>
                  <td class:error={iface.rx_drops + iface.tx_drops > 0}>{iface.rx_drops + iface.tx_drops}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>

      <!-- Total Bandwidth -->
      <div class="overview-grid">
        <div class="card">
          <h3>Total RX Bandwidth</h3>
          <div class="big-value rx">{formatBitsPerSec(metrics.network.reduce((sum, i) => sum + i.rx_bps, 0))}</div>
        </div>
        <div class="card">
          <h3>Total TX Bandwidth</h3>
          <div class="big-value tx">{formatBitsPerSec(metrics.network.reduce((sum, i) => sum + i.tx_bps, 0))}</div>
        </div>
        <div class="card">
          <h3>Total RX Packets</h3>
          <div class="big-value">{metrics.network.reduce((sum, i) => sum + i.rx_packets, 0).toLocaleString()}</div>
        </div>
        <div class="card">
          <h3>Total TX Packets</h3>
          <div class="big-value">{metrics.network.reduce((sum, i) => sum + i.tx_packets, 0).toLocaleString()}</div>
        </div>
      </div>

      <!-- ConnTrack -->
      {#if conntrack}
        <div class="card wide">
          <h3>Connection Tracking</h3>
          <div class="overview-grid compact">
            <div class="stat-item">
              <span class="stat-value">{conntrack.active_sessions}</span>
              <span class="stat-label">Active Sessions</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.max_sessions}</span>
              <span class="stat-label">Max Sessions</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.tcp_established}</span>
              <span class="stat-label">TCP Established</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.tcp_syn_sent}</span>
              <span class="stat-label">TCP SYN Sent</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.tcp_time_wait}</span>
              <span class="stat-label">TCP Time Wait</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.udp}</span>
              <span class="stat-label">UDP</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.icmp}</span>
              <span class="stat-label">ICMP</span>
            </div>
            <div class="stat-item">
              <span class="stat-value">{conntrack.other}</span>
              <span class="stat-label">Other</span>
            </div>
          </div>
          <div class="progress-bar" style="margin-top: 0.75rem">
            <div class="progress-fill" style="width: {(conntrack.active_sessions / Math.max(conntrack.max_sessions, 1)) * 100}%; background: {barColor((conntrack.active_sessions / Math.max(conntrack.max_sessions, 1)) * 100)}"></div>
          </div>
          <div class="sub-text">{((conntrack.active_sessions / Math.max(conntrack.max_sessions, 1)) * 100).toFixed(1)}% of capacity</div>
        </div>
      {/if}

      <!-- Network Latency -->
      <div class="card wide">
        <h3>Network Latency</h3>
        <div class="latency-section">
          <div class="latency-input-row">
            <input type="text" bind:value={latencyTarget} placeholder="Target IP/hostname" class="latency-input" />
            <button class="btn-ping" on:click={measureLatency}>Ping</button>
          </div>
          {#if networkLatency}
            <div class="latency-result">
              <span class="latency-target">{networkLatency.target}</span>
              {#if networkLatency.latency_ms !== null}
                <span class="latency-value" style="color: {networkLatency.latency_ms < 30 ? '#00ff88' : networkLatency.latency_ms < 100 ? '#ffaa00' : '#ff4444'}">{networkLatency.latency_ms.toFixed(1)} ms</span>
              {:else}
                <span class="latency-value" style="color: #ff4444">No response</span>
              {/if}
              <span class="latency-time">{formatTime(networkLatency.timestamp)}</span>
            </div>
          {/if}
        </div>
      </div>

      {#if history.length > 0}
        <div class="card wide">
          <h3>Network History ({historyHours}h)</h3>
          <div class="history-labels">
            <span class="rx">RX (incoming)</span>
            <span class="tx">TX (outgoing)</span>
          </div>
          <div class="mini-chart dual">
            {#each buildBarData(getHistoryValues('network', '0')) as bar, i}
              <div class="mini-bar-group">
                <div class="mini-bar rx" style="height: {bar.height}%" title="RX: {bar.label}"></div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}

    <!-- Disk Tab -->
    {#if activeTab === 'disk'}
      <div class="card wide">
        <h3>Disk Usage</h3>
        {#each metrics.disk_usage as disk}
          <div class="disk-detail-row">
            <div class="disk-info">
              <span class="disk-device">{disk.device}</span>
              <span class="disk-mount">{disk.mountpoint}</span>
              <span class="disk-type">{disk.fstype}</span>
              <span class="disk-health" style="color: {healthBadgeColor(disk.health)}">{disk.health}</span>
            </div>
            <div class="disk-bar-section">
              <div class="progress-bar">
                <div class="progress-fill" style="width: {disk.percent}%; background: {barColor(disk.percent)}"></div>
              </div>
              <span class="disk-pct" style="color: {barColor(disk.percent)}">{disk.percent.toFixed(1)}%</span>
            </div>
            <div class="disk-sizes">
              {formatBytes(disk.used)} / {formatBytes(disk.total)} ({formatBytes(disk.available)} free)
            </div>
          </div>
        {/each}
      </div>
      <div class="card wide">
        <h3>Disk I/O</h3>
        <div class="disk-io-summary">
          <span>Total Read: {formatBytes(metrics.disk_io.total_read_bytes_per_sec)}/s</span>
          <span>Total Write: {formatBytes(metrics.disk_io.total_write_bytes_per_sec)}/s</span>
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Device</th>
                <th>Read Rate</th>
                <th>Write Rate</th>
                <th>I/O Bar</th>
              </tr>
            </thead>
            <tbody>
              {#each metrics.disk_io.devices as dev}
                {@const totalIO = dev.read_bytes_per_sec + dev.write_bytes_per_sec}
                {@const maxIO = Math.max(...metrics.disk_io.devices.map(d => d.read_bytes_per_sec + d.write_bytes_per_sec), 1)}
                <tr>
                  <td class="iface-name">{dev.name}</td>
                  <td class="rx">{formatBytes(dev.read_bytes_per_sec)}/s</td>
                  <td class="tx">{formatBytes(dev.write_bytes_per_sec)}/s</td>
                  <td>
                    <div class="disk-io-bar">
                      <div class="disk-io-read" style="width: {(dev.read_bytes_per_sec / maxIO) * 100}%"></div>
                      <div class="disk-io-write" style="width: {(dev.write_bytes_per_sec / maxIO) * 100}%"></div>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="io-legend">
          <span class="legend-item"><span class="legend-dot rx"></span>Read</span>
          <span class="legend-item"><span class="legend-dot tx"></span>Write</span>
        </div>
      </div>
    {/if}

    <!-- Processes Tab -->
    {#if activeTab === 'processes'}
      <div class="card wide">
        <h3>Process Monitoring</h3>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th>Status</th>
                <th>PID</th>
                <th>Memory (RSS)</th>
                <th>CPU %</th>
              </tr>
            </thead>
            <tbody>
              {#each metrics.processes as proc}
                <tr class:row-stopped={!proc.running}>
                  <td class="proc-name-cell">{proc.name}</td>
                  <td>
                    <span class="state-badge" class:up={proc.running} class:down={!proc.running}>
                      {proc.running ? 'running' : 'stopped'}
                    </span>
                  </td>
                  <td>{proc.pid ?? '-'}</td>
                  <td>{formatBytes(proc.mem_rss)}</td>
                  <td>{proc.cpu_percent.toFixed(1)}%</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}

    <!-- VPP Tab -->
    {#if activeTab === 'vpp'}
      {#if metrics.vpp.available}
        <div class="overview-grid">
          <div class="card">
            <h3>VPP Version</h3>
            <div class="big-value" style="color: #00ff88">{metrics.vpp.version}</div>
          </div>
          <div class="card">
            <h3>NAT Sessions</h3>
            <div class="big-value">{metrics.vpp.nat_sessions}</div>
          </div>
          <div class="card">
            <h3>PPPoE Sessions</h3>
            <div class="big-value">{metrics.vpp.pppoe_active} <span class="sub-text">active</span></div>
            <div class="sub-text">{metrics.vpp.pppoe_discovery} discovering / {metrics.vpp.pppoe_total} total</div>
          </div>
          <div class="card">
            <h3>VPP Memory</h3>
            <div class="big-value" style="color: {barColor(metrics.vpp.memory_percent)}">{metrics.vpp.memory_percent.toFixed(1)}%</div>
            <div class="sub-text">{metrics.vpp.memory_used_mb.toFixed(1)} / {metrics.vpp.memory_total_mb.toFixed(1)} MB</div>
            <div class="progress-bar">
              <div class="progress-fill" style="width: {metrics.vpp.memory_percent}%; background: {barColor(metrics.vpp.memory_percent)}"></div>
            </div>
          </div>
        </div>

        <!-- Worker Threads -->
        {#if metrics.vpp.worker_threads > 0 || (vppPerf?.threads && vppPerf.threads.length > 0)}
          <div class="card wide">
            <h3>Worker Threads</h3>
            <div class="sub-text" style="margin-bottom: 0.75rem">Total worker threads: {metrics.vpp.worker_threads || vppPerf?.threads?.length || 0}</div>
            {#if vppPerf?.threads && vppPerf.threads.length > 0}
              <div class="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Thread</th>
                      <th>State</th>
                      <th>CPU Time</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each vppPerf.threads as thread}
                      <tr>
                        <td class="iface-name">{thread.name}</td>
                        <td><span class="state-badge" class:up={thread.state === 'running'}>{thread.state}</span></td>
                        <td>{thread.cpu_time.toFixed(1)}%</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Packet Processing Rate -->
        <div class="card wide">
          <h3>Packet Processing Rate</h3>
          <div class="overview-grid compact">
            <div class="stat-item">
              <span class="stat-value rx">{metrics.vpp.packet_rate_rx.toLocaleString()}</span>
              <span class="stat-label">Total RX Packets</span>
            </div>
            <div class="stat-item">
              <span class="stat-value tx">{metrics.vpp.packet_rate_tx.toLocaleString()}</span>
              <span class="stat-label">Total TX Packets</span>
            </div>
            {#if vppPerf?.packet_rate}
              <div class="stat-item">
                <span class="stat-value">{vppPerf.packet_rate.rx.toLocaleString()}</span>
                <span class="stat-label">Current RX pps</span>
              </div>
              <div class="stat-item">
                <span class="stat-value">{vppPerf.packet_rate.tx.toLocaleString()}</span>
                <span class="stat-label">Current TX pps</span>
              </div>
            {/if}
          </div>
        </div>

        <!-- VPP Errors -->
        {#if metrics.vpp.errors_total > 0}
          <div class="card wide warning-card">
            <h3>VPP Errors ({metrics.vpp.errors_total})</h3>
            {#if vppPerf?.errors?.counters && vppPerf.errors.counters.length > 0}
              <div class="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Count</th>
                      <th>Node</th>
                      <th>Reason</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each vppPerf.errors.counters.slice(0, 15) as counter}
                      <tr>
                        <td class="error">{counter.count.toLocaleString()}</td>
                        <td>{counter.node}</td>
                        <td>{counter.reason}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {:else}
              <div class="sub-text">Error counters detected in the VPP data plane.</div>
            {/if}
          </div>
        {/if}
      {:else}
        <div class="card wide">
          <h3>VPP</h3>
          <div class="big-value" style="color: #ff4444">VPP is not responding</div>
          <div class="sub-text">The VPP process may have crashed or is not installed.</div>
        </div>
      {/if}
    {/if}

    <!-- Alerts Tab -->
    {#if activeTab === 'alerts'}
      <div class="card wide">
        <div class="alert-header">
          <h3>Active Alerts</h3>
          <button class="btn-refresh" on:click={fetchAlerts}>Refresh</button>
        </div>
        {#if alerts.length === 0}
          <div class="no-alerts">No active alerts. System is operating normally.</div>
        {:else}
          {#each alerts as alert}
            <div class="alert-row" style="border-left-color: {severityColor(alert.severity)}">
              <div class="alert-main">
                <span class="alert-severity" style="color: {severityColor(alert.severity)}">{alert.severity.toUpperCase()}</span>
                <span class="alert-category">{alert.category}</span>
                <span class="alert-message">{alert.message}</span>
              </div>
              <div class="alert-meta">
                <span>Value: {alert.value} (threshold: {alert.threshold})</span>
                <span>First: {formatTime(alert.first_seen)}</span>
                <span>Last: {formatTime(alert.last_seen)}</span>
                <span>Count: {alert.count}</span>
                <button class="btn-ack" on:click={() => ackAlert(alert.id)}>Acknowledge</button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    {/if}

    <!-- History Time Range Selector -->
    <div class="history-controls">
      <span>History range:</span>
      {#each [1, 6, 24, 72] as hours}
        <button
          class="range-btn"
          class:active={historyHours === hours}
          on:click={() => { historyHours = hours; fetchHistory(); }}
        >
          {hours}h
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .monitor { max-width: 1400px; }

  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1.5rem;
  }

  h1 { color: #00ff88; margin: 0; }

  .health-badge {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1.25rem;
    background: #1a1a2e;
    border: 2px solid;
    border-radius: 0.75rem;
  }

  .health-score { font-size: 2rem; font-weight: bold; }
  .health-label { font-size: 0.9rem; color: #888; text-transform: uppercase; }

  .loading {
    text-align: center;
    padding: 4rem;
    color: #888;
    font-size: 1.1rem;
  }

  /* Tabs */
  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 1.5rem;
    border-bottom: 2px solid #333;
    padding-bottom: 0;
    overflow-x: auto;
  }

  .tab {
    padding: 0.75rem 1.25rem;
    background: none;
    border: none;
    color: #888;
    cursor: pointer;
    font-size: 0.9rem;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    transition: all 0.2s;
    white-space: nowrap;
  }

  .tab:hover { color: #ccc; }
  .tab.active { color: #00ff88; border-bottom-color: #00ff88; }

  /* Overview Grid */
  .overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .overview-grid.compact {
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  }

  .card {
    background: #1a1a2e;
    padding: 1.25rem;
    border-radius: 0.75rem;
  }

  .card.wide { margin-bottom: 1.5rem; }
  .card h3 { margin: 0 0 0.75rem 0; font-size: 0.95rem; color: #888; }

  .big-value { font-size: 2rem; font-weight: bold; margin: 0.25rem 0; }
  .sub-text { font-size: 0.8rem; color: #888; margin: 0.25rem 0; }

  .progress-bar {
    height: 6px;
    background: #333;
    border-radius: 3px;
    overflow: hidden;
    margin: 0.5rem 0;
  }

  .progress-bar.small { height: 4px; }

  .progress-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.5s ease;
  }

  /* Disk */
  .disk-row { display: flex; justify-content: space-between; margin-top: 0.5rem; }
  .disk-mount { color: #ccc; font-size: 0.9rem; }
  .disk-value { font-weight: bold; }

  /* Network */
  .net-row { display: flex; justify-content: space-between; align-items: center; margin-top: 0.5rem; }
  .net-name { color: #ccc; font-weight: 600; }
  .net-state { font-size: 0.8rem; text-transform: uppercase; color: #888; }
  .net-state.up { color: #00ff88; }
  .net-throughput { display: flex; gap: 1rem; font-size: 0.8rem; color: #aaa; margin-top: 0.25rem; }
  .net-latency { font-size: 0.8rem; color: #aaa; margin-top: 0.5rem; }
  .rx { color: #4fc3f7; }
  .tx { color: #81c784; }

  /* VPP */
  .vpp-detail { font-size: 0.85rem; color: #aaa; margin: 0.2rem 0; }

  /* System */
  .sys-detail { font-size: 0.85rem; color: #aaa; margin: 0.25rem 0; }

  /* CPU Core Chart */
  .core-chart {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    height: 120px;
    padding: 0.5rem 0;
  }

  .core-chart.tall { height: 200px; }

  .core-bar-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    height: 100%;
    justify-content: flex-end;
  }

  .core-bar {
    width: 100%;
    min-height: 2px;
    border-radius: 2px 2px 0 0;
    transition: height 0.5s ease;
  }

  .core-label { font-size: 0.65rem; color: #666; margin-top: 4px; }
  .core-value { font-size: 0.65rem; color: #888; }

  /* Load Average */
  .load-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
    text-align: center;
  }

  .load-item { display: flex; flex-direction: column; gap: 0.25rem; }
  .load-value { font-size: 1.5rem; font-weight: bold; color: #00ff88; }
  .load-label { font-size: 0.8rem; color: #888; }

  /* Temperature */
  .temp-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.4rem 0;
    border-bottom: 1px solid #222;
  }

  .temp-name { color: #ccc; font-size: 0.85rem; }
  .temp-value { font-weight: bold; font-size: 0.9rem; }

  /* Memory Breakdown */
  .mem-breakdown {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .mem-bar-container { width: 100%; }

  .mem-bar {
    display: flex;
    height: 24px;
    border-radius: 4px;
    overflow: hidden;
    background: #333;
  }

  .mem-segment {
    height: 100%;
    transition: width 0.5s ease;
  }

  .mem-segment.used { background: #ff6b6b; }
  .mem-segment.cached { background: #ffd93d; }
  .mem-segment.buffers { background: #6bcb77; }
  .mem-segment.free { background: #4d96ff; }

  .mem-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    font-size: 0.85rem;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: #aaa;
  }

  .legend-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    display: inline-block;
  }

  .legend-dot.rx { background: #4fc3f7; }
  .legend-dot.tx { background: #81c784; }

  /* Process grid */
  .process-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 0.75rem;
  }

  .proc-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .proc-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .proc-name { color: #e0e0e0; font-weight: 600; }
  .proc-pid { font-size: 0.8rem; color: #888; }
  .proc-mem { font-size: 0.8rem; color: #888; margin-left: auto; }

  /* Mini chart */
  .mini-chart {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 80px;
    padding: 0.5rem 0;
  }

  .mini-bar {
    flex: 1;
    min-height: 2px;
    border-radius: 2px 2px 0 0;
    transition: height 0.3s ease;
  }

  .mini-bar.rx { background: #4fc3f7; }
  .mini-bar.tx { background: #81c784; }

  .history-labels { display: flex; gap: 1rem; margin-bottom: 0.5rem; font-size: 0.8rem; }

  /* Table */
  .table-wrap { overflow-x: auto; }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th, td {
    padding: 0.6rem 1rem;
    text-align: left;
    border-bottom: 1px solid #333;
    font-size: 0.85rem;
  }

  th { color: #888; font-weight: 600; }
  td { color: #ccc; }

  .iface-name { font-weight: 600; color: #e0e0e0; }
  .error { color: #ff4444; }
  .row-stopped { opacity: 0.5; }
  .proc-name-cell { font-weight: 600; }

  .state-badge {
    padding: 0.2rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    font-weight: 600;
    background: #333;
    color: #888;
  }

  .state-badge.up { background: #003322; color: #00ff88; }
  .state-badge.down { background: #330000; color: #ff4444; }

  /* Disk detail */
  .disk-detail-row { margin-bottom: 1rem; }
  .disk-info { display: flex; gap: 1rem; margin-bottom: 0.5rem; align-items: center; }
  .disk-device { font-weight: 600; color: #e0e0e0; }
  .disk-mount { color: #888; }
  .disk-type { color: #666; font-size: 0.8rem; }
  .disk-health {
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    font-weight: 600;
    background: #222;
  }
  .disk-bar-section { display: flex; align-items: center; gap: 0.75rem; }
  .disk-pct { font-weight: bold; min-width: 50px; }
  .disk-sizes { font-size: 0.8rem; color: #888; margin-top: 0.25rem; }

  .disk-io-summary { display: flex; gap: 2rem; margin-bottom: 1rem; font-size: 0.9rem; color: #aaa; }

  .disk-io-bar {
    display: flex;
    gap: 2px;
    height: 12px;
    border-radius: 2px;
    overflow: hidden;
    min-width: 100px;
  }

  .disk-io-read {
    height: 100%;
    background: #4fc3f7;
    border-radius: 2px;
  }

  .disk-io-write {
    height: 100%;
    background: #81c784;
    border-radius: 2px;
  }

  .io-legend {
    display: flex;
    gap: 1.5rem;
    margin-top: 0.75rem;
    font-size: 0.8rem;
    color: #aaa;
  }

  /* Stat items */
  .stat-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    padding: 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .stat-value {
    font-size: 1.5rem;
    font-weight: bold;
    color: #00ff88;
  }

  .stat-value.rx { color: #4fc3f7; }
  .stat-value.tx { color: #81c784; }

  .stat-label {
    font-size: 0.75rem;
    color: #888;
    text-align: center;
  }

  /* Latency */
  .latency-section {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .latency-input-row {
    display: flex;
    gap: 0.5rem;
  }

  .latency-input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    background: #0f0f23;
    border: 1px solid #333;
    color: #e0e0e0;
    border-radius: 0.25rem;
    font-size: 0.85rem;
  }

  .latency-input:focus {
    outline: none;
    border-color: #00ff88;
  }

  .btn-ping {
    padding: 0.5rem 1rem;
    background: #16213e;
    border: 1px solid #444;
    color: #ccc;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-ping:hover { background: #333; border-color: #00ff88; color: #00ff88; }

  .latency-result {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .latency-target { color: #888; font-size: 0.85rem; }
  .latency-value { font-size: 1.1rem; font-weight: bold; }
  .latency-time { font-size: 0.75rem; color: #666; margin-left: auto; }

  /* Alerts */
  .alert-header { display: flex; justify-content: space-between; align-items: center; }
  .no-alerts { padding: 2rem; text-align: center; color: #00ff88; }

  .alert-row {
    border-left: 3px solid;
    padding: 0.75rem 1rem;
    margin-bottom: 0.75rem;
    background: #16213e;
    border-radius: 0 0.5rem 0.5rem 0;
  }

  .alert-main { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 0.5rem; flex-wrap: wrap; }
  .alert-severity { font-weight: bold; font-size: 0.8rem; }
  .alert-category { color: #888; font-size: 0.8rem; }
  .alert-message { color: #e0e0e0; }

  .alert-meta { display: flex; gap: 1rem; flex-wrap: wrap; font-size: 0.75rem; color: #888; align-items: center; }

  .btn-ack, .btn-refresh {
    padding: 0.3rem 0.75rem;
    background: #1a1a2e;
    border: 1px solid #444;
    color: #ccc;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .btn-ack:hover { background: #333; border-color: #00ff88; color: #00ff88; }
  .btn-refresh:hover { background: #333; }

  .warning-card { border: 1px solid #ff8800; }

  /* History controls */
  .history-controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 1rem;
    padding: 0.75rem;
    background: #1a1a2e;
    border-radius: 0.5rem;
    font-size: 0.85rem;
    color: #888;
  }

  .range-btn {
    padding: 0.3rem 0.75rem;
    background: #16213e;
    border: 1px solid #333;
    color: #888;
    border-radius: 0.25rem;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .range-btn:hover { border-color: #00ff88; color: #ccc; }
  .range-btn.active { background: #003322; border-color: #00ff88; color: #00ff88; }
</style>
