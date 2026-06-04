<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  let conntrackStatus: any = null;
  let topTalkers: any = null;
  let connections: any = null;
  let loading = true;
  let error = '';
  let autoRefresh = true;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // Connection count history for sparkline
  let connHistory: { time: string; count: number }[] = [];
  const MAX_HISTORY = 30;

  // Filter form
  let filterIp = '';
  let filterPort = '';
  let filterProtocol = '';
  let filteredConnections: any = null;
  let filtering = false;

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
      if (autoRefresh && !filtering) {
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
        fetch('/api/conntrack/status').then(r => r.json()),
        fetch('/api/conntrack/top').then(r => r.json()),
      ]);

      if (statusRes.error) {
        error = statusRes.error;
      } else {
        conntrackStatus = statusRes;
        const now = new Date();
        const timeStr = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
        connHistory = [
          ...connHistory.slice(-(MAX_HISTORY - 1)),
          { time: timeStr, count: statusRes.stats?.total_connections || 0 }
        ];
      }

      if (topRes.error) {
        error = error ? error + '; ' + topRes.error : topRes.error;
      } else {
        topTalkers = topRes;
      }
    } catch (e) {
      error = 'Failed to fetch connection tracking data';
    } finally {
      loading = false;
    }
  }

  async function applyFilter() {
    if (!filterIp && !filterPort && !filterProtocol) {
      filteredConnections = null;
      filtering = false;
      return;
    }

    try {
      filtering = true;
      error = '';
      const payload: any = {};
      if (filterIp) payload.ip = filterIp;
      if (filterPort) payload.port = parseInt(filterPort);
      if (filterProtocol) payload.protocol = filterProtocol;

      const res = await fetch('/api/conntrack/filter', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        filteredConnections = data;
      }
    } catch (e) {
      error = 'Failed to filter connections';
    }
  }

  function clearFilter() {
    filterIp = '';
    filterPort = '';
    filterProtocol = '';
    filteredConnections = null;
    filtering = false;
  }

  function formatNumber(n: number): string {
    return n.toLocaleString();
  }

  function stateColor(state: string): string {
    switch (state?.toLowerCase()) {
      case 'established': return '#00ff88';
      case 'syn-sent':
      case 'syn-received': return '#ffd93d';
      case 'time-wait': return '#888';
      case 'close-wait': return '#ff6b6b';
      default: return '#6bcbff';
    }
  }
</script>

<svelte:head>
  <title>VectorOS - Connection Tracking</title>
</svelte:head>

