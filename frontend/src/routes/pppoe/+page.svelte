<script lang="ts">
  import { onMount } from 'svelte';

  let pppoeStatus: any = null;
  let loading = true;
  let error = '';

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

  onMount(async () => {
    await fetchPppoeStatus();
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

  async function handleSubmit() {
    // TODO: Implement PPPoE configuration save
    alert('PPPoE configuration saved (not yet implemented)');
  }
</script>

<svelte:head>
  <title>VectorOS - PPPoE Configuration</title>
</svelte:head>

<div class="pppoe-page">
  <h1>PPPoE Configuration</h1>

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

  button {
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

  button:hover {
    opacity: 0.9;
  }
</style>
