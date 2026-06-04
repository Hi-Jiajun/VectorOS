<script lang="ts">
  import { onMount } from 'svelte';

  let pppoeStatus: any = null;
  let loading = true;
  let error = '';

  // Auto-connect state
  let autoStatus: any = null;
  let autoLoading = true;

  // PPPoE configuration form
  let config = {
    username: '',
    password: '',
    interface: 'enp1s0',
    acName: '',
    serviceName: '',
    mtu: 1492,
    mru: 1492,
    usePeerDns: true,
    addDefaultRoute4: true,
    addDefaultRoute6: true
  };

  // Auto-connect configuration form
  let autoConfig = {
    enabled: false,
    max_retries: 0,
    retry_interval: 5,
    backoff_factor: 2.0,
    max_retry_interval: 300,
    check_interval: 10,
    health_check_interval: 60
  };

  onMount(async () => {
    await Promise.all([fetchPppoeStatus(), fetchAutoStatus()]);
  });

  async function fetchPppoeStatus() {
    try {
      loading = true;
      const res = await fetch('/api/pppoe/clients');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        pppoeStatus = data;
      }
    } catch (e) {
      error = 'Failed to fetch PPPoE status';
    } finally {
      loading = false;
    }
  }

  async function fetchAutoStatus() {
    try {
      autoLoading = true;
      const res = await fetch('/api/pppoe/autoconnect/status');
      autoStatus = await res.json();
      if (autoStatus.config) {
        autoConfig = { ...autoConfig, ...autoStatus.config };
      }
    } catch (e) {
      console.error('Failed to fetch auto-connect status');
    } finally {
      autoLoading = false;
    }
  }

  async function startAutoConnect() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          username: config.username,
          password: config.password
        })
      });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch (e) {
      console.error('Failed to start auto-connect');
    }
  }

  async function stopAutoConnect() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/stop', { method: 'POST' });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch (e) {
      console.error('Failed to stop auto-connect');
    }
  }

  async function saveAutoConfig() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(autoConfig)
      });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch (e) {
      console.error('Failed to save auto-connect config');
    }
  }

  async function handleSubmit() {
    // TODO: Implement PPPoE configuration save
    alert('PPPoE configuration saved (not yet implemented)');
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'connected': return '#00ff88';
      case 'connecting':
      case 'retrying': return '#ffaa00';
      case 'failed': return '#ff4444';
      case 'disabled':
      case 'idle': return '#666';
      default: return '#666';
    }
  }

  function formatTimestamp(ts: string | null): string {
    if (!ts) return 'N/A';
    try {
      return new Date(ts).toLocaleString();
    } catch {
      return ts;
    }
  }

  function eventTypeLabel(type: string): string {
    switch (type) {
      case 'connected': return 'Connected';
      case 'disconnected': return 'Disconnected';
      case 'connect_attempt': return 'Connecting';
      case 'connect_failed': return 'Failed';
      case 'health_check_ok': return 'Health OK';
      case 'health_check_failed': return 'Health Fail';
      case 'retry_scheduled': return 'Retry Pending';
      case 'started': return 'Started';
      case 'stopped': return 'Stopped';
      case 'dns_refresh': return 'DNS Refresh';
      case 'error': return 'Error';
      case 'max_retries_reached': return 'Max Retries';
      default: return type;
    }
  }

  function eventTypeColor(type: string): string {
    switch (type) {
      case 'connected': return '#00ff88';
      case 'disconnected': return '#ff4444';
      case 'connect_failed': return '#ff6644';
      case 'health_check_ok': return '#00ff88';
      case 'health_check_failed': return '#ffaa00';
      case 'error': return '#ff4444';
      case 'max_retries_reached': return '#ff4444';
      default: return '#888';
    }
  }
</script>

<svelte:head>
  <title>VectorOS - PPPoE Configuration</title>
</svelte:head>