<div class="conntrack-page">
  <h1>Connection Tracking</h1>

  <!-- Status Card -->
  <div class="status-card">
    <div class="status-header">
      <h2>Connection Status</h2>
      <div class="button-row">
        <button class="btn-sm" class:btn-active={autoRefresh} on:click={() => { autoRefresh = !autoRefresh; if (autoRefresh) startAutoRefresh(); else stopAutoRefresh(); }}>
          {autoRefresh ? 'Auto-refresh ON' : 'Auto-refresh OFF'}
        </button>
        <button class="btn-sm btn-secondary" on:click={fetchAll}>Refresh Now</button>
      </div>
    </div>

    {#if loading}
      <p>Loading...</p>
    {:else if conntrackStatus}
      <div class="stat-grid">
        <div class="stat-item">
          <span class="stat-label">Active Connections</span>
          <span class="stat-value stat-big">{formatNumber(conntrackStatus.stats?.total_connections || 0)}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Established</span>
          <span class="stat-value" style="color: #00ff88">{formatNumber(conntrackStatus.stats?.established_connections || 0)}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">New (SYN)</span>
          <span class="stat-value" style="color: #ffd93d">{formatNumber(conntrackStatus.stats?.new_connections || 0)}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Data Source</span>
          <span class="stat-value source-badge">{conntrackStatus.data_source || 'none'}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">NAT Interfaces</span>
          <span class="stat-value">{conntrackStatus.nat_interfaces?.length || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">ARP Neighbors</span>
          <span class="stat-value">{conntrackStatus.arp_neighbor_count || 0}</span>
        </div>
      </div>

      {#if conntrackStatus.nat_interfaces?.length > 0}
        <div class="nat-ifaces">
          <span class="plugins-label">NAT Interfaces:</span>
          {#each conntrackStatus.nat_interfaces as iface}
            <span class="plugin-badge">{iface.name} ({iface.direction})</span>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  <!-- Connection Count Over Time -->
  {#if connHistory.length > 1}
    <div class="chart-card">
      <h2>Connections Over Time</h2>
      <div class="sparkline-container">
        {@const maxVal = Math.max(...connHistory.map(h => h.count), 1)}
        <div class="sparkline">
          {#each connHistory as point, i}
            {@const h = (point.count / maxVal) * 100}
            <div
              class="spark-bar"
              style="height: {Math.max(h, 2)}%"
              title="{point.time}: {point.count} connections"
            ></div>
          {/each}
        </div>
        <div class="sparkline-labels">
          <span>{connHistory[0]?.time || ''}</span>
          <span>{connHistory[Math.floor(connHistory.length / 2)]?.time || ''}</span>
          <span>{connHistory[connHistory.length - 1]?.time || ''}</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- Protocol Distribution + Connection State Distribution -->
  <div class="grid-2col">
    <!-- Protocol Distribution Pie Chart -->
    <div class="chart-card">
      <h2>Protocol Distribution</h2>
      {#if conntrackStatus?.stats?.protocol_distribution}
        {@const pd = conntrackStatus.stats.protocol_distribution}
        {@const total = (pd.tcp || 0) + (pd.udp || 0) + (pd.icmp || 0) + (pd.other || 0)}
        {#if total > 0}
          <div class="pie-layout">
            <div class="pie-chart-wrapper">
              {@const tcpPct = ((pd.tcp || 0) / total) * 360}
              {@const udpPct = ((pd.udp || 0) / total) * 360}
              {@const icmpPct = ((pd.icmp || 0) / total) * 360}
              <div class="pie-chart" style="background: conic-gradient(#00ff88 0deg {tcpPct}deg, #6bcbff {tcpPct}deg {tcpPct + udpPct}deg, #ffd93d {tcpPct + udpPct}deg {tcpPct + udpPct + icmpPct}deg, #c084fc {tcpPct + udpPct + icmpPct}deg 360deg)"></div>
              <div class="pie-center">
                <span class="pie-center-label">Total</span>
                <span class="pie-center-value">{formatNumber(total)}</span>
              </div>
            </div>
            <div class="pie-legend">
              <div class="legend-item">
                <span class="legend-dot" style="background: #00ff88"></span>
                <span class="legend-label">TCP</span>
                <span class="legend-pct">{((pd.tcp || 0) / total * 100).toFixed(1)}%</span>
                <span class="legend-count">{formatNumber(pd.tcp || 0)}</span>
              </div>
              <div class="legend-item">
                <span class="legend-dot" style="background: #6bcbff"></span>
                <span class="legend-label">UDP</span>
                <span class="legend-pct">{((pd.udp || 0) / total * 100).toFixed(1)}%</span>
                <span class="legend-count">{formatNumber(pd.udp || 0)}</span>
              </div>
              <div class="legend-item">
                <span class="legend-dot" style="background: #ffd93d"></span>
                <span class="legend-label">ICMP</span>
                <span class="legend-pct">{((pd.icmp || 0) / total * 100).toFixed(1)}%</span>
                <span class="legend-count">{formatNumber(pd.icmp || 0)}</span>
              </div>
              {#if pd.other > 0}
                <div class="legend-item">
                  <span class="legend-dot" style="background: #c084fc"></span>
                  <span class="legend-label">Other</span>
                  <span class="legend-pct">{((pd.other || 0) / total * 100).toFixed(1)}%</span>
                  <span class="legend-count">{formatNumber(pd.other || 0)}</span>
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <p class="no-data">No protocol data available</p>
        {/if}
      {:else}
        <p class="no-data">No protocol data available</p>
      {/if}
    </div>

    <!-- Connection State Distribution -->
    <div class="chart-card">
      <h2>Connection State Distribution</h2>
      {#if conntrackStatus?.stats?.state_distribution && Object.keys(conntrackStatus.stats.state_distribution).length > 0}
        {@const states = Object.entries(conntrackStatus.stats.state_distribution).sort((a, b) => b[1] - a[1])}
        {@const maxStateCount = states[0]?.[1] || 1}
        {#each states as [state, count]}
          <div class="bar-row">
            <span class="bar-label" style="color: {stateColor(state)}">{state}</span>
            <div class="bar-track">
              <div class="bar-fill" style="width: {(count / maxStateCount) * 100}%; background: {stateColor(state)}"></div>
            </div>
            <span class="bar-value">{formatNumber(count)}</span>
          </div>
        {/each}
      {:else}
        <p class="no-data">No state data available</p>
      {/if}
    </div>
  </div>

  <!-- Top Talkers -->
  <div class="grid-2col">
    <!-- Top Source IPs -->
    <div class="card">
      <h2>Top 10 Source IPs</h2>
      {#if topTalkers?.top_sources?.length > 0}
        {@const maxCount = topTalkers.top_sources[0]?.connection_count || 1}
        {#each topTalkers.top_sources as talker, i}
          <div class="bar-row">
            <span class="bar-label" title={talker.address}>{talker.address}</span>
            <div class="bar-track">
              <div class="bar-fill" style="width: {(talker.connection_count / maxCount) * 100}%"></div>
            </div>
            <span class="bar-value">{formatNumber(talker.connection_count)}</span>
            <span class="bar-count">conns</span>
          </div>
        {/each}
      {:else}
        <p class="no-data">No source IP data available</p>
      {/if}
    </div>

    <!-- Top Destination IPs -->
    <div class="card">
      <h2>Top 10 Destination IPs</h2>
      {#if topTalkers?.top_destinations?.length > 0}
        {@const maxCount = topTalkers.top_destinations[0]?.connection_count || 1}
        {#each topTalkers.top_destinations as talker, i}
          <div class="bar-row">
            <span class="bar-label" title={talker.address}>{talker.address}</span>
            <div class="bar-track">
              <div class="bar-fill bar-fill-dst" style="width: {(talker.connection_count / maxCount) * 100}%"></div>
            </div>
            <span class="bar-value">{formatNumber(talker.connection_count)}</span>
            <span class="bar-count">conns</span>
          </div>
        {/each}
      {:else}
        <p class="no-data">No destination IP data available</p>
      {/if}
    </div>
  </div>

  <!-- Top Ports -->
  {#if conntrackStatus?.stats?.top_tcp_dst_ports?.length > 0 || conntrackStatus?.stats?.top_udp_dst_ports?.length > 0}
    <div class="grid-2col">
      {#if conntrackStatus?.stats?.top_tcp_dst_ports?.length > 0}
        <div class="card">
          <h2>Top TCP Destination Ports</h2>
          {@const maxPortCount = conntrackStatus.stats.top_tcp_dst_ports[0]?.count || 1}
          {#each conntrackStatus.stats.top_tcp_dst_ports as portInfo}
            <div class="bar-row">
              <span class="bar-label port-label">:{portInfo.port}</span>
              <div class="bar-track">
                <div class="bar-fill bar-fill-tcp" style="width: {(portInfo.count / maxPortCount) * 100}%"></div>
              </div>
              <span class="bar-value">{formatNumber(portInfo.count)}</span>
            </div>
          {/each}
        </div>
      {/if}

      {#if conntrackStatus?.stats?.top_udp_dst_ports?.length > 0}
        <div class="card">
          <h2>Top UDP Destination Ports</h2>
          {@const maxPortCount = conntrackStatus.stats.top_udp_dst_ports[0]?.count || 1}
          {#each conntrackStatus.stats.top_udp_dst_ports as portInfo}
            <div class="bar-row">
              <span class="bar-label port-label">:{portInfo.port}</span>
              <div class="bar-track">
                <div class="bar-fill bar-fill-udp" style="width: {(portInfo.count / maxPortCount) * 100}%"></div>
              </div>
              <span class="bar-value">{formatNumber(portInfo.count)}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Connection Filter -->
  <div class="config-card">
    <h2>Filter Connections</h2>
    <form on:submit|preventDefault={applyFilter}>
      <div class="form-row">
        <div class="form-group">
          <label for="filter-ip">IP Address</label>
          <input type="text" id="filter-ip" bind:value={filterIp} placeholder="e.g. 192.168.1.100" />
        </div>
        <div class="form-group">
          <label for="filter-port">Port</label>
          <input type="number" id="filter-port" bind:value={filterPort} placeholder="e.g. 443" min="1" max="65535" />
        </div>
        <div class="form-group">
          <label for="filter-proto">Protocol</label>
          <select id="filter-proto" bind:value={filterProtocol}>
            <option value="">Any</option>
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="icmp">ICMP</option>
          </select>
        </div>
      </div>
      <div class="button-row">
        <button type="submit" class="btn-primary">Apply Filter</button>
        <button type="button" class="btn-secondary" on:click={clearFilter}>Clear</button>
      </div>
    </form>

    {#if filteredConnections}
      <div class="filter-results">
        <h3>Filtered Results: {filteredConnections.total_after} of {filteredConnections.total_before} connections</h3>
        {#if filteredConnections.connections?.length > 0}
          <div class="connections-table">
            <div class="conn-header">
              <span class="col-proto">Proto</span>
              <span class="col-src">Source</span>
              <span class="col-dst">Destination</span>
              <span class="col-state">State</span>
              <span class="col-dir">Direction</span>
            </div>
            {#each filteredConnections.connections as conn}
              <div class="conn-row">
                <span class="col-proto">
                  <span class="proto-badge" class:tcp={conn.protocol === 'TCP'} class:udp={conn.protocol === 'UDP'} class:icmp={conn.protocol === 'ICMP'}>
                    {conn.protocol}
                  </span>
                </span>
                <span class="col-src mono">{conn.src_ip}{conn.src_port ? ':' + conn.src_port : ''}</span>
                <span class="col-dst mono">{conn.dst_ip}{conn.dst_port ? ':' + conn.dst_port : ''}</span>
                <span class="col-state" style="color: {stateColor(conn.state)}">{conn.state}</span>
                <span class="col-dir">{conn.direction || '-'}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="no-data">No connections match the filter</p>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .conntrack-page {
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

  h3 {
    margin: 1rem 0 0.5rem;
    color: #e0e0e0;
    font-size: 1rem;
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
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
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

  .stat-big {
    font-size: 1.6rem;
    color: #00ff88;
  }

  .source-badge {
    background: #16213e;
    padding: 0.2rem 0.6rem;
    border-radius: 0.3rem;
    display: inline-block;
  }

  .nat-ifaces {
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
    grid-template-columns: 140px 1fr 80px 50px;
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

  .port-label {
    color: #888;
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

  .bar-fill-tcp {
    background: linear-gradient(90deg, #00ff88, #00cc66);
  }

  .bar-fill-udp {
    background: linear-gradient(90deg, #6bcbff, #3399ff);
  }

  .bar-value {
    font-size: 0.8rem;
    color: #e0e0e0;
    text-align: right;
    font-family: monospace;
  }

  .bar-count {
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

  .legend-count {
    font-size: 0.8rem;
    color: #6bcbff;
    font-family: monospace;
  }

  /* Form */
  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
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

  /* Filter results table */
  .filter-results {
    margin-top: 1.5rem;
    border-top: 1px solid #333;
    padding-top: 0.5rem;
  }

  .connections-table {
    font-size: 0.85rem;
    overflow-x: auto;
    margin-top: 0.5rem;
  }

  .conn-header, .conn-row {
    display: grid;
    grid-template-columns: 70px 1fr 1fr 120px 140px;
    gap: 0.5rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #222;
    align-items: center;
  }

  .conn-header {
    font-weight: bold;
    color: #888;
    border-bottom: 1px solid #555;
  }

  .conn-row {
    color: #e0e0e0;
  }

  .mono {
    font-family: monospace;
    font-size: 0.8rem;
  }

  .proto-badge {
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: bold;
  }

  .proto-badge.tcp {
    background: #003322;
    color: #00ff88;
  }

  .proto-badge.udp {
    background: #112233;
    color: #6bcbff;
  }

  .proto-badge.icmp {
    background: #332200;
    color: #ffd93d;
  }

  @media (max-width: 900px) {
    .grid-2col {
      grid-template-columns: 1fr;
    }

    .bar-row {
      grid-template-columns: 120px 1fr 60px;
    }

    .bar-count {
      display: none;
    }

    .form-row {
      grid-template-columns: 1fr;
    }

    .conn-header, .conn-row {
      grid-template-columns: 60px 1fr 1fr 80px;
    }

    .col-dir {
      display: none;
    }
  }
</style>
