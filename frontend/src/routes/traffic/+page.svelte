<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // ── Types ──────────────────────────────────────────────────────────

  interface InterfaceInfo {
    name: string;
    sw_if_index: number;
    state: string;
    mtu: number;
    mac_address?: string;
    ip_addresses?: string[];
    interface_type?: string;
  }

  interface InterfaceStats {
    name: string;
    rx_packets: number;
    tx_packets: number;
    rx_bytes: number;
    tx_bytes: number;
    rx_errors: number;
    tx_errors: number;
    rx_drops: number;
    tx_drops: number;
  }

  interface RateData {
    rxBytesPerSec: number;
    txBytesPerSec: number;
    rxPacketsPerSec: number;
    txPacketsPerSec: number;
  }

  interface TrafficSnapshot {
    rx_bytes: number;
    tx_bytes: number;
    rx_packets: number;
    tx_packets: number;
    ts: number;
  }

  interface IfaceTrafficData {
    stats: InterfaceStats;
    rates: RateData;
    prevSnapshot: TrafficSnapshot | null;
    rxRateHistory: number[];
    txRateHistory: number[];
    // Bandwidth monitor
    peakRxRate: number;
    peakTxRate: number;
    totalRxBytes: number;
    totalTxBytes: number;
    sampleCount: number;
    sumRxRate: number;
    sumTxRate: number;
  }

  // ── State ──────────────────────────────────────────────────────────

  let interfaces: InterfaceInfo[] = [];
  let loading = true;
  let error = '';
  let autoRefresh = true;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let miniStatsInterval: ReturnType<typeof setInterval> | null = null;

  // Per-interface traffic data
  let trafficMap: Map<string, IfaceTrafficData> = new Map();

  // Selected interface for detail view
  let selectedIface = '';

  // Canvas graph references
  let overviewGraphCanvas: HTMLCanvasElement;
  let overviewGraphCtx: CanvasRenderingContext2D | null = null;
  let detailGraphCanvas: HTMLCanvasElement;
  let detailGraphCtx: CanvasRenderingContext2D | null = null;

  const GRAPH_POINTS = 60;
  const POLL_INTERVAL_MS = 2000;

  // Global totals for dashboard widget
  let totalRxRate = 0;
  let totalTxRate = 0;

  // ── Lifecycle ──────────────────────────────────────────────────────

  onMount(async () => {
    await fetchInterfaces();
    refreshInterval = setInterval(async () => {
      if (autoRefresh) {
        await pollAllStats();
      }
    }, POLL_INTERVAL_MS);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
    if (miniStatsInterval) clearInterval(miniStatsInterval);
  });

  // ── Canvas graph setup ─────────────────────────────────────────────

  $: if (overviewGraphCanvas && !overviewGraphCtx) {
    overviewGraphCtx = overviewGraphCanvas.getContext('2d');
  }

  $: if (detailGraphCanvas && !detailGraphCtx) {
    detailGraphCtx = detailGraphCanvas.getContext('2d');
  }

  // Redraw overview graph when data changes
  $: if (overviewGraphCtx) {
    drawOverviewGraph();
  }

  // Redraw detail graph when selected interface data changes
  $: if (detailGraphCtx && selectedIface) {
    const data = trafficMap.get(selectedIface);
    if (data && (data.rxRateHistory.length > 0 || data.txRateHistory.length > 0)) {
      drawDetailGraph(data);
    }
  }

  // ── API helpers ────────────────────────────────────────────────────

  async function fetchInterfaces() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/interfaces');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        interfaces = data.interfaces || [];
        // Initialize traffic data for new interfaces
        for (const iface of interfaces) {
          if (!trafficMap.has(iface.name)) {
            trafficMap.set(iface.name, {
              stats: {
                name: iface.name,
                rx_packets: 0, tx_packets: 0,
                rx_bytes: 0, tx_bytes: 0,
                rx_errors: 0, tx_errors: 0,
                rx_drops: 0, tx_drops: 0,
              },
              rates: { rxBytesPerSec: 0, txBytesPerSec: 0, rxPacketsPerSec: 0, txPacketsPerSec: 0 },
              prevSnapshot: null,
              rxRateHistory: [],
              txRateHistory: [],
              peakRxRate: 0,
              peakTxRate: 0,
              totalRxBytes: 0,
              totalTxBytes: 0,
              sampleCount: 0,
              sumRxRate: 0,
              sumTxRate: 0,
            });
          }
        }
        // Auto-select first interface if none selected
        if (!selectedIface && interfaces.length > 0) {
          selectedIface = interfaces[0].name;
        }
        // Fetch stats for all interfaces
        await pollAllStats();
      }
    } catch {
      error = 'Failed to fetch interfaces';
    } finally {
      loading = false;
    }
  }

  async function fetchIfaceStats(name: string) {
    try {
      const res = await fetch(`/api/interfaces/${encodeURIComponent(name)}/stats`);
      const data = await res.json();
      if (data.stats) {
        const s: InterfaceStats = data.stats;
        const existing = trafficMap.get(name);
        if (existing) {
          const now = Date.now();
          let rates: RateData = existing.rates;

          if (existing.prevSnapshot) {
            const dt = (now - existing.prevSnapshot.ts) / 1000;
            if (dt > 0.3) {
              rates = {
                rxBytesPerSec: Math.max(0, (s.rx_bytes - existing.prevSnapshot.rx_bytes) / dt),
                txBytesPerSec: Math.max(0, (s.tx_bytes - existing.prevSnapshot.tx_bytes) / dt),
                rxPacketsPerSec: Math.max(0, (s.rx_packets - existing.prevSnapshot.rx_packets) / dt),
                txPacketsPerSec: Math.max(0, (s.tx_packets - existing.prevSnapshot.tx_packets) / dt),
              };
            }
          }

          // Update rate histories
          const rxHistory = [...existing.rxRateHistory.slice(-(GRAPH_POINTS - 1)), rates.rxBytesPerSec];
          const txHistory = [...existing.txRateHistory.slice(-(GRAPH_POINTS - 1)), rates.txBytesPerSec];

          // Update bandwidth monitor stats
          const peakRx = Math.max(existing.peakRxRate, rates.rxBytesPerSec);
          const peakTx = Math.max(existing.peakTxRate, rates.txBytesPerSec);
          const newSampleCount = existing.sampleCount + 1;
          const sumRx = existing.sumRxRate + rates.rxBytesPerSec;
          const sumTx = existing.sumTxRate + rates.txBytesPerSec;

          trafficMap.set(name, {
            stats: s,
            rates,
            prevSnapshot: { rx_bytes: s.rx_bytes, tx_bytes: s.tx_bytes, rx_packets: s.rx_packets, tx_packets: s.tx_packets, ts: now },
            rxRateHistory: rxHistory,
            txRateHistory: txHistory,
            peakRxRate: peakRx,
            peakTxRate: peakTx,
            totalRxBytes: s.rx_bytes,
            totalTxBytes: s.tx_bytes,
            sampleCount: newSampleCount,
            sumRxRate: sumRx,
            sumTxRate: sumTx,
          });
        } else {
          // First time - create entry
          const now = Date.now();
          trafficMap.set(name, {
            stats: s,
            rates: { rxBytesPerSec: 0, txBytesPerSec: 0, rxPacketsPerSec: 0, txPacketsPerSec: 0 },
            prevSnapshot: { rx_bytes: s.rx_bytes, tx_bytes: s.tx_bytes, rx_packets: s.rx_packets, tx_packets: s.tx_packets, ts: now },
            rxRateHistory: [],
            txRateHistory: [],
            peakRxRate: 0,
            peakTxRate: 0,
            totalRxBytes: s.rx_bytes,
            totalTxBytes: s.tx_bytes,
            sampleCount: 0,
            sumRxRate: 0,
            sumTxRate: 0,
          });
        }
        trafficMap = trafficMap; // trigger reactivity
      }
    } catch {
      // Silent
    }
  }

  async function pollAllStats() {
    totalRxRate = 0;
    totalTxRate = 0;
    for (const iface of interfaces) {
      await fetchIfaceStats(iface.name);
      const data = trafficMap.get(iface.name);
      if (data) {
        totalRxRate += data.rates.rxBytesPerSec;
        totalTxRate += data.rates.txBytesPerSec;
      }
    }
  }

  function selectInterface(name: string) {
    selectedIface = name;
  }

  // ── Formatting ─────────────────────────────────────────────────────

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
  }

  function formatBytesRate(bytesPerSec: number): string {
    if (bytesPerSec === 0) return '0 B/s';
    if (bytesPerSec >= 1024 * 1024 * 1024) return (bytesPerSec / (1024 * 1024 * 1024)).toFixed(2) + ' GB/s';
    if (bytesPerSec >= 1024 * 1024) return (bytesPerSec / (1024 * 1024)).toFixed(2) + ' MB/s';
    if (bytesPerSec >= 1024) return (bytesPerSec / 1024).toFixed(1) + ' KB/s';
    return bytesPerSec.toFixed(0) + ' B/s';
  }

  function formatBitsRate(bytesPerSec: number): string {
    const bitsPerSec = bytesPerSec * 8;
    if (bitsPerSec >= 1_000_000_000) return (bitsPerSec / 1_000_000_000).toFixed(1) + ' Gbps';
    if (bitsPerSec >= 1_000_000) return (bitsPerSec / 1_000_000).toFixed(1) + ' Mbps';
    if (bitsPerSec >= 1_000) return (bitsPerSec / 1_000).toFixed(1) + ' Kbps';
    return bitsPerSec.toFixed(0) + ' bps';
  }

  function formatPps(packetsPerSec: number): string {
    if (packetsPerSec >= 1_000_000) return (packetsPerSec / 1_000_000).toFixed(2) + ' Mpps';
    if (packetsPerSec >= 1_000) return (packetsPerSec / 1_000).toFixed(1) + ' Kpps';
    return packetsPerSec.toFixed(0) + ' pps';
  }

  function formatCount(n: number): string {
    if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + 'B';
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
    return n.toString();
  }

  function avgRate(data: IfaceTrafficData): { rx: number; tx: number } {
    if (data.sampleCount === 0) return { rx: 0, tx: 0 };
    return {
      rx: data.sumRxRate / data.sampleCount,
      tx: data.sumTxRate / data.sampleCount,
    };
  }

  function trafficBarWidth(rate: number, maxRate: number): number {
    if (maxRate === 0) return 0;
    return Math.max((rate / maxRate) * 100, 1);
  }

  // ── Canvas graph drawing ───────────────────────────────────────────

  function drawOverviewGraph() {
    if (!overviewGraphCtx || !overviewGraphCanvas) return;
    const ctx = overviewGraphCtx;
    const w = overviewGraphCanvas.width;
    const h = overviewGraphCanvas.height;

    // Background
    ctx.fillStyle = '#0f0f23';
    ctx.fillRect(0, 0, w, h);

    // Grid lines
    ctx.strokeStyle = '#1a1a2e';
    ctx.lineWidth = 1;
    for (let i = 1; i < 5; i++) {
      const y = (h / 5) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    // Aggregate all interface histories into total RX/TX
    const totalRxHistory: number[] = [];
    const totalTxHistory: number[] = [];

    for (const iface of interfaces) {
      const data = trafficMap.get(iface.name);
      if (data) {
        for (let i = 0; i < GRAPH_POINTS; i++) {
          const rx = data.rxRateHistory[i] || 0;
          const tx = data.txRateHistory[i] || 0;
          totalRxHistory[i] = (totalRxHistory[i] || 0) + rx;
          totalTxHistory[i] = (totalTxHistory[i] || 0) + tx;
        }
      }
    }

    const allValues = [...totalRxHistory, ...totalTxHistory];
    const maxVal = Math.max(1, ...allValues) * 1.15;

    function drawLine(data: number[], color: string) {
      if (data.length < 2) return;
      const step = w / (GRAPH_POINTS - 1);
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.lineJoin = 'round';
      for (let i = 0; i < data.length; i++) {
        const x = i * step;
        const y = h - (data[i] / maxVal) * h;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Area fill
      const lastX = (data.length - 1) * step;
      ctx.lineTo(lastX, h);
      ctx.lineTo(0, h);
      ctx.closePath();
      ctx.fillStyle = color.replace('1)', '0.12)');
      ctx.fill();
    }

    drawLine(totalRxHistory, 'rgba(77, 171, 247, 1)');
    drawLine(totalTxHistory, 'rgba(81, 207, 102, 1)');

    // Legend
    ctx.font = '11px sans-serif';
    const legendY = 14;
    ctx.fillStyle = '#4dabf7';
    ctx.fillRect(8, legendY - 8, 12, 3);
    ctx.fillStyle = '#aaa';
    ctx.fillText('RX', 24, legendY);
    ctx.fillStyle = '#51cf66';
    ctx.fillRect(56, legendY - 8, 12, 3);
    ctx.fillStyle = '#aaa';
    ctx.fillText('TX', 72, legendY);

    // Scale label
    ctx.fillStyle = '#666';
    ctx.textAlign = 'right';
    ctx.fillText(formatBytesRate(maxVal), w - 8, legendY);
    ctx.textAlign = 'left';
  }

  function drawDetailGraph(data: IfaceTrafficData) {
    if (!detailGraphCtx || !detailGraphCanvas) return;
    const ctx = detailGraphCtx;
    const w = detailGraphCanvas.width;
    const h = detailGraphCanvas.height;

    // Background
    ctx.fillStyle = '#0f0f23';
    ctx.fillRect(0, 0, w, h);

    // Grid lines
    ctx.strokeStyle = '#1a1a2e';
    ctx.lineWidth = 1;
    for (let i = 1; i < 5; i++) {
      const y = (h / 5) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    const allValues = [...data.rxRateHistory, ...data.txRateHistory];
    const maxVal = Math.max(1, ...allValues) * 1.15;

    function drawLine(series: number[], color: string) {
      if (series.length < 2) return;
      const step = w / (GRAPH_POINTS - 1);
      ctx.beginPath();
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.lineJoin = 'round';
      for (let i = 0; i < series.length; i++) {
        const x = i * step;
        const y = h - (series[i] / maxVal) * h;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      const lastX = (series.length - 1) * step;
      ctx.lineTo(lastX, h);
      ctx.lineTo(0, h);
      ctx.closePath();
      ctx.fillStyle = color.replace('1)', '0.12)');
      ctx.fill();
    }

    drawLine(data.rxRateHistory, 'rgba(77, 171, 247, 1)');
    drawLine(data.txRateHistory, 'rgba(81, 207, 102, 1)');

    // Legend
    ctx.font = '11px sans-serif';
    const legendY = 14;
    ctx.fillStyle = '#4dabf7';
    ctx.fillRect(8, legendY - 8, 12, 3);
    ctx.fillStyle = '#aaa';
    ctx.fillText('RX', 24, legendY);
    ctx.fillStyle = '#51cf66';
    ctx.fillRect(56, legendY - 8, 12, 3);
    ctx.fillStyle = '#aaa';
    ctx.fillText('TX', 72, legendY);

    // Scale label
    ctx.fillStyle = '#666';
    ctx.textAlign = 'right';
    ctx.fillText(formatBytesRate(maxVal), w - 8, legendY);
    ctx.textAlign = 'left';
  }

  // ── Derived ────────────────────────────────────────────────────────

  $: selectedData = selectedIface ? trafficMap.get(selectedIface) || null : null;
  $: maxInterfaceRate = Math.max(
    ...interfaces.map((iface) => {
      const d = trafficMap.get(iface.name);
      return d ? Math.max(d.rates.rxBytesPerSec, d.rates.txBytesPerSec) : 0;
    }),
    1
  );
</script>

<svelte:head>
  <title>VectorOS - Traffic Monitor</title>
</svelte:head>

<div class="traffic-monitor">
  <div class="header-row">
    <h1>Traffic Monitor</h1>
    <div class="header-controls">
      <button class="btn-sm" class:btn-active={autoRefresh} on:click={() => { autoRefresh = !autoRefresh; }}>
        {autoRefresh ? 'Auto-refresh ON' : 'Auto-refresh OFF'}
      </button>
      <button class="btn-sm btn-secondary" on:click={pollAllStats}>Refresh</button>
    </div>
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  {#if loading && interfaces.length === 0}
    <div class="loading-card">
      <div class="spinner"></div>
      <span>Loading interfaces...</span>
    </div>
  {:else}
    <!-- ═══ 1. Traffic Dashboard Widget ═══ -->
    <div class="dashboard-widget">
      <h2>Total Traffic</h2>
      <div class="total-rate-grid">
        <div class="total-rate-box rx">
          <span class="total-rate-label">Total RX Rate</span>
          <span class="total-rate-value">{formatBytesRate(totalRxRate)}</span>
          <span class="total-rate-bits">{formatBitsRate(totalRxRate)}</span>
        </div>
        <div class="total-rate-box tx">
          <span class="total-rate-label">Total TX Rate</span>
          <span class="total-rate-value">{formatBytesRate(totalTxRate)}</span>
          <span class="total-rate-bits">{formatBitsRate(totalTxRate)}</span>
        </div>
      </div>

      <!-- Per-interface traffic bars -->
      <div class="iface-bars">
        {#each interfaces as iface}
          {@const data = trafficMap.get(iface.name)}
          {#if data && (data.rates.rxBytesPerSec > 0 || data.rates.txBytesPerSec > 0)}
            <button class="iface-bar-row" class:selected={selectedIface === iface.name} on:click={() => selectInterface(iface.name)}>
              <span class="bar-label">{iface.name}</span>
              <div class="bar-tracks">
                <div class="bar-track">
                  <div class="bar-fill rx" style="width: {trafficBarWidth(data.rates.rxBytesPerSec, maxInterfaceRate)}%"></div>
                </div>
                <div class="bar-track">
                  <div class="bar-fill tx" style="width: {trafficBarWidth(data.rates.txBytesPerSec, maxInterfaceRate)}%"></div>
                </div>
              </div>
              <div class="bar-rates">
                <span class="bar-rate rx">{formatBytesRate(data.rates.rxBytesPerSec)}</span>
                <span class="bar-rate tx">{formatBytesRate(data.rates.txBytesPerSec)}</span>
              </div>
            </button>
          {/if}
        {/each}
      </div>

      <!-- Overview graph -->
      <div class="graph-wrapper">
        <div class="graph-label">Aggregate Traffic Over Time</div>
        <canvas bind:this={overviewGraphCanvas} width={720} height={140} class="traffic-graph"></canvas>
      </div>
    </div>

    <!-- ═══ 2. Interface Traffic Cards ═══ -->
    <div class="iface-cards-section">
      <h2>Interface Traffic</h2>
      <div class="iface-cards-grid">
        {#each interfaces as iface}
          {@const data = trafficMap.get(iface.name)}
          <button class="iface-card" class:selected={selectedIface === iface.name} on:click={() => selectInterface(iface.name)}>
            <div class="iface-card-header">
              <span class="iface-card-name">{iface.name}</span>
              <span class="state-dot" class:up={iface.state === 'up'} class:down={iface.state !== 'up'}></span>
            </div>
            {#if data}
              <div class="iface-card-stats">
                <div class="card-stat-row">
                  <span class="card-stat-label">RX Bytes</span>
                  <span class="card-stat-value rx">{formatBytes(data.stats.rx_bytes)}</span>
                </div>
                <div class="card-stat-row">
                  <span class="card-stat-label">TX Bytes</span>
                  <span class="card-stat-value tx">{formatBytes(data.stats.tx_bytes)}</span>
                </div>
                <div class="card-stat-row">
                  <span class="card-stat-label">RX Pkts</span>
                  <span class="card-stat-value">{formatCount(data.stats.rx_packets)}</span>
                </div>
                <div class="card-stat-row">
                  <span class="card-stat-label">TX Pkts</span>
                  <span class="card-stat-value">{formatCount(data.stats.tx_packets)}</span>
                </div>
                <div class="card-stat-row">
                  <span class="card-stat-label">Errors</span>
                  <span class="card-stat-value" class:err-warn={data.stats.rx_errors + data.stats.tx_errors > 0}>
                    {formatCount(data.stats.rx_errors + data.stats.tx_errors)}
                  </span>
                </div>
                <div class="card-stat-row">
                  <span class="card-stat-label">Drops</span>
                  <span class="card-stat-value" class:err-warn={data.stats.rx_drops + data.stats.tx_drops > 0}>
                    {formatCount(data.stats.rx_drops + data.stats.tx_drops)}
                  </span>
                </div>
                <div class="card-stat-row rate">
                  <span class="card-stat-label">Rate</span>
                  <span class="card-stat-value">{formatBytesRate(data.rates.rxBytesPerSec + data.rates.txBytesPerSec)}</span>
                </div>
              </div>
            {:else}
              <div class="card-stat-empty">No data</div>
            {/if}
          </button>
        {/each}
      </div>
    </div>

    <!-- ═══ 3. Traffic Graph + 4. Bandwidth Monitor (Detail) ═══ -->
    {#if selectedIface && selectedData}
      <div class="detail-section">
        <h2>{selectedIface} &mdash; Traffic Detail</h2>

        <!-- Traffic Graph -->
        <div class="graph-wrapper">
          <div class="graph-header">
            <div class="graph-label">RX/TX Rate Over Time (60 samples)</div>
            <div class="graph-legend">
              <span class="legend-item rx"><span class="legend-dot"></span> RX</span>
              <span class="legend-item tx"><span class="legend-dot"></span> TX</span>
            </div>
          </div>
          <canvas bind:this={detailGraphCanvas} width={720} height={180} class="traffic-graph detail-graph"></canvas>
        </div>

        <!-- Bandwidth Monitor -->
        {@const avg = avgRate(selectedData)}
        <div class="bandwidth-monitor">
          <h3>Bandwidth Monitor</h3>
          <div class="bw-grid">
            <div class="bw-card">
              <span class="bw-label">Current RX</span>
              <span class="bw-value rx">{formatBytesRate(selectedData.rates.rxBytesPerSec)}</span>
              <span class="bw-bits">{formatBitsRate(selectedData.rates.rxBytesPerSec)}</span>
            </div>
            <div class="bw-card">
              <span class="bw-label">Current TX</span>
              <span class="bw-value tx">{formatBytesRate(selectedData.rates.txBytesPerSec)}</span>
              <span class="bw-bits">{formatBitsRate(selectedData.rates.txBytesPerSec)}</span>
            </div>
            <div class="bw-card">
              <span class="bw-label">Peak RX</span>
              <span class="bw-value rx peak">{formatBytesRate(selectedData.peakRxRate)}</span>
              <span class="bw-bits">{formatBitsRate(selectedData.peakRxRate)}</span>
            </div>
            <div class="bw-card">
              <span class="bw-label">Peak TX</span>
              <span class="bw-value tx peak">{formatBytesRate(selectedData.peakTxRate)}</span>
              <span class="bw-bits">{formatBitsRate(selectedData.peakTxRate)}</span>
            </div>
            <div class="bw-card">
              <span class="bw-label">Average RX</span>
              <span class="bw-value rx">{formatBytesRate(avg.rx)}</span>
              <span class="bw-bits">{formatBitsRate(avg.rx)}</span>
            </div>
            <div class="bw-card">
              <span class="bw-label">Average TX</span>
              <span class="bw-value tx">{formatBytesRate(avg.tx)}</span>
              <span class="bw-bits">{formatBitsRate(avg.tx)}</span>
            </div>
          </div>
        </div>

        <!-- Detailed counters -->
        <div class="detail-counters">
          <h3>Counters</h3>
          <div class="counters-grid">
            <div class="counter-cell">
              <span class="counter-label">RX Bytes</span>
              <span class="counter-value rx">{formatBytes(selectedData.stats.rx_bytes)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">TX Bytes</span>
              <span class="counter-value tx">{formatBytes(selectedData.stats.tx_bytes)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">RX Packets</span>
              <span class="counter-value">{formatCount(selectedData.stats.rx_packets)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">TX Packets</span>
              <span class="counter-value">{formatCount(selectedData.stats.tx_packets)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">RX Errors</span>
              <span class="counter-value" class:err-warn={selectedData.stats.rx_errors > 0}>{formatCount(selectedData.stats.rx_errors)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">TX Errors</span>
              <span class="counter-value" class:err-warn={selectedData.stats.tx_errors > 0}>{formatCount(selectedData.stats.tx_errors)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">RX Drops</span>
              <span class="counter-value" class:err-warn={selectedData.stats.rx_drops > 0}>{formatCount(selectedData.stats.rx_drops)}</span>
            </div>
            <div class="counter-cell">
              <span class="counter-label">TX Drops</span>
              <span class="counter-value" class:err-warn={selectedData.stats.tx_drops > 0}>{formatCount(selectedData.stats.tx_drops)}</span>
            </div>
          </div>
        </div>
      </div>
    {:else}
      <div class="empty-card">
        <p>Select an interface to view detailed traffic data</p>
      </div>
    {/if}
  {/if}
</div>

<style>
  /* ── Page layout ──────────────────────────────────────── */
  .traffic-monitor {
    max-width: 1400px;
  }

  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1.5rem;
  }

  h1 {
    color: #00ff88;
    margin: 0;
  }

  h2 {
    font-size: 1.1rem;
    color: #e0e0e0;
    margin-bottom: 1rem;
  }

  h3 {
    font-size: 0.95rem;
    color: #e0e0e0;
    margin-bottom: 0.75rem;
  }

  .header-controls {
    display: flex;
    gap: 0.5rem;
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

  .btn-secondary {
    background: #333;
    color: #e0e0e0;
    border: none;
  }

  /* ── Loading & Error ──────────────────────────────────── */
  .loading-card {
    background: #1a1a2e;
    padding: 2rem;
    border-radius: 0.75rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    color: #888;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid #333;
    border-top-color: #00ff88;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #ff4444;
  }

  /* ── 1. Dashboard Widget ──────────────────────────────── */
  .dashboard-widget {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .total-rate-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
    margin-bottom: 1.25rem;
  }

  .total-rate-box {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
    text-align: center;
    border-left: 3px solid transparent;
  }
  .total-rate-box.rx { border-left-color: #4dabf7; }
  .total-rate-box.tx { border-left-color: #51cf66; }

  .total-rate-label {
    display: block;
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 0.25rem;
  }

  .total-rate-value {
    display: block;
    font-size: 1.8rem;
    font-weight: 700;
  }
  .rx .total-rate-value { color: #4dabf7; }
  .tx .total-rate-value { color: #51cf66; }

  .total-rate-bits {
    display: block;
    font-size: 0.8rem;
    color: #666;
    margin-top: 0.15rem;
  }

  /* Per-interface traffic bars */
  .iface-bars {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 1.25rem;
  }

  .iface-bar-row {
    display: grid;
    grid-template-columns: 140px 1fr 160px;
    gap: 0.75rem;
    align-items: center;
    padding: 0.4rem 0.6rem;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    cursor: pointer;
    color: #e0e0e0;
    font-size: 0.85rem;
    text-align: left;
    transition: background 0.15s, border-color 0.15s;
  }
  .iface-bar-row:hover { background: #16213e44; }
  .iface-bar-row.selected { background: #0d1a14; border-color: #00ff8844; }

  .bar-label {
    font-family: monospace;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .bar-tracks {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .bar-track {
    height: 10px;
    background: #0f0f23;
    border-radius: 2px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.5s ease;
  }
  .bar-fill.rx { background: linear-gradient(90deg, #4dabf7, #3399ff); }
  .bar-fill.tx { background: linear-gradient(90deg, #51cf66, #33cc66); }

  .bar-rates {
    display: flex;
    gap: 0.75rem;
    font-family: monospace;
    font-size: 0.8rem;
  }

  .bar-rate.rx { color: #4dabf7; }
  .bar-rate.tx { color: #51cf66; }

  /* ── Graph ────────────────────────────────────────────── */
  .graph-wrapper {
    background: #0f0f23;
    border-radius: 0.5rem;
    padding: 0.5rem;
    overflow: hidden;
  }

  .graph-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.25rem;
  }

  .graph-label {
    font-size: 0.75rem;
    color: #888;
    padding: 0.25rem 0.5rem;
  }

  .graph-legend {
    display: flex;
    gap: 1rem;
    padding-right: 0.5rem;
  }

  .legend-item {
    font-size: 0.75rem;
    color: #888;
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .legend-dot {
    width: 10px;
    height: 3px;
    border-radius: 1px;
    display: inline-block;
  }
  .legend-item.rx .legend-dot { background: #4dabf7; }
  .legend-item.tx .legend-dot { background: #51cf66; }

  .traffic-graph {
    width: 100%;
    height: 140px;
    display: block;
  }

  .detail-graph {
    height: 180px;
  }

  /* ── 2. Interface Traffic Cards ───────────────────────── */
  .iface-cards-section {
    margin-bottom: 1.5rem;
  }

  .iface-cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
  }

  .iface-card {
    background: #1a1a2e;
    padding: 1rem;
    border-radius: 0.75rem;
    border: 1px solid transparent;
    cursor: pointer;
    text-align: left;
    color: #e0e0e0;
    transition: border-color 0.15s, background 0.15s;
  }
  .iface-card:hover { background: #16213e44; }
  .iface-card.selected { border-color: #00ff8844; background: #0d1a14; }

  .iface-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.75rem;
  }

  .iface-card-name {
    font-weight: 600;
    font-family: monospace;
    font-size: 0.9rem;
  }

  .state-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .state-dot.up { background: #00ff88; box-shadow: 0 0 6px #00ff8844; }
  .state-dot.down { background: #ff4444; }

  .iface-card-stats {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .card-stat-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.8rem;
  }

  .card-stat-row.rate {
    margin-top: 0.35rem;
    padding-top: 0.35rem;
    border-top: 1px solid #333;
  }

  .card-stat-label {
    color: #888;
  }

  .card-stat-value {
    font-family: monospace;
    font-weight: 600;
    color: #e0e0e0;
  }
  .card-stat-value.rx { color: #4dabf7; }
  .card-stat-value.tx { color: #51cf66; }

  .card-stat-empty {
    color: #555;
    font-size: 0.85rem;
    text-align: center;
    padding: 0.5rem 0;
  }

  .err-warn { color: #ff6666; }

  /* ── Detail Section ───────────────────────────────────── */
  .detail-section {
    margin-bottom: 1.5rem;
  }

  /* ── 4. Bandwidth Monitor ─────────────────────────────── */
  .bandwidth-monitor {
    background: #1a1a2e;
    padding: 1.25rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .bw-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0.75rem;
  }

  .bw-card {
    background: #0f0f23;
    padding: 0.85rem;
    border-radius: 0.5rem;
    text-align: center;
  }

  .bw-label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 0.3rem;
  }

  .bw-value {
    display: block;
    font-size: 1.2rem;
    font-weight: 700;
    font-family: monospace;
  }
  .bw-value.rx { color: #4dabf7; }
  .bw-value.tx { color: #51cf66; }
  .bw-value.peak { font-size: 1.1rem; }

  .bw-bits {
    display: block;
    font-size: 0.7rem;
    color: #666;
    margin-top: 0.15rem;
  }

  /* ── Detail Counters ──────────────────────────────────── */
  .detail-counters {
    background: #1a1a2e;
    padding: 1.25rem;
    border-radius: 0.75rem;
  }

  .counters-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.5rem;
  }

  .counter-cell {
    background: #0f0f23;
    padding: 0.65rem;
    border-radius: 0.4rem;
    text-align: center;
  }

  .counter-label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    text-transform: uppercase;
    margin-bottom: 0.2rem;
  }

  .counter-value {
    display: block;
    font-size: 1.1rem;
    font-weight: 700;
    font-family: monospace;
    color: #e0e0e0;
  }
  .counter-value.rx { color: #4dabf7; }
  .counter-value.tx { color: #51cf66; }

  /* ── Empty ────────────────────────────────────────────── */
  .empty-card {
    background: #1a1a2e;
    padding: 3rem;
    border-radius: 0.75rem;
    text-align: center;
    color: #555;
  }

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 900px) {
    .total-rate-grid { grid-template-columns: 1fr; }
    .iface-bar-row { grid-template-columns: 100px 1fr 120px; }
    .counters-grid { grid-template-columns: repeat(2, 1fr); }
    .bw-grid { grid-template-columns: repeat(2, 1fr); }
  }

  @media (max-width: 600px) {
    .iface-cards-grid { grid-template-columns: 1fr; }
    .iface-bar-row { grid-template-columns: 80px 1fr; }
    .bar-rates { display: none; }
  }
</style>