<div class="pppoe-page">
  <h1>PPPoE Configuration</h1>

  <!-- Auto-Connect Section -->
  <div class="status-card">
    <h2>Auto-Connect</h2>
    {#if autoLoading}
      <p>Loading...</p>
    {:else}
      <div class="autoconnect-controls">
        <div class="autoconnect-toggle">
          <span class="status-indicator" style="background: {statusColor(autoStatus?.status || 'idle')}"></span>
          <span>Status: <strong style="color: {statusColor(autoStatus?.status || 'idle')}">{autoStatus?.status || 'idle'}</strong></span>
          {#if autoStatus?.running}
            <button class="btn-stop" on:click={stopAutoConnect}>Stop</button>
          {:else}
            <button class="btn-start" on:click={startAutoConnect}>Start</button>
          {/if}
        </div>

        <div class="autoconnect-stats">
          <div class="stat">
            <span class="stat-label">Total Reconnects</span>
            <span class="stat-value">{autoStatus?.total_reconnects || 0}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Consecutive Failures</span>
            <span class="stat-value">{autoStatus?.consecutive_failures || 0}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Current Retry Interval</span>
            <span class="stat-value">{autoStatus?.current_retry_interval || 0}s</span>
          </div>
          <div class="stat">
            <span class="stat-label">Last Health Check</span>
            <span class="stat-value">{formatTimestamp(autoStatus?.last_health_check)}</span>
          </div>
        </div>

        <!-- Configuration -->
        <div class="autoconnect-config">
          <h3>Configuration</h3>
          <div class="form-row">
            <div class="form-group">
              <label for="auto-enabled">Enabled</label>
              <label class="toggle">
                <input type="checkbox" id="auto-enabled" bind:checked={autoConfig.enabled} />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="form-group">
              <label for="auto-max-retries">Max Retries (0=infinite)</label>
              <input type="number" id="auto-max-retries" bind:value={autoConfig.max_retries} min="0" />
            </div>
            <div class="form-group">
              <label for="auto-retry-interval">Retry Interval (s)</label>
              <input type="number" id="auto-retry-interval" bind:value={autoConfig.retry_interval} min="1" max="60" />
            </div>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label for="auto-backoff">Backoff Factor</label>
              <input type="number" id="auto-backoff" bind:value={autoConfig.backoff_factor} min="1" max="10" step="0.5" />
            </div>
            <div class="form-group">
              <label for="auto-max-interval">Max Retry Interval (s)</label>
              <input type="number" id="auto-max-interval" bind:value={autoConfig.max_retry_interval} min="10" max="3600" />
            </div>
            <div class="form-group">
              <label for="auto-check">Check Interval (s)</label>
              <input type="number" id="auto-check" bind:value={autoConfig.check_interval} min="5" max="120" />
            </div>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label for="auto-health">Health Check Interval (s)</label>
              <input type="number" id="auto-health" bind:value={autoConfig.health_check_interval} min="10" max="600" />
            </div>
          </div>
          <button class="btn-save" on:click={saveAutoConfig}>Save Configuration</button>
        </div>

        <!-- Connection History -->
        {#if autoStatus?.history && autoStatus.history.length > 0}
          <div class="connection-history">
            <h3>Connection History</h3>
            <div class="history-list">
              {#each autoStatus.history.slice().reverse() as entry}
                <div class="history-entry">
                  <span class="history-time">{formatTimestamp(entry.timestamp)}</span>
                  <span class="history-type" style="color: {eventTypeColor(entry.event_type)}">{eventTypeLabel(entry.event_type)}</span>
                  <span class="history-message">{entry.message}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Connection Status -->
  <div class="status-card">
    <h2>VPP Connection Status</h2>
    {#if loading}
      <p>Loading...</p>
    {:else if error}
      <p class="error">{error}</p>
    {:else if pppoeStatus}
      <div class="status-info">
        <p>Status: <span class="status-ok">{pppoeStatus.status}</span></p>
        <p>Base Message ID: {pppoeStatus.base_msg_id}</p>
        <p>{pppoeStatus.message}</p>
      </div>
    {/if}
  </div>

  <!-- PPPoE Settings -->
  <div class="config-card">
    <h2>PPPoE Settings</h2>
    <form on:submit|preventDefault={handleSubmit}>
      <div class="form-group">
        <label for="username">Username</label>
        <input
          type="text"
          id="username"
          bind:value={config.username}
          placeholder="PPPoE username"
        />
      </div>

      <div class="form-group">
        <label for="password">Password</label>
        <input
          type="password"
          id="password"
          bind:value={config.password}
          placeholder="PPPoE password"
        />
      </div>

      <div class="form-group">
        <label for="interface">Interface</label>
        <select id="interface" bind:value={config.interface}>
          <option value="enp1s0">enp1s0 (WAN)</option>
          <option value="enp2s0">enp2s0 (LAN)</option>
          <option value="enp3s0">enp3s0</option>
        </select>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="mtu">MTU</label>
          <input type="number" id="mtu" bind:value={config.mtu} min="576" max="1500" />
        </div>
        <div class="form-group">
          <label for="mru">MRU</label>
          <input type="number" id="mru" bind:value={config.mru} min="576" max="1500" />
        </div>
      </div>

      <div class="form-group">
        <label for="acName">AC Name Filter (optional)</label>
        <input type="text" id="acName" bind:value={config.acName} placeholder="Leave empty for any" />
      </div>

      <div class="form-group">
        <label for="serviceName">Service Name (optional)</label>
        <input type="text" id="serviceName" bind:value={config.serviceName} placeholder="Leave empty for any" />
      </div>

      <div class="form-group checkbox">
        <label>
          <input type="checkbox" bind:checked={config.usePeerDns} />
          Use Peer DNS
        </label>
      </div>

      <div class="form-group checkbox">
        <label>
          <input type="checkbox" bind:checked={config.addDefaultRoute4} />
          Add Default Route (IPv4)
        </label>
      </div>

      <div class="form-group checkbox">
        <label>
          <input type="checkbox" bind:checked={config.addDefaultRoute6} />
          Add Default Route (IPv6)
        </label>
      </div>

      <button type="submit">Save Configuration</button>
    </form>
  </div>
</div>

<style>
  .pppoe-page {
    max-width: 800px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  .status-card, .config-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 2rem;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
  }

  h3 {
    margin: 1.5rem 0 0.75rem;
    color: #e0e0e0;
    font-size: 1rem;
  }

  .status-info p {
    margin: 0.5rem 0;
  }

  .status-ok {
    color: #00ff88;
    font-weight: bold;
  }

  .error {
    color: #ff4444;
  }

  /* Auto-connect controls */
  .autoconnect-controls {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .autoconnect-toggle {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem;
    background: #0f0f23;
    border-radius: 0.5rem;
  }

  .status-indicator {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }

  .autoconnect-toggle span {
    flex: 1;
  }

  .btn-start, .btn-stop, .btn-save {
    padding: 0.5rem 1.5rem;
    border: none;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-start {
    background: #00ff88;
    color: #0f0f23;
  }

  .btn-stop {
    background: #ff4444;
    color: white;
  }

  .btn-save {
    background: #00ff88;
    color: #0f0f23;
    margin-top: 1rem;
  }

  .btn-start:hover, .btn-stop:hover, .btn-save:hover {
    opacity: 0.9;
  }

  /* Stats */
  .autoconnect-stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }

  .stat {
    background: #0f0f23;
    padding: 0.75rem;
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-label {
    font-size: 0.8rem;
    color: #888;
  }

  .stat-value {
    font-size: 1.1rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  /* Config form */
  .autoconnect-config {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
  }

  .autoconnect-config .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 1rem;
  }

  .autoconnect-config .form-row:last-child {
    grid-template-columns: 1fr;
  }

  /* Toggle switch */
  .toggle {
    position: relative;
    display: inline-block;
    width: 48px;
    height: 24px;
    cursor: pointer;
  }

  .toggle input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .toggle-slider {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: #333;
    border-radius: 24px;
    transition: 0.3s;
  }

  .toggle-slider:before {
    content: '';
    position: absolute;
    width: 18px;
    height: 18px;
    left: 3px;
    bottom: 3px;
    background: white;
    border-radius: 50%;
    transition: 0.3s;
  }

  .toggle input:checked + .toggle-slider {
    background: #00ff88;
  }

  .toggle input:checked + .toggle-slider:before {
    transform: translateX(24px);
  }

  /* Connection history */
  .connection-history {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-height: 300px;
    overflow-y: auto;
  }

  .history-entry {
    display: grid;
    grid-template-columns: 160px 100px 1fr;
    gap: 0.75rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #1a1a2e;
    font-size: 0.85rem;
  }

  .history-time {
    color: #888;
    font-family: monospace;
  }

  .history-type {
    font-weight: 600;
  }

  .history-message {
    color: #aaa;
  }

  /* General form styles */
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .form-group.checkbox {
    flex-direction: row;
    align-items: center;
  }

  .form-group.checkbox label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  label {
    font-size: 0.9rem;
    color: #888;
  }

  input, select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.75rem;
    border-radius: 0.5rem;
    font-size: 1rem;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  input[type="checkbox"] {
    width: 1.2rem;
    height: 1.2rem;
    cursor: pointer;
  }

  button[type="submit"] {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 1rem;
    border-radius: 0.5rem;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    margin-top: 1rem;
  }

  button[type="submit"]:hover {
    opacity: 0.9;
  }
</style>
