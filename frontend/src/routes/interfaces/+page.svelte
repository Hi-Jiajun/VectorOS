<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  interface InterfaceInfo {
    name: string;
    sw_if_index: number;
    state: string;
    mtu: number;
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

  let interfaces: InterfaceInfo[] = [];
  let loading = true;
  let error = '';
  let selectedIface = '';
  let stats: InterfaceStats | null = null;
  let statsLoading = false;
  let message = '';
  let messageType: 'ok' | 'error' = 'ok';

  // Config form state
  let mtuValue = 1500;
  let ipAddValue = '';
  let ipRemoveValue = '';
  let promiscuous = false;

  let statsInterval: ReturnType<typeof setInterval>;

  onMount(async () => {
    await fetchInterfaces();
    // Poll stats every 3 seconds for the selected interface
    statsInterval = setInterval(() => {
      if (selectedIface) fetchStats(selectedIface);
    }, 3000);
  });

  onDestroy(() => {
    if (statsInterval) clearInterval(statsInterval);
  });

  async function fetchInterfaces() {
    try {
      loading = true;
      const res = await fetch('/api/interfaces');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        interfaces = data.interfaces || [];
        if (interfaces.length > 0 && !selectedIface) {
          selectInterface(interfaces[0].name);
        }
      }
    } catch (e) {
      error = 'Failed to fetch interfaces';
    } finally {
      loading = false;
    }
  }

  async function fetchStats(name: string) {
    try {
      const res = await fetch(`/api/interfaces/${name}/stats`);
      const data = await res.json();
      if (data.stats) {
        stats = data.stats;
      }
    } catch (e) {
      // silent - stats polling shouldn't show errors prominently
    }
  }

  function selectInterface(name: string) {
    selectedIface = name;
    const iface = interfaces.find(i => i.name === name);
    if (iface) {
      mtuValue = iface.mtu;
      promiscuous = false;
    }
    ipAddValue = '';
    ipRemoveValue = '';
    stats = null;
    fetchStats(name);
  }

  async function applyConfig() {
    if (!selectedIface) return;
    statsLoading = true;
    message = '';

    const body: Record<string, unknown> = {};
    // Check if MTU changed from current
    const currentIface = interfaces.find(i => i.name === selectedIface);
    if (currentIface && mtuValue !== currentIface.mtu) {
      body.mtu = mtuValue;
    }
    if (ipAddValue.trim()) {
      body.ip_add = ipAddValue.trim();
    }
    if (ipRemoveValue.trim()) {
      body.ip_remove = ipRemoveValue.trim();
    }
    // Only send promiscuous if the user toggled it
    body.promiscuous = promiscuous;

    if (Object.keys(body).length === 0) {
      message = 'No changes to apply';
      messageType = 'error';
      statsLoading = false;
      return;
    }

    try {
      const res = await fetch(`/api/interfaces/${selectedIface}/config`, {
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
      } else {
        const errs = (data.errors || []).join('; ');
        const apps = (data.applied || []).join(', ');
        message = (apps ? 'Applied: ' + apps + '. ' : '') + 'Errors: ' + errs;
        messageType = 'error';
      }
    } catch (e) {
      message = 'Request failed: ' + e;
      messageType = 'error';
    } finally {
      statsLoading = false;
    }
  }

  async function toggleState(name: string, currentState: string) {
    const newState = currentState === 'up' ? 'down' : 'up';
    try {
      const res = await fetch(`/api/interfaces/${name}/${newState}`, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        message = data.error;
        messageType = 'error';
      } else {
        await fetchInterfaces();
      }
    } catch (e) {
      message = 'Failed to toggle interface state';
      messageType = 'error';
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(1) + ' ' + units[i];
  }

  function formatPackets(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
    return n.toString();
  }
</script>

<svelte:head>
  <title>VectorOS - Interface Management</title>
</svelte:head>

<div class="interfaces-page">
  <h1>Interface Management</h1>

  {#if message}
    <div class="toast" class:toast-ok={messageType === 'ok'} class:toast-error={messageType === 'error'}>
      {message}
    </div>
  {/if}

  <!-- Interface list -->
  <div class="card">
    <h2>Interfaces</h2>
    {#if loading}
      <p>Loading...</p>
    {:else if error}
      <p class="error">{error}</p>
    {:else}
      <div class="iface-list">
        {#each interfaces as iface}
          <button
            class="iface-item"
            class:selected={selectedIface === iface.name}
            on:click={() => selectInterface(iface.name)}
          >
            <span class="iface-name">{iface.name}</span>
            <span class="iface-idx">#{iface.sw_if_index}</span>
            <span class="iface-state" class:state-up={iface.state === 'up'} class:state-down={iface.state !== 'up'}>
              {iface.state}
            </span>
            <span class="iface-mtu">MTU {iface.mtu}</span>
            <button class="btn-sm" on:click|stopPropagation={() => toggleState(iface.name, iface.state)}>
              {iface.state === 'up' ? 'Down' : 'Up'}
            </button>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if selectedIface}
    <!-- Statistics panel -->
    <div class="stats-panel card">
      <h2>Traffic Statistics &mdash; {selectedIface}</h2>
      {#if stats}
        <div class="stats-grid">
          <div class="stat-box">
            <span class="stat-label">RX Packets</span>
            <span class="stat-value">{formatPackets(stats.rx_packets)}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">TX Packets</span>
            <span class="stat-value">{formatPackets(stats.tx_packets)}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">RX Bytes</span>
            <span class="stat-value">{formatBytes(stats.rx_bytes)}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">TX Bytes</span>
            <span class="stat-value">{formatBytes(stats.tx_bytes)}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">RX Errors</span>
            <span class="stat-value stat-error">{stats.rx_errors}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">TX Errors</span>
            <span class="stat-value stat-error">{stats.tx_errors}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">RX Drops</span>
            <span class="stat-value stat-drop">{stats.rx_drops}</span>
          </div>
          <div class="stat-box">
            <span class="stat-label">TX Drops</span>
            <span class="stat-value stat-drop">{stats.tx_drops}</span>
          </div>
        </div>
      {:else}
        <p class="muted">Loading statistics...</p>
      {/if}
    </div>

    <!-- Configuration panel -->
    <div class="config-panel card">
      <h2>Configure &mdash; {selectedIface}</h2>

      <div class="form-group">
        <label for="mtu">MTU (packet)</label>
        <div class="form-row-inline">
          <input type="number" id="mtu" bind:value={mtuValue} min="576" max="9216" />
          <span class="hint">Current: {interfaces.find(i => i.name === selectedIface)?.mtu ?? '?'}</span>
        </div>
      </div>

      <div class="form-group">
        <label for="ip-add">Add IP Address</label>
        <div class="form-row-inline">
          <input type="text" id="ip-add" bind:value={ipAddValue} placeholder="192.168.1.1/24" />
        </div>
      </div>

      <div class="form-group">
        <label for="ip-remove">Remove IP Address</label>
        <div class="form-row-inline">
          <input type="text" id="ip-remove" bind:value={ipRemoveValue} placeholder="192.168.1.1/24" />
        </div>
      </div>

      <div class="form-group checkbox-group">
        <label class="toggle-label">
          <input type="checkbox" bind:checked={promiscuous} />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
          Promiscuous Mode
        </label>
      </div>

      <button class="btn-apply" on:click={applyConfig} disabled={statsLoading}>
        {statsLoading ? 'Applying...' : 'Apply Configuration'}
      </button>
    </div>
  {/if}
</div>

<style>
  .interfaces-page {
    max-width: 1000px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
    font-size: 1.1rem;
  }

  .error { color: #ff4444; }
  .muted { color: #666; }

  .toast {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    font-size: 0.9rem;
  }
  .toast-ok { background: #0d3320; color: #00ff88; border: 1px solid #00ff8844; }
  .toast-error { background: #331010; color: #ff6666; border: 1px solid #ff444444; }

  /* Interface list */
  .iface-list { display: flex; flex-direction: column; gap: 0.5rem; }

  .iface-item {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background: #0f0f23;
    border: 1px solid #333;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: border-color 0.2s;
    color: #e0e0e0;
    font-size: 0.95rem;
    text-align: left;
    width: 100%;
  }
  .iface-item:hover { border-color: #00ff8866; }
  .iface-item.selected { border-color: #00ff88; background: #0d1a14; }

  .iface-name { font-weight: 600; min-width: 140px; }
  .iface-idx { color: #666; font-size: 0.85rem; }
  .iface-state { font-size: 0.85rem; font-weight: 600; }
  .state-up { color: #00ff88; }
  .state-down { color: #ff6666; }
  .iface-mtu { color: #888; font-size: 0.85rem; margin-left: auto; }

  .btn-sm {
    background: #16213e;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.3rem 0.75rem;
    border-radius: 0.35rem;
    font-size: 0.8rem;
    cursor: pointer;
  }
  .btn-sm:hover { border-color: #00ff88; }

  /* Stats panel */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.75rem;
  }

  .stat-box {
    background: #0f0f23;
    padding: 0.75rem;
    border-radius: 0.5rem;
    text-align: center;
  }

  .stat-label {
    display: block;
    font-size: 0.75rem;
    color: #888;
    margin-bottom: 0.25rem;
  }

  .stat-value {
    display: block;
    font-size: 1.3rem;
    font-weight: 700;
    color: #00ff88;
  }

  .stat-error { color: #ff6666; }
  .stat-drop { color: #ffaa00; }

  /* Config panel */
  .form-group {
    margin-bottom: 1rem;
  }

  .form-group label {
    display: block;
    font-size: 0.85rem;
    color: #888;
    margin-bottom: 0.4rem;
  }

  .form-row-inline {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .hint { color: #555; font-size: 0.8rem; }

  input[type="text"],
  input[type="number"] {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.6rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.95rem;
    width: 260px;
  }

  input:focus {
    outline: none;
    border-color: #00ff88;
  }

  /* Toggle switch */
  .checkbox-group { margin-top: 0.5rem; }

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
    width: 40px;
    height: 22px;
    background: #333;
    border-radius: 11px;
    position: relative;
    transition: background 0.2s;
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    background: #888;
    border-radius: 50%;
    transition: transform 0.2s, background 0.2s;
  }

  .toggle-label input:checked + .toggle-track {
    background: #00ff8844;
  }

  .toggle-label input:checked + .toggle-track .toggle-thumb {
    transform: translateX(18px);
    background: #00ff88;
  }

  /* Apply button */
  .btn-apply {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.75rem 2rem;
    border-radius: 0.5rem;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    margin-top: 0.5rem;
  }
  .btn-apply:hover { opacity: 0.9; }
  .btn-apply:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
