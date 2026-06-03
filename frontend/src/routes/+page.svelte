<script lang="ts">
  import { onMount } from 'svelte';

  let health: any = null;
  let interfaces: any[] = [];
  let routes: any[] = [];

  onMount(async () => {
    const [healthRes, ifacesRes, routesRes] = await Promise.all([
      fetch('/api/health').then(r => r.json()),
      fetch('/api/interfaces').then(r => r.json()),
      fetch('/api/routes').then(r => r.json())
    ]);

    health = healthRes;
    interfaces = ifacesRes.interfaces;
    routes = routesRes.routes;
  });
</script>

<svelte:head>
  <title>VectorOS - Dashboard</title>
</svelte:head>

<div class="dashboard">
  <h1>VectorOS Dashboard</h1>

  <div class="status-card">
    <h2>System Status</h2>
    {#if health}
      <p>Status: <span class="status-ok">{health.status}</span></p>
      <p>Version: {health.version}</p>
    {:else}
      <p>Loading...</p>
    {/if}
  </div>

  <div class="grid">
    <div class="card">
      <h2>Interfaces</h2>
      <p class="count">{interfaces.length}</p>
      <p>active interfaces</p>
    </div>

    <div class="card">
      <h2>Routes</h2>
      <p class="count">{routes.length}</p>
      <p>routing entries</p>
    </div>
  </div>
</div>

<style>
  .dashboard {
    max-width: 1200px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 2rem;
  }

  .status-ok {
    color: #00ff88;
    font-weight: bold;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
  }

  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    text-align: center;
  }

  .count {
    font-size: 3rem;
    font-weight: bold;
    color: #00ff88;
    margin: 0.5rem 0;
  }
</style>
