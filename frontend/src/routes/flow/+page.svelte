<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  let flowStatus: any = null;
  let topTalkers: any = null;
  let loading = true;
  let error = '';
  let autoRefresh = true;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // Export config form
  let collectorIp = '';
  let collectorPort = '';
  let exportMessage = '';

  // Flow rate history for sparkline chart
  let flowRateHistory: { time: string; count: number }[] = [];
  const MAX_HISTORY = 30;

  onMount(async () => {
    await fetchAll();
    startAutoRefresh();
  });

  onDestroy(() => {
    stopAutoRefresh();
  });

  function startAutoRefresh() {
    stopAutoRefresh();
    refreshInterval = setInterval(async () => {
      if (autoRefresh) {
        await fetchAll();
      }
    }, 5000);
  }

  function stopAutoRefresh() {
    if (refreshInterval) {
      clearInterval(refreshInterval);
      refreshInterval = null;
    }
  }

  async function fetchAll() {
    try {
      error = '';
      const [statusRes, topRes] = await Promise.all([
        fetch('/api/flows/status').then(r => r.json()),
        fetch('/api/flows/top').then(r => r.json()),
      ]);

      if (statusRes.error) {
        error = statusRes.error;
      } else {
        flowStatus = statusRes;
        // Update flow rate history
        const now = new Date();
        const timeStr = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
        flowRateHistory = [
          ...flowRateHistory.slice(-(MAX_HISTORY - 1)),
          { time: timeStr, count: statusRes.active_flows || 0 }
        ];
      }

      if (topRes.error) {
        error = error ? error + '; ' + topRes.error : topRes.error;
      } else {
        topTalkers = topRes;
      }
    } catch (e) {
      error = 'Failed to fetch flow data';
    } finally {
      loading = false;
    }
  }

  async function setExportCollector() {
    if (!collectorIp || !collectorPort) {
      error = 'Collector IP and port are required';
      return;
    }
    try {
      exportMessage = '';
      error = '';
      const res = await fetch('/api/flows/export', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          collector_ip: collectorIp,
          collector_port: parseInt(collectorPort)
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        exportMessage = data.message || 'Collector configured';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to configure export collector';
    }
  }

  async function toggleExport(enable: boolean) {
    try {
      error = '';
      const endpoint = enable ? '/api/flows/export/enable' : '/api/flows/export/disable';
      const res = await fetch(endpoint, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        exportMessage = data.message || (enable ? 'Export enabled' : 'Export disabled');
        await fetchAll();
      }
    } catch (e) {
      error = `Failed to ${enable ? 'enable' : 'disable'} flow export`;
    }
  }

  async function setupClassify() {
    try {
      error = '';
      const res = await fetch('/api/flows/classify-setup', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        exportMessage = data.message || 'Classify table configured';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to set up classify table';
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatNumber(n: number): string {
    return n.toLocaleString();
  }

  // Compute CSS pie chart angles for protocol distribution
  function computePieSegments(dist: any[]): { protocol: string; start: number; end: number; pct: number; color: string }[] {
    if (!dist || dist.length === 0) return [];
    const total = dist.reduce((s, d) => s + (d.percentage || 0), 0) || 1;
    let cumulative = 0;
    const colors = ['#00ff88', '#ff6b6b', '#ffd93d', '#6bcbff', '#c084fc', '#fb923c', '#34d399', '#f472b6'];
    return dist.map((d, i) => {
      const start = cumulative;
      cumulative += (d.percentage / total) * 360;
      return {
        protocol: d.protocol,
        start,
        end: cumulative,
        pct: d.percentage,
        color: colors[i % colors.length],
      };
    });
  }

  function pieGradient(segments: { start: number; end: number; color: string }[]): string {
    if (segments.length === 0) return 'conic-gradient(#333 0deg 360deg)';
    const parts = segments.map(s => `${s.color} ${s.start}deg ${s.end}deg`);
    return `conic-gradient(${parts.join(', ')})`;
  }
</script>

<svelte:head>
  <title>VectorOS - Flow Monitor</title>
</svelte:head>

<div class="flow-page">
  <h1>Flow Monitor</h1>

  <!-- Status Card -->
  <div class="status-card">
    <div class="status-header">
      <h2>Monitoring Status</h2>
      <div class="button-row">
        <button class="btn-sm" class:btn-active={autoRefresh} on:click={() => { autoRefresh = !autoRefresh; if (autoRefresh) startAutoRefresh(); else stopAutoRefresh(); }}>
          {autoRefresh ? 'Auto-refresh ON' : 'Auto-refresh OFF'}
        </button>
        <button class="btn-sm btn-secondary" on:click={fetchAll}>Refresh Now</button>
      </div>
    </div>

    {#if loading}
      <p>Loading...</p>
    {:else if flowStatus}
      <div class="stat-grid">
        <div class="stat-item">
          <span class="stat-label">Active Flows</span>
          <span class="stat-value">{formatNumber(flowStatus.active_flows || 0)}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Flow Source</span>
          <span class="stat-value source-badge">{flowStatus.flow_source || 'none'}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Export Status</span>
          <span class="status-badge" class:enabled={flowStatus.export_enabled} class:disabled={!flowStatus.export_enabled}>
            {flowStatus.export_enabled ? 'ACTIVE' : 'INACTIVE'}
          </span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Collector</span>
          <span class="stat-value">
            {flowStatus.collector_ip ? `${flowStatus.collector_ip}:${flowStatus.collector_port}` : 'Not configured'}
          </span>
        </div>
      </div>

      {#if flowStatus.flow_plugins_found && flowStatus.flow_plugins_found.length > 0}
        <div class="plugins-info">
          <span class="plugins-label">Flow plugins:</span>
          {#each flowStatus.flow_plugins_found as plugin}
            <span class="plugin-badge">{plugin}</span>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  {#if exportMessage}
    <div class="success-card">{exportMessage}</div>
  {/if}

  <!-- Flow Rate Over Time -->
  {#if flowRateHistory.length > 1}
    <div class="chart-card">
      <h2>Active Flows Over Time</h2>
      <div class="sparkline-container">
        {@const maxVal = Math.max(...flowRateHistory.map(h => h.count), 1)}
        <div class="sparkline">
          {#each flowRateHistory as point, i}
            {@const h = (point.count / maxVal) * 100}
            <div
              class="spark-bar"
              style="height: {Math.max(h, 2)}%"
              title="{point.time}: {point.count} flows"
            ></div>
          {/each}
        </div>
        <div class="sparkline-labels">
          <span>{flowRateHistory[0]?.time || ''}</span>
          <span>{flowRateHistory[Math.floor(flowRateHistory.length / 2)]?.time || ''}</span>
          <span>{flowRateHistory[flowRateHistory.length - 1]?.time || ''}</span>
        </div>
      </div>
    </div>
  {/if}

  <div class="grid-2col">
    <!-- Top Source IPs -->
    <div class="card">
      <h2>Top 10 Source IPs</h2>
      {#if topTalkers?.top_sources_by_bytes?.length > 0}
        {@const maxBytes = topTalkers.top_sources_by_bytes[0]?.bytes || 1}
        {#each topTalkers.top_sources_by_bytes as talker, i}
          <div class="bar-row">
            <span class="bar-label" title={talker.address}>{talker.address}</span>
            <div class="bar-track">
              <div class="bar-fill" style="width: {(talker.bytes / maxBytes) * 100}%"></div>
            </div>
            <span class="bar-value">{formatBytes(talker.bytes)}</span>
            <span class="bar-flows">{talker.flow_count} flows</span>
          </div>
        {/each}
      {:else}
        <p class="no-data">No flow data available</p>
      {/if}
    </div>

    <!-- Top Destination IPs -->
    <div class="card">
      <h2>Top 10 Destination IPs</h2>
      {#if topTalkers?.top_destinations_by_bytes?.length > 0}
        {@const maxBytes = topTalkers.top_destinations_by_bytes[0]?.bytes || 1}
        {#each topTalkers.top_destinations_by_bytes as talker, i}
          <div class="bar-row">
            <span class="bar-label" title={talker.address}>{talker.address}</span>
            <div class="bar-track">
              <div class="bar-fill bar-fill-dst" style="width: {(talker.bytes / maxBytes) * 100}%"></div>
            </div>
            <span class="bar-value">{formatBytes(talker.bytes)}</span>
            <span class="bar-flows">{talker.flow_count} flows</span>
          </div>
        {/each}
      {:else}
        <p class="no-data">No flow data available</p>
      {/if}
    </div>
  </div>

  <!-- Protocol Distribution Pie Chart (CSS only) -->
  <div class="chart-card">
    <h2>Protocol Distribution</h2>
    {#if topTalkers?.protocol_distribution?.length > 0}
      <div class="pie-layout">
        <div class="pie-chart-wrapper">
          <div class="pie-chart" style="background: {pieGradient(computePieSegments(topTalkers.protocol_distribution))}"></div>
          <div class="pie-center">
            <span class="pie-center-label">Protocols</span>
            <span class="pie-center-value">{topTalkers.protocol_distribution.length}</span>
          </div>
        </div>
        <div class="pie-legend">
          {#each computePieSegments(topTalkers.protocol_distribution) as seg}
            <div class="legend-item">
              <span class="legend-dot" style="background: {seg.color}"></span>
              <span class="legend-label">{seg.protocol}</span>
              <span class="legend-pct">{seg.pct}%</span>
              <span class="legend-bytes">{formatBytes(topTalkers.protocol_distribution.find(p => p.protocol === seg.protocol)?.bytes || 0)}</span>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <p class="no-data">No protocol data available</p>
    {/if}
  </div>

  <!-- Export Configuration -->
  <div class="config-card">
    <h2>NetFlow/IPFIX Export</h2>
    <form on:submit|preventDefault={setExportCollector}>
      <div class="form-row">
        <div class="form-group">
          <label for="collector-ip">Collector IP</label>
          <input type="text" id="collector-ip" bind:value={collectorIp} placeholder="e.g. 10.0.0.100" />
        </div>
        <div class="form-group">
          <label for="collector-port">Collector Port</label>
          <input type="number" id="collector-port" bind:value={collectorPort} placeholder="e.g. 9995" min="1" max="65535" />
        </div>
      </div>
      <div class="button-row">
        <button type="submit" class="btn-primary">Set Collector</button>
        {#if flowStatus?.export_enabled}
          <button type="button" class="btn-danger" on:click={() => toggleExport(false)}>Disable Export</button>
        {:else}
          <button type="button" class="btn-primary" on:click={() => toggleExport(true)}>Enable Export</button>
        {/if}
        <button type="button" class="btn-secondary" on:click={setupClassify}>Setup Classify Table</button>
      </div>
    </form>
  </div>
</div>

<style>
  .flow-page {
    max-width: 1400px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
    font-size: 1.1rem;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .status-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
    margin-top: 1rem;
  }

  .stat-item {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .stat-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
  }

  .stat-value {
    font-size: 1.2rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  .source-badge {
    background: #16213e;
    padding: 0.2rem 0.6rem;
    border-radius: 0.3rem;
    display: inline-block;
  }

  .status-badge {
    padding: 0.2rem 0.8rem;
    border-radius: 0.3rem;
    font-weight: bold;
    font-size: 0.85rem;
    display: inline-block;
  }

  .status-badge.enabled {
    background: #003322;
    color: #00ff88;
  }

  .status-badge.disabled {
    background: #331111;
    color: #ff4444;
  }

  .plugins-info {
    margin-top: 1rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .plugins-label {
    font-size: 0.85rem;
    color: #888;
  }

  .plugin-badge {
    background: #16213e;
    padding: 0.2rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    color: #6bcbff;
  }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #ff4444;
  }

  .success-card {
    background: #1a2e1a;
    border: 1px solid #00ff88;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #00ff88;
  }

  .grid-2col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .chart-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .config-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  /* Bar chart styles */
  .bar-row {
    display: grid;
    grid-template-columns: 140px 1fr 80px 70px;
    gap: 0.5rem;
    align-items: center;
    padding: 0.4rem 0;
    border-bottom: 1px solid #222;
  }

  .bar-row:last-child {
    border-bottom: none;
  }

  .bar-label {
    font-size: 0.85rem;
    color: #e0e0e0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: monospace;
  }

  .bar-track {
    height: 16px;
    background: #0f0f23;
    border-radius: 0.25rem;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    background: linear-gradient(90deg, #00ff88, #00cc66);
    border-radius: 0.25rem;
    transition: width 0.3s ease;
  }

  .bar-fill-dst {
    background: linear-gradient(90deg, #6bcbff, #3399ff);
  }

  .bar-value {
    font-size: 0.8rem;
    color: #e0e0e0;
    text-align: right;
    font-family: monospace;
  }

  .bar-flows {
    font-size: 0.75rem;
    color: #888;
    text-align: right;
  }

  .no-data {
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  /* Sparkline chart */
  .sparkline-container {
    padding: 0.5rem 0;
  }

  .sparkline {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 80px;
    background: #0f0f23;
    border-radius: 0.5rem;
    padding: 0.5rem;
  }

  .spark-bar {
    flex: 1;
    background: linear-gradient(to top, #00ff88, #00cc66);
    border-radius: 2px 2px 0 0;
    min-width: 4px;
    transition: height 0.3s ease;
  }

  .sparkline-labels {
    display: flex;
    justify-content: space-between;
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.3rem;
    padding: 0 0.5rem;
  }

  /* Pie chart */
  .pie-layout {
    display: flex;
    align-items: center;
    gap: 2rem;
    flex-wrap: wrap;
  }

  .pie-chart-wrapper {
    position: relative;
    width: 160px;
    height: 160px;
    flex-shrink: 0;
  }

  .pie-chart {
    width: 100%;
    height: 100%;
    border-radius: 50%;
  }

  .pie-center {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 70px;
    height: 70px;
    background: #1a1a2e;
    border-radius: 50%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .pie-center-label {
    font-size: 0.6rem;
    color: #888;
    text-transform: uppercase;
  }

  .pie-center-value {
    font-size: 1.2rem;
    font-weight: bold;
    color: #00ff88;
  }

  .pie-legend {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    flex: 1;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .legend-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .legend-label {
    font-size: 0.9rem;
    color: #e0e0e0;
    min-width: 50px;
  }

  .legend-pct {
    font-size: 0.85rem;
    color: #888;
    min-width: 45px;
    text-align: right;
  }

  .legend-bytes {
    font-size: 0.8rem;
    color: #6bcbff;
    font-family: monospace;
  }

  /* Form */
  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 0.75rem;
  }

  label {
    font-size: 0.85rem;
    color: #888;
  }

  input, select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.6rem;
    border-radius: 0.5rem;
    font-size: 0.95rem;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  .button-row {
    display: flex;
    gap: 1rem;
    margin-top: 0.5rem;
  }

  .btn-sm {
    padding: 0.4rem 0.8rem;
    font-size: 0.8rem;
    border-radius: 0.4rem;
    border: 1px solid #333;
    background: #16213e;
    color: #e0e0e0;
    cursor: pointer;
  }

  .btn-active {
    background: #003322;
    border-color: #00ff88;
    color: #00ff88;
  }

  .btn-primary {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.6rem 1.2rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-primary:hover { opacity: 0.9; }

  .btn-secondary {
    background: #333;
    color: #e0e0e0;
    border: none;
    padding: 0.6rem 1.2rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-secondary:hover { opacity: 0.9; }

  .btn-danger {
    background: #ff4444;
    color: #ffffff;
    border: none;
    padding: 0.6rem 1.2rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-danger:hover { opacity: 0.9; }

  @media (max-width: 900px) {
    .grid-2col {
      grid-template-columns: 1fr;
    }

    .bar-row {
      grid-template-columns: 120px 1fr 60px;
    }

    .bar-flows {
      display: none;
    }
  }
</style>
