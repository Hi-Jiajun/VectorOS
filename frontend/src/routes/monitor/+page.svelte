<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // -----------------------------------------------------------------------
  // Types
  // -----------------------------------------------------------------------
  interface CoreMetric { core: number; percent: number; }
  interface MemoryMetric { total: number; used: number; free: number; buffers: number; cached: number; available: number; percent: number; }
  interface DiskMetric { device: string; fstype: string; total: number; used: number; available: number; percent: number; mountpoint: string; }
  interface DiskIoMetric { total_read_bytes_per_sec: number; total_write_bytes_per_sec: number; devices: { name: string; read_bytes_per_sec: number; write_bytes_per_sec: number }[]; }
  interface NetworkMetric { name: string; state: string; rx_bytes: number; tx_bytes: number; rx_packets: number; tx_packets: number; rx_errors: number; tx_errors: number; rx_drops: number; tx_drops: number; rx_bps: number; tx_bps: number; rx_pps: number; tx_pps: number; }
  interface VppMetric { available: boolean; version: string; nat_sessions: number; pppoe_active: number; pppoe_discovery: number; pppoe_total: number; memory_total_mb: number; memory_used_mb: number; memory_percent: number; errors_total: number; }
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
  let activeTab: 'overview' | 'cpu' | 'network' | 'disk' | 'processes' | 'vpp' | 'alerts' = 'overview';

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

  // -----------------------------------------------------------------------
  // Lifecycle
  // -----------------------------------------------------------------------
  onMount(() => {
    fetchMetrics();
    fetchAlerts();
    fetchHistory();
    refreshInterval = setInterval(() => {
      fetchMetrics();
      fetchAlerts();
    }, 5000);
    historyInterval = setInterval(fetchHistory, 30000);
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
                  <td class:error={iface.rx_errors + iface.tx_errors > 0}>{iface.rx_errors + iface.tx_errors}</td>
                  <td class:error={iface.rx_drops + iface.tx_drops > 0}>{iface.rx_drops + iface.tx_drops}</td>
                </tr>
              {/each}
            </tbody>
          </table>
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
          <span>Read: {formatBytes(metrics.disk_io.total_read_bytes_per_sec)}/s</span>
          <span>Write: {formatBytes(metrics.disk_io.total_write_bytes_per_sec)}/s</span>
        </div>
        {#each metrics.disk_io.devices as dev}
          <div class="disk-io-row">
            <span class="dev-name">{dev.name}</span>
            <span class="rx">R: {formatBytes(dev.read_bytes_per_sec)}/s</span>
            <span class="tx">W: {formatBytes(dev.write_bytes_per_sec)}/s</span>
          </div>
        {/each}
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
        {#if metrics.vpp.errors_total > 0}
          <div class="card wide warning-card">
            <h3>VPP Errors ({metrics.vpp.errors_total})</h3>
            <div class="sub-text">Error counters detected in the VPP data plane.</div>
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
  .disk-info { display: flex; gap: 1rem; margin-bottom: 0.5rem; }
  .disk-device { font-weight: 600; color: #e0e0e0; }
  .disk-mount { color: #888; }
  .disk-type { color: #666; font-size: 0.8rem; }
  .disk-bar-section { display: flex; align-items: center; gap: 0.75rem; }
  .disk-pct { font-weight: bold; min-width: 50px; }
  .disk-sizes { font-size: 0.8rem; color: #888; margin-top: 0.25rem; }

  .disk-io-summary { display: flex; gap: 2rem; margin-bottom: 1rem; font-size: 0.9rem; color: #aaa; }
  .disk-io-row { display: flex; gap: 1.5rem; padding: 0.4rem 0; border-bottom: 1px solid #222; font-size: 0.85rem; }
  .dev-name { font-weight: 600; color: #e0e0e0; min-width: 100px; }

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
