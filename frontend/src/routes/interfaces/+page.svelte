<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { initWebSocket } from '$lib/stores/websocket';
  import { getWebSocket, type WsMessage, type ConnectionStatus } from '$lib/websocket';
  import type { Unsubscriber } from 'svelte/store';
  import { wsStatus } from '$lib/stores/websocket';

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

  interface TrafficSample {
    rx_bytes: number;
    tx_bytes: number;
    rx_packets: number;
    tx_packets: number;
    ts: number;
  }

  interface RateData {
    rxBytesPerSec: number;
    txBytesPerSec: number;
    rxPacketsPerSec: number;
    txPacketsPerSec: number;
  }

  interface BoundInterface {
    vpp_name: string;
    vf_name: string;
    method: string;
    pci: string;
    bound: boolean;
    sw_if_index: number;
    state: string;
    mtu: number;
  }

  interface AvailableVf {
    vf_name: string;
    pci: string;
    driver: string;
    bound: boolean;
    suggested_vpp_name: string;
  }

  // ── Interface type classification ──────────────────────────────────

  type InterfaceCategory = 'wan' | 'lan' | 'pppoe' | 'mgmt' | 'other';

  function classifyInterface(name: string): InterfaceCategory {
    if (name.startsWith('enp')) return 'wan';
    if (name.startsWith('lan')) return 'lan';
    if (name.startsWith('pppoe')) return 'pppoe';
    if (name === 'ens18') return 'mgmt';
    return 'other';
  }

  function categoryLabel(cat: InterfaceCategory): string {
    switch (cat) {
      case 'wan': return 'WAN';
      case 'lan': return 'LAN';
      case 'pppoe': return 'PPPoE';
      case 'mgmt': return 'Mgmt';
      case 'other': return 'Other';
    }
  }

  function categoryColor(cat: InterfaceCategory): string {
    switch (cat) {
      case 'wan': return '#4dabf7';
      case 'lan': return '#51cf66';
      case 'pppoe': return '#ffd43b';
      case 'mgmt': return '#cc5de8';
      case 'other': return '#868e96';
    }
  }

  // ── State ──────────────────────────────────────────────────────────

  let interfaces: InterfaceInfo[] = [];
  let loading = true;
  let error = '';
  let selectedIface = '';
  let stats: InterfaceStats | null = null;
  let message = '';
  let messageType: 'ok' | 'error' | 'partial' = 'ok';
  let applying = false;

  // Config form
  let mtuValue = 1500;
  let ipAddValue = '';
  let ipRemoveValue = '';
  let promiscuous = false;

  // Binding state
  let boundInterfaces: BoundInterface[] = [];
  let availableVfs: AvailableVf[] = [];
  let bindingInProgress = false;
  let showBindingPanel = false;

  // Bind form
  let bindVfName = '';
  let bindVppName = '';
  let bindMethod: 'rdma' | 'dpdk' = 'rdma';
  let bindPci = '';

  // Traffic rate calculation
  let prevSample: TrafficSample | null = null;
  let currentRates: RateData = { rxBytesPerSec: 0, txBytesPerSec: 0, rxPacketsPerSec: 0, txPacketsPerSec: 0 };

  // Error and drop rates
  interface ErrorRates {
    rxErrorsPerSec: number;
    txErrorsPerSec: number;
    rxDropsPerSec: number;
    txDropsPerSec: number;
    rxLossPercent: number;
    txLossPercent: number;
  }
  let errorRates: ErrorRates = { rxErrorsPerSec: 0, txErrorsPerSec: 0, rxDropsPerSec: 0, txDropsPerSec: 0, rxLossPercent: 0, txLossPercent: 0 };
  let prevErrorSample: { rx_errors: number; tx_errors: number; rx_drops: number; tx_drops: number; rx_packets: number; tx_packets: number; ts: number } | null = null;

  // Traffic graph data (ring buffer of ~60 points = 3 min at 3s intervals)
  const GRAPH_POINTS = 60;
  let rxRateHistory: number[] = [];
  let txRateHistory: number[] = [];
  let graphCanvas: HTMLCanvasElement;
  let graphCtx: CanvasRenderingContext2D | null = null;

  // Interval handles
  let listInterval: ReturnType<typeof setInterval>;
  let statsInterval: ReturnType<typeof setInterval>;
  let unsubWsStatus: Unsubscriber | null = null;
  let unsubWsMessage: (() => void) | null = null;
  let wsConnStatus: ConnectionStatus = 'disconnected';

  // ── Lifecycle ──────────────────────────────────────────────────────

  onMount(async () => {
    initWebSocket();
    unsubWsStatus = wsStatus.subscribe((v) => { wsConnStatus = v; });

    // Subscribe to WS interface updates to refresh state without polling
    const ws = getWebSocket();
    unsubWsMessage = ws.onMessage((msg: WsMessage) => {
      if (msg.type === 'InterfaceUpdate') {
        // Update the matching interface in the list
        const idx = interfaces.findIndex((i) => i.name === msg.name);
        if (idx >= 0) {
          interfaces[idx] = {
            ...interfaces[idx],
            state: msg.state,
          };
          interfaces = [...interfaces]; // trigger reactivity
        }
      }
    });

    await fetchInterfaces();
    await fetchBindings();
    listInterval = setInterval(fetchInterfaces, 3000);
    statsInterval = setInterval(pollStats, 3000);
  });

  onDestroy(() => {
    if (listInterval) clearInterval(listInterval);
    if (statsInterval) clearInterval(statsInterval);
    unsubWsStatus?.();
    unsubWsMessage?.();
  });

  // ── Canvas graph setup ─────────────────────────────────────────────

  $: if (graphCanvas && !graphCtx) {
    graphCtx = graphCanvas.getContext('2d');
  }

  $: if (graphCtx && (rxRateHistory.length > 0 || txRateHistory.length > 0)) {
    drawGraph();
  }

  // ── API helpers ────────────────────────────────────────────────────

  async function fetchInterfaces() {
    try {
      const res = await fetch('/api/interfaces');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        interfaces = data.interfaces || [];
        // If selected interface is gone, deselect it
        if (selectedIface && !interfaces.find((i) => i.name === selectedIface)) {
          selectedIface = '';
          stats = null;
        }
      }
    } catch {
      error = 'Failed to fetch interfaces';
    } finally {
      loading = false;
    }
  }

  async function fetchBindings() {
    try {
      const res = await fetch('/api/interfaces/bound');
      const data = await res.json();
      if (!data.error) {
        boundInterfaces = data.interfaces || [];
        availableVfs = data.available_vfs || [];
      }
    } catch {
      // Binding fetch errors are silent
    }
  }

  async function bindInterface() {
    if (!bindVfName || !bindVppName) {
      message = 'VF name and VPP name are required';
      messageType = 'error';
      return;
    }
    bindingInProgress = true;
    try {
      const body: Record<string, string> = {
        vf_name: bindVfName,
        vpp_name: bindVppName,
        method: bindMethod,
      };
      if (bindPci.trim()) {
        body.pci = bindPci.trim();
      }
      const res = await fetch('/api/interfaces/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (data.error) {
        message = data.error;
        messageType = 'error';
      } else {
        message = data.message || `Bound ${bindVfName} to VPP as ${bindVppName}`;
        messageType = 'ok';
        bindVfName = '';
        bindVppName = '';
        bindPci = '';
        showBindingPanel = false;
        await fetchBindings();
        await fetchInterfaces();
      }
    } catch {
      message = 'Failed to bind interface';
      messageType = 'error';
    } finally {
      bindingInProgress = false;
    }
  }

  async function unbindInterface(vppName: string) {
    bindingInProgress = true;
    try {
      const res = await fetch('/api/interfaces/unbind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ vpp_name: vppName }),
      });
      const data = await res.json();
      if (data.error) {
        message = data.error;
        messageType = 'error';
      } else {
        message = data.message || `Unbound ${vppName} from VPP`;
        messageType = 'ok';
        if (selectedIface === vppName) {
          selectedIface = '';
          stats = null;
        }
        await fetchBindings();
        await fetchInterfaces();
      }
    } catch {
      message = 'Failed to unbind interface';
      messageType = 'error';
    } finally {
      bindingInProgress = false;
    }
  }

  function prefillBind(vf: AvailableVf) {
    bindVfName = vf.vf_name;
    bindVppName = vf.suggested_vpp_name;
    bindPci = vf.pci;
    bindMethod = 'rdma';
    showBindingPanel = true;
  }

  async function fetchStats(name: string) {
    try {
      const res = await fetch(`/api/interfaces/${encodeURIComponent(name)}/stats`);
      const data = await res.json();
      if (data.stats) {
        const newStats: InterfaceStats = data.stats;
        // Calculate rates from previous sample
        if (prevSample) {
          const dt = (Date.now() - prevSample.ts) / 1000;
          if (dt > 0.5) {
            currentRates = {
              rxBytesPerSec: Math.max(0, (newStats.rx_bytes - prevSample.rx_bytes) / dt),
              txBytesPerSec: Math.max(0, (newStats.tx_bytes - prevSample.tx_bytes) / dt),
              rxPacketsPerSec: Math.max(0, (newStats.rx_packets - prevSample.rx_packets) / dt),
              txPacketsPerSec: Math.max(0, (newStats.tx_packets - prevSample.tx_packets) / dt),
            };
            // Push to history
            rxRateHistory = [...rxRateHistory.slice(-(GRAPH_POINTS - 1)), currentRates.rxBytesPerSec];
            txRateHistory = [...txRateHistory.slice(-(GRAPH_POINTS - 1)), currentRates.txBytesPerSec];
          }
        }

        // Calculate error and drop rates
        if (prevErrorSample) {
          const dt = (Date.now() - prevErrorSample.ts) / 1000;
          if (dt > 0.5) {
            const rxDeltas = newStats.rx_packets - prevErrorSample.rx_packets;
            const txDeltas = newStats.tx_packets - prevErrorSample.tx_packets;
            errorRates = {
              rxErrorsPerSec: Math.max(0, (newStats.rx_errors - prevErrorSample.rx_errors) / dt),
              txErrorsPerSec: Math.max(0, (newStats.tx_errors - prevErrorSample.tx_errors) / dt),
              rxDropsPerSec: Math.max(0, (newStats.rx_drops - prevErrorSample.rx_drops) / dt),
              txDropsPerSec: Math.max(0, (newStats.tx_drops - prevErrorSample.tx_drops) / dt),
              rxLossPercent: rxDeltas > 0 ? ((newStats.rx_drops - prevErrorSample.rx_drops) / rxDeltas) * 100 : 0,
              txLossPercent: txDeltas > 0 ? ((newStats.tx_drops - prevErrorSample.tx_drops) / txDeltas) * 100 : 0,
            };
          }
        }

        prevErrorSample = {
          rx_errors: newStats.rx_errors,
          tx_errors: newStats.tx_errors,
          rx_drops: newStats.rx_drops,
          tx_drops: newStats.tx_drops,
          rx_packets: newStats.rx_packets,
          tx_packets: newStats.tx_packets,
          ts: Date.now(),
        };

        prevSample = {
          rx_bytes: newStats.rx_bytes,
          tx_bytes: newStats.tx_bytes,
          rx_packets: newStats.rx_packets,
          tx_packets: newStats.tx_packets,
          ts: Date.now(),
        };
        stats = newStats;
      }
    } catch {
      // Stats polling errors are silent
    }
  }

  async function pollStats() {
    if (selectedIface) {
      await fetchStats(selectedIface);
    }
  }

  function selectInterface(name: string) {
    selectedIface = name;
    const iface = interfaces.find((i) => i.name === name);
    if (iface) {
      mtuValue = iface.mtu;
      promiscuous = false;
    }
    ipAddValue = '';
    ipRemoveValue = '';
    stats = null;
    prevSample = null;
    prevErrorSample = null;
    currentRates = { rxBytesPerSec: 0, txBytesPerSec: 0, rxPacketsPerSec: 0, txPacketsPerSec: 0 };
    errorRates = { rxErrorsPerSec: 0, txErrorsPerSec: 0, rxDropsPerSec: 0, txDropsPerSec: 0, rxLossPercent: 0, txLossPercent: 0 };
    rxRateHistory = [];
    txRateHistory = [];
    fetchStats(name);
  }

  // ── Configuration actions ──────────────────────────────────────────

  async function applyConfig() {
    if (!selectedIface) return;
    applying = true;
    message = '';

    const body: Record<string, unknown> = {};
    const currentIface = interfaces.find((i) => i.name === selectedIface);
    if (currentIface && mtuValue !== currentIface.mtu) {
      body.mtu = mtuValue;
    }
    if (ipAddValue.trim()) {
      body.ip_add = ipAddValue.trim();
    }
    if (ipRemoveValue.trim()) {
      body.ip_remove = ipRemoveValue.trim();
    }
    body.promiscuous = promiscuous;

    if (Object.keys(body).length === 0) {
      message = 'No changes to apply';
      messageType = 'partial';
      applying = false;
      return;
    }

    try {
      const res = await fetch(`/api/interfaces/${encodeURIComponent(selectedIface)}/config`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (data.status === 'ok') {
        message = 'Applied: ' + (data.applied || []).join(', ');
        messageType = 'ok';
        ipAddValue = '';
        ipRemoveValue = '';
        await fetchInterfaces();
        await fetchStats(selectedIface);
      } else if (data.status === 'partial') {
        const errs = (data.errors || []).join('; ');
        const apps = (data.applied || []).join(', ');
        message = (apps ? 'Applied: ' + apps + '. ' : '') + 'Errors: ' + errs;
        messageType = 'partial';
        ipAddValue = '';
        ipRemoveValue = '';
        await fetchInterfaces();
      } else {
        const errs = (data.errors || []).join('; ');
        message = 'Errors: ' + errs;
        messageType = 'error';
      }
    } catch (e) {
      message = 'Request failed: ' + e;
      messageType = 'error';
    } finally {
      applying = false;
    }
  }

  async function toggleState(name: string, currentState: string) {
    const newState = currentState === 'up' ? 'down' : 'up';
    try {
      const res = await fetch(`/api/interfaces/${encodeURIComponent(name)}/${newState}`, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        message = data.error;
        messageType = 'error';
      } else {
        message = `Interface ${name} set to ${newState}`;
        messageType = 'ok';
        await fetchInterfaces();
        if (selectedIface === name) {
          await fetchStats(name);
        }
      }
    } catch {
      message = 'Failed to toggle interface state';
      messageType = 'error';
    }
  }

  // ── Formatting ─────────────────────────────────────────────────────

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
  }

  function formatRate(bytesPerSec: number): string {
    if (bytesPerSec === 0) return '0 B/s';
    if (bytesPerSec >= 1024 * 1024 * 1024) return (bytesPerSec / (1024 * 1024 * 1024)).toFixed(2) + ' GB/s';
    if (bytesPerSec >= 1024 * 1024) return (bytesPerSec / (1024 * 1024)).toFixed(2) + ' MB/s';
    if (bytesPerSec >= 1024) return (bytesPerSec / 1024).toFixed(1) + ' KB/s';
    return bytesPerSec.toFixed(0) + ' B/s';
  }

  function formatPps(packetsPerSec: number): string {
    if (packetsPerSec >= 1_000_000) return (packetsPerSec / 1_000_000).toFixed(2) + ' Mpps';
    if (packetsPerSec >= 1_000) return (packetsPerSec / 1_000).toFixed(1) + ' Kpps';
    return packetsPerSec.toFixed(0) + ' pps';
  }

  function formatTotalPackets(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
    return n.toString();
  }

  function hasErrors(): boolean {
    return !!stats && (stats.rx_errors > 0 || stats.tx_errors > 0 || stats.rx_drops > 0 || stats.tx_drops > 0);
  }

  // ── Canvas graph drawing ───────────────────────────────────────────

  function drawGraph() {
    if (!graphCtx || !graphCanvas) return;
    const ctx = graphCtx;
    const w = graphCanvas.width;
    const h = graphCanvas.height;

    // Background
    ctx.fillStyle = '#0f0f23';
    ctx.fillRect(0, 0, w, h);

    // Grid lines
    const gridCount = 5;
    ctx.strokeStyle = '#1a1a2e';
    ctx.lineWidth = 1;
    for (let i = 1; i < gridCount; i++) {
      const y = (h / gridCount) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    // Find max value for scale
    const allValues = [...rxRateHistory, ...txRateHistory];
    const maxVal = Math.max(1, ...allValues) * 1.15;

    // Helper: draw a line series
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

    drawLine(rxRateHistory, 'rgba(77, 171, 247, 1)');
    drawLine(txRateHistory, 'rgba(81, 207, 102, 1)');

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
    ctx.fillText(formatRate(maxVal), w - 8, legendY);
    ctx.textAlign = 'left';
  }

  // ── Derived helpers for grouping interfaces ────────────────────────

  $: groupedInterfaces = interfaces.reduce<Record<InterfaceCategory, InterfaceInfo[]>>(
    (acc, iface) => {
      const cat = classifyInterface(iface.name);
      acc[cat].push(iface);
      return acc;
    },
    { wan: [], lan: [], pppoe: [], mgmt: [], other: [] }
  );

  $: hasNonEmptyGroups = Object.values(groupedInterfaces).some((g) => g.length > 0);
  $: selectedIfaceData = interfaces.find((i) => i.name === selectedIface) || null;
</script>

<svelte:head>
  <title>VectorOS - Interface Management</title>
</svelte:head>

<div class="interfaces-page">
  <div class="header-row">
    <h1>Interface Management</h1>
    <div class="ws-indicator">
      <span class="ws-dot" style="background: {wsConnStatus === 'connected' ? '#00ff88' : '#666'}"></span>
      <span class="ws-text">{wsConnStatus}</span>
    </div>
  </div>

  <!-- Toast messages -->
  {#if message}
    <div
      class="toast"
      class:toast-ok={messageType === 'ok'}
      class:toast-error={messageType === 'error'}
      class:toast-partial={messageType === 'partial'}
      on:click={() => { message = ''; }}
      role="button"
      tabindex="0"
    >
      <span>{message}</span>
      <span class="toast-close">&times;</span>
    </div>
  {/if}

  <!-- ── VF Binding Panel ── -->
  <div class="card binding-panel">
    <div class="card-header" style="cursor: pointer" on:click={() => showBindingPanel = !showBindingPanel}>
      <h2>
        VF Interface Binding
        <span class="binding-count">
          {boundInterfaces.length} bound
          {#if availableVfs.length > 0}
            &middot; {availableVfs.length} available
          {/if}
        </span>
      </h2>
      <span class="expand-icon">{showBindingPanel ? '&#9650;' : '&#9660;'}</span>
    </div>

    {#if showBindingPanel}
      <div class="binding-content">
        <!-- Bound interfaces -->
        {#if boundInterfaces.length > 0}
          <div class="binding-section">
            <div class="section-label">Bound Interfaces</div>
            {#each boundInterfaces as bi}
              <div class="binding-row">
                <div class="binding-info">
                  <span class="binding-vpp">{bi.vpp_name}</span>
                  <span class="binding-arrow">&larr;</span>
                  <span class="binding-vf">{bi.vf_name}</span>
                  <span class="binding-meta">({bi.method}{bi.pci ? `, ${bi.pci}` : ''})</span>
                </div>
                <div class="binding-actions">
                  <span class="binding-state" class:state-up={bi.state === 'up'} class:state-down={bi.state !== 'up'}>
                    {bi.state || '?'}
                  </span>
                  <button
                    class="btn-unbind"
                    on:click|stopPropagation={() => unbindInterface(bi.vpp_name)}
                    disabled={bindingInProgress}
                    title="Unbind from VPP"
                  >
                    Unbind
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Available VFs -->
        {#if availableVfs.length > 0}
          <div class="binding-section">
            <div class="section-label">Available VF Interfaces</div>
            {#each availableVfs as vf}
              <div class="binding-row vf-available">
                <div class="binding-info">
                  <span class="binding-vf">{vf.vf_name}</span>
                  {#if vf.pci}
                    <span class="binding-meta">({vf.pci})</span>
                  {/if}
                  {#if vf.driver}
                    <span class="binding-driver">{vf.driver}</span>
                  {/if}
                </div>
                <button
                  class="btn-bind"
                  on:click|stopPropagation={() => prefillBind(vf)}
                  disabled={bindingInProgress}
                  title="Bind to VPP"
                >
                  Bind to VPP
                </button>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Manual bind form -->
        <div class="binding-section">
          <div class="section-label">Manual Binding</div>
          <div class="bind-form">
            <div class="bind-form-row">
              <div class="form-group">
                <label for="bind-vf">VF Interface</label>
                <input type="text" id="bind-vf" bind:value={bindVfName} placeholder="enp1s0" />
              </div>
              <div class="form-group">
                <label for="bind-vpp">VPP Name</label>
                <input type="text" id="bind-vpp" bind:value={bindVppName} placeholder="wan0" />
              </div>
              <div class="form-group">
                <label for="bind-method">Method</label>
                <select id="bind-method" bind:value={bindMethod}>
                  <option value="rdma">RDMA (no driver change)</option>
                  <option value="dpdk">DPDK (vfio-pci)</option>
                </select>
              </div>
              {#if bindMethod === 'dpdk'}
                <div class="form-group">
                  <label for="bind-pci">PCI Address</label>
                  <input type="text" id="bind-pci" bind:value={bindPci} placeholder="0000:01:00.0" />
                </div>
              {/if}
              <button
                class="btn-apply bind-btn"
                on:click={bindInterface}
                disabled={bindingInProgress || !bindVfName || !bindVppName}
              >
                {#if bindingInProgress}
                  <span class="btn-spinner"></span> Binding...
                {:else}
                  Bind
                {/if}
              </button>
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <div class="two-col">
    <!-- ── Interface List ── -->
    <div class="col-left">
      <div class="card">
        <div class="card-header">
          <h2>Interfaces</h2>
          <span class="badge">{interfaces.length}</span>
        </div>
        {#if loading && interfaces.length === 0}
          <div class="loading-row">
            <div class="spinner"></div>
            <span>Loading interfaces...</span>
          </div>
        {:else if error}
          <p class="error">{error}</p>
        {:else if !hasNonEmptyGroups}
          <p class="muted">No interfaces found</p>
        {:else}
          <div class="iface-groups">
            {#each (['wan', 'lan', 'pppoe', 'mgmt', 'other'] as InterfaceCategory[]) as cat}
              {#if groupedInterfaces[cat].length > 0}
                <div class="group-label">
                  <span class="group-dot" style="background: {categoryColor(cat)}"></span>
                  {categoryLabel(cat)}
                </div>
                {#each groupedInterfaces[cat] as iface}
                  <button
                    class="iface-row"
                    class:selected={selectedIface === iface.name}
                    on:click={() => selectInterface(iface.name)}
                  >
                    <div class="iface-left">
                      <span class="status-dot" class:up={iface.state === 'up'} class:down={iface.state !== 'up'}></span>
                      <div class="iface-info">
                        <div class="iface-name-row">
                          <span class="iface-name">{iface.name}</span>
                          {#if iface.interface_type}
                            <span class="iface-type-badge">{iface.interface_type}</span>
                          {/if}
                        </div>
                        <span class="iface-meta">
                          idx {iface.sw_if_index} &middot; MTU {iface.mtu}
                          {#if iface.mac_address}
                            &middot; {iface.mac_address}
                          {/if}
                        </span>
                        {#if iface.ip_addresses && iface.ip_addresses.length > 0}
                          <span class="iface-ip">{iface.ip_addresses.join(', ')}</span>
                        {/if}
                      </div>
                    </div>
                    <div class="iface-right">
                      <span class="state-badge" class:state-up={iface.state === 'up'} class:state-down={iface.state !== 'up'}>
                        {iface.state}
                      </span>
                      <button
                        class="btn-toggle"
                        title={iface.state === 'up' ? 'Bring down' : 'Bring up'}
                        on:click|stopPropagation={() => toggleState(iface.name, iface.state)}
                      >
                        {iface.state === 'up' ? '⏻' : '⏻'}
                      </button>
                    </div>
                  </button>
                {/each}
              {/if}
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <!-- ── Right panel (stats + config) ── -->
    <div class="col-right">
      {#if selectedIface}
        <!-- Interface Info Overview -->
        {#if selectedIfaceData}
          <div class="card info-card">
            <div class="card-header">
              <h2>Interface Info</h2>
              <span class="category-tag" style="color: {categoryColor(classifyInterface(selectedIface))}; border-color: {categoryColor(classifyInterface(selectedIface))}44">
                {categoryLabel(classifyInterface(selectedIface))}
              </span>
            </div>
            <div class="info-grid">
              <div class="info-item">
                <span class="info-label">Name</span>
                <span class="info-value">{selectedIfaceData.name}</span>
              </div>
              <div class="info-item">
                <span class="info-label">State</span>
                <span class="info-value">
                  <span class="state-badge" class:state-up={selectedIfaceData.state === 'up'} class:state-down={selectedIfaceData.state !== 'up'}>
                    {selectedIfaceData.state}
                  </span>
                </span>
              </div>
              <div class="info-item">
                <span class="info-label">Index</span>
                <span class="info-value">{selectedIfaceData.sw_if_index}</span>
              </div>
              <div class="info-item">
                <span class="info-label">MTU</span>
                <span class="info-value">{selectedIfaceData.mtu}</span>
              </div>
              {#if selectedIfaceData.interface_type}
                <div class="info-item">
                  <span class="info-label">Type</span>
                  <span class="info-value type-badge">{selectedIfaceData.interface_type}</span>
                </div>
              {/if}
              {#if selectedIfaceData.mac_address}
                <div class="info-item">
                  <span class="info-label">MAC Address</span>
                  <span class="info-value mac">{selectedIfaceData.mac_address}</span>
                </div>
              {/if}
              {#if selectedIfaceData.ip_addresses && selectedIfaceData.ip_addresses.length > 0}
                <div class="info-item info-wide">
                  <span class="info-label">IP Addresses</span>
                  <span class="info-value">
                    {#each selectedIfaceData.ip_addresses as ip, i}
                      <span class="ip-tag">{ip}</span>{#if i < selectedIfaceData.ip_addresses!.length - 1}{' '}{/if}
                    {/each}
                  </span>
                </div>
              {:else}
                <div class="info-item info-wide">
                  <span class="info-label">IP Addresses</span>
                  <span class="info-value no-ip">No IP assigned</span>
                </div>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Statistics -->
        <div class="card stats-card">
          <div class="card-header">
            <h2>Traffic &mdash; {selectedIface}</h2>
            <span class="category-tag" style="color: {categoryColor(classifyInterface(selectedIface))}; border-color: {categoryColor(classifyInterface(selectedIface))}44">
              {categoryLabel(classifyInterface(selectedIface))}
            </span>
          </div>

          <!-- Rate cards -->
          <div class="rate-grid">
            <div class="rate-box rx">
              <span class="rate-label">RX Rate</span>
              <span class="rate-value">{formatRate(currentRates.rxBytesPerSec)}</span>
              <span class="rate-pps">{formatPps(currentRates.rxPacketsPerSec)}</span>
            </div>
            <div class="rate-box tx">
              <span class="rate-label">TX Rate</span>
              <span class="rate-value">{formatRate(currentRates.txBytesPerSec)}</span>
              <span class="rate-pps">{formatPps(currentRates.txPacketsPerSec)}</span>
            </div>
          </div>

          <!-- Graph -->
          <div class="graph-wrapper">
            <canvas bind:this={graphCanvas} width={520} height={140} class="traffic-graph"></canvas>
          </div>

          <!-- Cumulative counters -->
          {#if stats}
            <div class="stats-grid">
              <div class="stat-cell">
                <span class="stat-label">RX Packets</span>
                <span class="stat-value rx">{formatTotalPackets(stats.rx_packets)}</span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">TX Packets</span>
                <span class="stat-value tx">{formatTotalPackets(stats.tx_packets)}</span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">RX Bytes</span>
                <span class="stat-value rx">{formatBytes(stats.rx_bytes)}</span>
              </div>
              <div class="stat-cell">
                <span class="stat-label">TX Bytes</span>
                <span class="stat-value tx">{formatBytes(stats.tx_bytes)}</span>
              </div>
            </div>

            {#if hasErrors()}
              <div class="error-panel">
                <span class="error-icon">!</span>
                <div class="error-counters">
                  <span class="err-item">RX Errors: <strong>{stats.rx_errors}</strong></span>
                  <span class="err-item">TX Errors: <strong>{stats.tx_errors}</strong></span>
                  <span class="err-item">RX Drops: <strong>{stats.rx_drops}</strong></span>
                  <span class="err-item">TX Drops: <strong>{stats.tx_drops}</strong></span>
                </div>
              </div>

              <!-- Error and loss rates -->
              <div class="rate-detail-grid">
                <div class="rate-detail-item">
                  <span class="rate-detail-label">RX Error Rate</span>
                  <span class="rate-detail-value err">{errorRates.rxErrorsPerSec > 0 ? formatPps(errorRates.rxErrorsPerSec) : '0 pps'}</span>
                </div>
                <div class="rate-detail-item">
                  <span class="rate-detail-label">TX Error Rate</span>
                  <span class="rate-detail-value err">{errorRates.txErrorsPerSec > 0 ? formatPps(errorRates.txErrorsPerSec) : '0 pps'}</span>
                </div>
                <div class="rate-detail-item">
                  <span class="rate-detail-label">RX Drop Rate</span>
                  <span class="rate-detail-value warn">{errorRates.rxDropsPerSec > 0 ? formatPps(errorRates.rxDropsPerSec) : '0 pps'}</span>
                </div>
                <div class="rate-detail-item">
                  <span class="rate-detail-label">TX Drop Rate</span>
                  <span class="rate-detail-value warn">{errorRates.txDropsPerSec > 0 ? formatPps(errorRates.txDropsPerSec) : '0 pps'}</span>
                </div>
                {#if errorRates.rxLossPercent > 0 || errorRates.txLossPercent > 0}
                  <div class="rate-detail-item">
                    <span class="rate-detail-label">RX Packet Loss</span>
                    <span class="rate-detail-value" class:warn={errorRates.rxLossPercent > 1} class:crit={errorRates.rxLossPercent > 10}>
                      {errorRates.rxLossPercent.toFixed(2)}%
                    </span>
                  </div>
                  <div class="rate-detail-item">
                    <span class="rate-detail-label">TX Packet Loss</span>
                    <span class="rate-detail-value" class:warn={errorRates.txLossPercent > 1} class:crit={errorRates.txLossPercent > 10}>
                      {errorRates.txLossPercent.toFixed(2)}%
                    </span>
                  </div>
                {/if}
              </div>
            {/if}
          {:else}
            <p class="muted">Collecting statistics...</p>
          {/if}
        </div>

        <!-- Configuration -->
        <div class="card config-card">
          <div class="card-header">
            <h2>Configure &mdash; {selectedIface}</h2>
          </div>

          <div class="form-grid">
            <!-- MTU -->
            <div class="form-group">
              <label for="mtu-input">MTU</label>
              <div class="form-row">
                <input
                  type="number"
                  id="mtu-input"
                  bind:value={mtuValue}
                  min="576"
                  max="9216"
                  step="1"
                />
                <span class="form-hint">Current: {interfaces.find((i) => i.name === selectedIface)?.mtu ?? '?'}</span>
              </div>
            </div>

            <!-- Add IP -->
            <div class="form-group">
              <label for="ip-add-input">Add IP Address</label>
              <div class="form-row">
                <input
                  type="text"
                  id="ip-add-input"
                  bind:value={ipAddValue}
                  placeholder="192.168.1.1/24"
                />
              </div>
            </div>

            <!-- Remove IP -->
            <div class="form-group">
              <label for="ip-remove-input">Remove IP Address</label>
              <div class="form-row">
                <input
                  type="text"
                  id="ip-remove-input"
                  bind:value={ipRemoveValue}
                  placeholder="192.168.1.1/24"
                />
              </div>
            </div>

            <!-- Promiscuous toggle -->
            <div class="form-group toggle-group">
              <label class="toggle-label">
                <input type="checkbox" bind:checked={promiscuous} />
                <span class="toggle-track">
                  <span class="toggle-thumb"></span>
                </span>
                Promiscuous Mode
              </label>
            </div>

            <!-- Actions -->
            <div class="form-actions">
              <button class="btn-apply" on:click={applyConfig} disabled={applying}>
                {#if applying}
                  <span class="btn-spinner"></span> Applying...
                {:else}
                  Apply Configuration
                {/if}
              </button>
              <button class="btn-secondary" on:click={() => toggleState(selectedIface, interfaces.find((i) => i.name === selectedIface)?.state ?? 'down')}>
                {interfaces.find((i) => i.name === selectedIface)?.state === 'up' ? 'Bring Down' : 'Bring Up'}
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div class="card empty-state">
          <div class="empty-icon">&#9881;</div>
          <p>Select an interface to view statistics and configure</p>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* ── Page layout ──────────────────────────────────────── */
  .interfaces-page {
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

  .ws-indicator {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.8rem;
    background: #1a1a2e;
    border-radius: 0.4rem;
    font-size: 0.8rem;
  }

  .ws-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }

  .ws-text {
    color: #888;
    text-transform: capitalize;
  }

  /* ── Toast ────────────────────────────────────────────── */
  .toast {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    font-size: 0.9rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    animation: fadeIn 0.2s ease;
  }
  .toast-ok { background: #0d3320; color: #00ff88; border: 1px solid #00ff8844; }
  .toast-error { background: #331010; color: #ff6666; border: 1px solid #ff444444; }
  .toast-partial { background: #2d2200; color: #ffaa00; border: 1px solid #ffaa0044; }
  .toast-close { font-size: 1.2rem; opacity: 0.6; }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Two-column layout ────────────────────────────────── */
  .two-col {
    display: grid;
    grid-template-columns: 380px 1fr;
    gap: 1.5rem;
    align-items: start;
  }

  .col-left, .col-right {
    min-width: 0;
  }

  /* ── Cards ────────────────────────────────────────────── */
  .card {
    background: #1a1a2e;
    padding: 1.25rem;
    border-radius: 0.75rem;
    margin-bottom: 1.25rem;
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  .card-header h2 {
    margin: 0;
    font-size: 1rem;
    color: #e0e0e0;
  }

  .badge {
    background: #16213e;
    color: #888;
    padding: 0.15rem 0.5rem;
    border-radius: 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .error { color: #ff4444; font-size: 0.9rem; }
  .muted { color: #666; font-size: 0.9rem; }

  /* ── Loading ──────────────────────────────────────────── */
  .loading-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: #888;
    font-size: 0.9rem;
    padding: 1rem 0;
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

  /* ── Interface groups ─────────────────────────────────── */
  .iface-groups {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .group-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #888;
    padding: 0.75rem 0.5rem 0.35rem;
    border-top: 1px solid #222;
  }

  .group-label:first-child {
    border-top: none;
    padding-top: 0.25rem;
  }

  .group-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: inline-block;
  }

  /* ── Interface row ────────────────────────────────────── */
  .iface-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    width: 100%;
    padding: 0.6rem 0.75rem;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 0.5rem;
    cursor: pointer;
    color: #e0e0e0;
    font-size: 0.9rem;
    text-align: left;
    transition: background 0.15s, border-color 0.15s;
    border: none;
  }
  .iface-row:hover { background: #16213e44; }
  .iface-row.selected {
    background: #0d1a14;
    border: 1px solid #00ff8844;
  }

  .iface-left {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status-dot.up { background: #00ff88; box-shadow: 0 0 6px #00ff8844; }
  .status-dot.down { background: #ff4444; }

  .iface-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .iface-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .iface-meta {
    font-size: 0.75rem;
    color: #666;
  }

  .iface-right {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .state-badge {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    padding: 0.15rem 0.45rem;
    border-radius: 0.25rem;
    letter-spacing: 0.04em;
  }
  .state-up { background: #0d3320; color: #00ff88; }
  .state-down { background: #331010; color: #ff6666; }

  .btn-toggle {
    background: none;
    border: 1px solid #333;
    color: #888;
    padding: 0.2rem 0.4rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .btn-toggle:hover {
    color: #00ff88;
    border-color: #00ff88;
  }

  /* ── Stats: rate cards ────────────────────────────────── */
  .rate-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .rate-box {
    background: #0f0f23;
    padding: 0.85rem;
    border-radius: 0.5rem;
    text-align: center;
    border-left: 3px solid transparent;
  }
  .rate-box.rx { border-left-color: #4dabf7; }
  .rate-box.tx { border-left-color: #51cf66; }

  .rate-label {
    display: block;
    font-size: 0.75rem;
    color: #888;
    margin-bottom: 0.25rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .rate-value {
    display: block;
    font-size: 1.4rem;
    font-weight: 700;
    color: #e0e0e0;
  }
  .rx .rate-value { color: #4dabf7; }
  .tx .rate-value { color: #51cf66; }

  .rate-pps {
    display: block;
    font-size: 0.8rem;
    color: #666;
    margin-top: 0.15rem;
  }

  /* ── Graph ────────────────────────────────────────────── */
  .graph-wrapper {
    background: #0f0f23;
    border-radius: 0.5rem;
    padding: 0.5rem;
    margin-bottom: 1rem;
    overflow: hidden;
  }

  .traffic-graph {
    width: 100%;
    height: 140px;
    display: block;
  }

  /* ── Stats grid ───────────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .stat-cell {
    background: #0f0f23;
    padding: 0.65rem;
    border-radius: 0.4rem;
    text-align: center;
  }

  .stat-label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    margin-bottom: 0.2rem;
    text-transform: uppercase;
  }

  .stat-value {
    display: block;
    font-size: 1.1rem;
    font-weight: 700;
  }
  .stat-value.rx { color: #4dabf7; }
  .stat-value.tx { color: #51cf66; }

  /* ── Error panel ──────────────────────────────────────── */
  .error-panel {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    background: #331010;
    border: 1px solid #ff444444;
    border-radius: 0.5rem;
    padding: 0.65rem 0.85rem;
    margin-top: 0.75rem;
    animation: fadeIn 0.3s ease;
  }

  .error-icon {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: #ff4444;
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.8rem;
    font-weight: 700;
    flex-shrink: 0;
  }

  .error-counters {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
  }

  .err-item {
    font-size: 0.85rem;
    color: #ff8888;
  }
  .err-item strong {
    color: #ff6666;
  }

  /* ── Config form ──────────────────────────────────────── */
  .config-card {
    border-top: 2px solid #00ff8833;
  }

  .form-grid {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form-group label {
    display: block;
    font-size: 0.8rem;
    color: #888;
    margin-bottom: 0.35rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .form-row input {
    flex: 1;
    max-width: 240px;
  }

  .form-hint {
    font-size: 0.8rem;
    color: #555;
    white-space: nowrap;
  }

  /* Toggle switch */
  .toggle-group { margin-top: 0.25rem; }

  .toggle-label {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    cursor: pointer;
    font-size: 0.9rem;
    color: #e0e0e0;
  }

  .toggle-label input { display: none; }

  .toggle-track {
    width: 38px;
    height: 20px;
    background: #333;
    border-radius: 10px;
    position: relative;
    transition: background 0.2s;
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: #888;
    border-radius: 50%;
    transition: transform 0.2s, background 0.2s;
  }

  .toggle-label input:checked + .toggle-track { background: #00ff8844; }
  .toggle-label input:checked + .toggle-track .toggle-thumb {
    transform: translateX(18px);
    background: #00ff88;
  }

  /* Action buttons */
  .form-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 0.25rem;
  }

  .btn-apply {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.65rem 1.5rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .btn-apply:hover { opacity: 0.9; }
  .btn-apply:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid #0f0f2333;
    border-top-color: #0f0f23;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    display: inline-block;
  }

  .btn-secondary {
    background: #16213e;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.65rem 1.5rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
  }
  .btn-secondary:hover { border-color: #00ff88; color: #00ff88; }

  /* ── Empty state ──────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 4rem 2rem;
    color: #555;
    text-align: center;
  }

  .empty-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.3;
  }

  /* ── Category tag ─────────────────────────────────────── */
  .category-tag {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    border: 1px solid;
    letter-spacing: 0.05em;
  }

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 900px) {
    .two-col {
      grid-template-columns: 1fr;
    }
    .stats-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  /* ── Binding Panel ────────────────────────────────────── */
  .binding-panel {
    border-top: 2px solid #4dabf733;
  }

  .binding-count {
    font-size: 0.75rem;
    font-weight: 400;
    color: #888;
    margin-left: 0.5rem;
  }

  .expand-icon {
    font-size: 0.8rem;
    color: #888;
  }

  .binding-content {
    margin-top: 0.5rem;
  }

  .binding-section {
    margin-bottom: 1rem;
  }

  .binding-section:last-child {
    margin-bottom: 0;
  }

  .section-label {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #888;
    margin-bottom: 0.5rem;
  }

  .binding-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    background: #0f0f23;
    border-radius: 0.4rem;
    margin-bottom: 0.4rem;
  }

  .binding-row.vf-available {
    border: 1px dashed #333;
    background: transparent;
  }

  .binding-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
  }

  .binding-vpp {
    color: #00ff88;
    font-weight: 600;
  }

  .binding-arrow {
    color: #555;
  }

  .binding-vf {
    color: #4dabf7;
  }

  .binding-meta {
    color: #666;
    font-size: 0.75rem;
  }

  .binding-driver {
    color: #888;
    font-size: 0.7rem;
    background: #16213e;
    padding: 0.1rem 0.4rem;
    border-radius: 0.2rem;
  }

  .binding-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .binding-state {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    padding: 0.1rem 0.4rem;
    border-radius: 0.2rem;
  }
  .binding-state.state-up { background: #0d3320; color: #00ff88; }
  .binding-state.state-down { background: #331010; color: #ff6666; }

  .btn-unbind {
    background: #331010;
    color: #ff6666;
    border: 1px solid #ff444444;
    padding: 0.25rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.75rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .btn-unbind:hover { background: #441818; border-color: #ff4444; }
  .btn-unbind:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-bind {
    background: #0d3320;
    color: #00ff88;
    border: 1px solid #00ff8844;
    padding: 0.25rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.75rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .btn-bind:hover { background: #0d4428; border-color: #00ff88; }
  .btn-bind:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ── Bind Form ────────────────────────────────────────── */
  .bind-form {
    background: #0f0f23;
    padding: 0.75rem;
    border-radius: 0.4rem;
  }

  .bind-form-row {
    display: flex;
    align-items: flex-end;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .bind-form-row .form-group {
    flex: 1;
    min-width: 120px;
  }

  .bind-form-row .form-group label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    margin-bottom: 0.25rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .bind-form-row .form-group input,
  .bind-form-row .form-group select {
    width: 100%;
    padding: 0.4rem 0.6rem;
    background: #1a1a2e;
    border: 1px solid #333;
    border-radius: 0.3rem;
    color: #e0e0e0;
    font-size: 0.85rem;
    box-sizing: border-box;
  }

  .bind-form-row .form-group input:focus,
  .bind-form-row .form-group select:focus {
    outline: none;
    border-color: #00ff88;
  }

  .bind-btn {
    flex-shrink: 0;
    margin-bottom: 0;
  }

  /* ── Interface Info Card ───────────────────────────────── */
  .info-card {
    border-top: 2px solid #4dabf733;
  }

  .info-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.65rem;
  }

  .info-item {
    background: #0f0f23;
    padding: 0.6rem 0.75rem;
    border-radius: 0.4rem;
  }

  .info-item.info-wide {
    grid-column: 1 / -1;
  }

  .info-label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 0.2rem;
  }

  .info-value {
    display: block;
    font-size: 0.9rem;
    color: #e0e0e0;
    font-weight: 500;
  }

  .info-value.mac {
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.85rem;
    color: #4dabf7;
  }

  .info-value.no-ip {
    color: #666;
    font-style: italic;
    font-weight: 400;
  }

  .type-badge {
    display: inline-block;
    background: #16213e;
    color: #888;
    padding: 0.1rem 0.5rem;
    border-radius: 0.2rem;
    font-size: 0.8rem;
    text-transform: capitalize;
  }

  .ip-tag {
    display: inline-block;
    background: #16213e;
    color: #51cf66;
    padding: 0.15rem 0.5rem;
    border-radius: 0.2rem;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 0.8rem;
    margin-right: 0.3rem;
  }

  /* ── Interface list additions ──────────────────────────── */
  .iface-name-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .iface-type-badge {
    font-size: 0.6rem;
    font-weight: 600;
    text-transform: uppercase;
    background: #16213e;
    color: #888;
    padding: 0.05rem 0.3rem;
    border-radius: 0.15rem;
    letter-spacing: 0.03em;
  }

  .iface-ip {
    font-size: 0.7rem;
    color: #51cf66;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  /* ── Error and loss rate detail ────────────────────────── */
  .rate-detail-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  .rate-detail-item {
    background: #0f0f23;
    padding: 0.5rem 0.65rem;
    border-radius: 0.35rem;
    text-align: center;
  }

  .rate-detail-label {
    display: block;
    font-size: 0.65rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    margin-bottom: 0.15rem;
  }

  .rate-detail-value {
    display: block;
    font-size: 0.9rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  .rate-detail-value.err {
    color: #ff6666;
  }

  .rate-detail-value.warn {
    color: #ffaa00;
  }

  .rate-detail-value.crit {
    color: #ff4444;
    animation: blink 1s ease-in-out infinite;
  }

  @keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
</style>
