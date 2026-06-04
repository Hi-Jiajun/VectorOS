<script lang="ts">
  import { onMount } from 'svelte';

  interface ServiceInfo {
    name: string;
    display_name: string;
    state: string;
    description: string;
    last_transition: string;
    error?: string;
  }

  let services: ServiceInfo[] = [];
  let loading = true;
  let error = '';
  let actionInProgress: string | null = null;

  onMount(async () => {
    await fetchServices();
  });

  async function fetchServices() {
    try {
      loading = true;
      const res = await fetch('/api/services');
      const data = await res.json();
      if (data.status === 'ok') {
        services = data.services;
      } else {
        error = data.error || 'Failed to fetch services';
      }
    } catch (e) {
      error = 'Failed to connect to server';
    } finally {
      loading = false;
    }
  }

  async function serviceAction(name: string, action: string) {
    try {
      actionInProgress = `${name}-${action}`;
      const res = await fetch(`/api/services/${name}/${action}`, { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        // Update the service in the list
        const idx = services.findIndex(s => s.name === name);
        if (idx >= 0) {
          services[idx] = data.service;
          services = [...services];
        }
      } else {
        error = data.error || `${action} failed`;
      }
    } catch (e) {
      error = `Failed to ${action} service`;
    } finally {
      actionInProgress = null;
    }
  }

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

  function stateIcon(state: string): string {
    switch (state) {
      case 'running': return '●'; // filled circle
      case 'stopped': return '○'; // empty circle
      case 'starting': return '◑'; // half circle
      case 'stopping': return '◑';
      case 'failed': return '✖'; // X
      default: return '○';
    }
  }

  function formatTime(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<svelte:head>
  <title>VectorOS - Service Manager</title>
</svelte:head>

<div class="services-page">
  <div class="header-row">
    <h1>Service Manager</h1>
    <button class="btn btn-refresh" on:click={fetchServices} disabled={loading}>
      {loading ? 'Refreshing...' : 'Refresh'}
    </button>
  </div>

  {#if error}
    <div class="error-banner">
      <span>{error}</span>
      <button class="btn-close" on:click={() => error = ''}>✖</button>
    </div>
  {/if}

  {#if loading && services.length === 0}
    <p class="loading-text">Loading services...</p>
  {:else}
    <div class="services-grid">
      {#each services as svc}
        <div class="service-card">
          <div class="service-header">
            <div class="service-title">
              <span class="state-dot" style="color: {stateColor(svc.state)}">{stateIcon(svc.state)}</span>
              <h2>{svc.display_name}</h2>
            </div>
            <span class="state-badge" style="background: {stateColor(svc.state)}20; color: {stateColor(svc.state)}; border: 1px solid {stateColor(svc.state)}40">
              {svc.state}
            </span>
          </div>

          <p class="service-desc">{svc.description}</p>

          {#if svc.error}
            <div class="service-error">
              <strong>Error:</strong> {svc.error}
            </div>
          {/if}

          <div class="service-meta">
            <span class="meta-item">Last change: {formatTime(svc.last_transition)}</span>
          </div>

          <div class="service-actions">
            {#if svc.state === 'stopped' || svc.state === 'failed'}
              <button
                class="btn btn-start"
                on:click={() => serviceAction(svc.name, 'start')}
                disabled={actionInProgress === `${svc.name}-start`}
              >
                {actionInProgress === `${svc.name}-start` ? 'Starting...' : 'Start'}
              </button>
            {:else if svc.state === 'running'}
              <button
                class="btn btn-stop"
                on:click={() => serviceAction(svc.name, 'stop')}
                disabled={actionInProgress === `${svc.name}-stop`}
              >
                {actionInProgress === `${svc.name}-stop` ? 'Stopping...' : 'Stop'}
              </button>
              <button
                class="btn btn-restart"
                on:click={() => serviceAction(svc.name, 'restart')}
                disabled={actionInProgress === `${svc.name}-restart`}
              >
                {actionInProgress === `${svc.name}-restart` ? 'Restarting...' : 'Restart'}
              </button>
              <button
                class="btn btn-reload"
                on:click={() => serviceAction(svc.name, 'reload')}
                disabled={actionInProgress === `${svc.name}-reload`}
              >
                {actionInProgress === `${svc.name}-reload` ? 'Reloading...' : 'Reload Config'}
              </button>
            {:else}
              <span class="transitioning-text">
                {svc.state}...
              </span>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if services.length === 0}
      <div class="empty-state">
        <p>No services registered.</p>
      </div>
    {/if}
  {/if}
</div>

<style>
  .services-page {
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

  .error-banner {
    background: #ff444422;
    border: 1px solid #ff4444;
    color: #ff8888;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1.5rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .btn-close {
    background: none;
    border: none;
    color: #ff8888;
    cursor: pointer;
    font-size: 1rem;
  }

  .loading-text {
    color: #888;
    font-style: italic;
  }

  .services-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
    gap: 1.5rem;
  }

  .service-card {
    background: #1a1a2e;
    border: 1px solid #333;
    border-radius: 0.75rem;
    padding: 1.25rem;
    transition: border-color 0.2s;
  }

  .service-card:hover {
    border-color: #555;
  }

  .service-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
  }

  .service-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .service-title h2 {
    margin: 0;
    font-size: 1.1rem;
    color: #e0e0e0;
  }

  .state-dot {
    font-size: 1.2rem;
  }

  .state-badge {
    font-size: 0.75rem;
    padding: 0.2rem 0.6rem;
    border-radius: 1rem;
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.05em;
  }

  .service-desc {
    color: #999;
    font-size: 0.9rem;
    margin: 0.25rem 0 0.75rem;
  }

  .service-error {
    background: #ff444415;
    border: 1px solid #ff444440;
    color: #ff8888;
    padding: 0.5rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.85rem;
    margin-bottom: 0.75rem;
  }

  .service-meta {
    font-size: 0.8rem;
    color: #666;
    margin-bottom: 0.75rem;
  }

  .service-actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn {
    padding: 0.4rem 0.8rem;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    transition: all 0.2s;
    color: #fff;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-refresh {
    background: #16213e;
    border-color: #333;
    color: #aaa;
  }

  .btn-refresh:hover:not(:disabled) {
    background: #1a2a4a;
    color: #fff;
  }

  .btn-start {
    background: #00aa55;
  }

  .btn-start:hover:not(:disabled) {
    background: #00cc66;
  }

  .btn-stop {
    background: #aa3333;
  }

  .btn-stop:hover:not(:disabled) {
    background: #cc4444;
  }

  .btn-restart {
    background: #aa7700;
  }

  .btn-restart:hover:not(:disabled) {
    background: #cc9900;
  }

  .btn-reload {
    background: #3366aa;
  }

  .btn-reload:hover:not(:disabled) {
    background: #4488cc;
  }

  .transitioning-text {
    color: #ffaa00;
    font-style: italic;
    font-size: 0.85rem;
  }

  .empty-state {
    text-align: center;
    color: #666;
    padding: 3rem;
  }
</style>
