<script lang="ts">
  import { onMount } from 'svelte';

  let logs: any[] = [];
  let loading = true;
  let error = '';

  // Filter state
  let sources = 'vpp,dnsmasq,vectoros';
  let level = 'debug';
  let lines = 500;
  let keyword = '';
  let limit = 100;

  onMount(async () => {
    await fetchLogs();
  });

  async function fetchLogs() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/logs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sources, level, lines, filter: keyword || undefined, limit })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
        logs = [];
      } else {
        logs = data.logs || [];
      }
    } catch (e) {
      error = 'Failed to fetch logs';
      logs = [];
    } finally {
      loading = false;
    }
  }

  async function clearLogs() {
    if (!confirm('Are you sure you want to clear all logs?')) return;
    try {
      const res = await fetch('/api/logs/clear', { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        await fetchLogs();
      } else {
        error = data.error || 'Failed to clear logs';
      }
    } catch (e) {
      error = 'Failed to clear logs';
    }
  }

  function levelColor(level: string) {
    switch (level?.toLowerCase()) {
      case 'error': return '#ff4444';
      case 'warn': return '#ffaa00';
      case 'info': return '#00ff88';
      case 'debug': return '#888888';
      default: return '#e0e0e0';
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      fetchLogs();
    }
  }
</script>

<svelte:head>
  <title>VectorOS - Log Viewer</title>
</svelte:head>

<div class="logs-page">
  <h1>Log Viewer</h1>

  <div class="controls-card">
    <h2>Filter Options</h2>
    <div class="filter-row">
      <div class="form-group">
        <label for="sources">Sources</label>
        <input
          type="text"
          id="sources"
          bind:value={sources}
          placeholder="vpp,dnsmasq,vectoros"
          on:keydown={handleKeydown}
        />
      </div>

      <div class="form-group">
        <label for="level">Min Level</label>
        <select id="level" bind:value={level}>
          <option value="debug">Debug</option>
          <option value="info">Info</option>
          <option value="warn">Warn</option>
          <option value="error">Error</option>
        </select>
      </div>

      <div class="form-group">
        <label for="lines">Lines</label>
        <input type="number" id="lines" bind:value={lines} min="100" max="5000" step="100" />
      </div>

      <div class="form-group">
        <label for="keyword">Keyword</label>
        <input
          type="text"
          id="keyword"
          bind:value={keyword}
          placeholder="Filter keyword..."
          on:keydown={handleKeydown}
        />
      </div>

      <div class="form-group">
        <label for="limit">Max Results</label>
        <input type="number" id="limit" bind:value={limit} min="10" max="500" step="10" />
      </div>
    </div>

    <div class="button-row">
      <button class="btn-primary" on:click={fetchLogs}>Refresh</button>
      <button class="btn-danger" on:click={clearLogs}>Clear Logs</button>
    </div>
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  <div class="logs-card">
    <h2>Logs ({logs.length} entries)</h2>

    {#if loading}
      <p>Loading...</p>
    {:else if logs.length === 0}
      <p class="no-data">No log entries found</p>
    {:else}
      <div class="log-table">
        <div class="log-header">
          <span class="col-ts">Timestamp</span>
          <span class="col-level">Level</span>
          <span class="col-source">Source</span>
          <span class="col-msg">Message</span>
        </div>

        {#each logs as log}
          <div class="log-row">
            <span class="col-ts">{log.timestamp || '-'}</span>
            <span class="col-level" style="color: {levelColor(log.level)}">
              {log.level?.toUpperCase() || 'INFO'}
            </span>
            <span class="col-source">{log.source || '-'}</span>
            <span class="col-msg">{log.message || ''}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .logs-page {
    max-width: 1200px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
  }

  .controls-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .filter-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
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
  }

  .btn-primary {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-danger {
    background: #ff4444;
    color: #ffffff;
    border: none;
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-danger:hover {
    opacity: 0.9;
  }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #ff4444;
  }

  .logs-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .no-data {
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  .log-table {
    font-family: 'Courier New', monospace;
    font-size: 0.85rem;
    overflow-x: auto;
  }

  .log-header, .log-row {
    display: grid;
    grid-template-columns: 180px 70px 100px 1fr;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #333;
    align-items: start;
  }

  .log-header {
    font-weight: bold;
    color: #888;
    border-bottom: 1px solid #555;
  }

  .log-row {
    color: #e0e0e0;
  }

  .col-ts {
    color: #888;
  }

  .col-level {
    font-weight: bold;
    text-transform: uppercase;
  }

  .col-source {
    color: #00aaff;
  }

  .col-msg {
    word-break: break-word;
  }
</style>
