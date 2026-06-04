<script lang="ts">
  import { onMount } from 'svelte';

  let qosStatus: any = null;
  let loading = true;
  let error = '';
  let success = '';

  // Policer form
  let newPolicer = {
    name: '',
    rate: '',
    burst: '',
    policer_type: 'single_rate_two_color'
  };

  // Rate limit form
  let rateLimitForm = {
    interface: '',
    rate: '',
    burst: '',
    direction: 'both'
  };

  const policerTypes = [
    { value: 'single_rate_two_color', label: 'Single Rate Two Color (srTCM)' },
    { value: 'single_rate_three_color', label: 'Single Rate Three Color' },
    { value: 'trtcm', label: 'Two Rate Three Color (trTCM)' }
  ];

  const directions = [
    { value: 'both', label: 'Both (Input + Output)' },
    { value: 'input', label: 'Input Only' },
    { value: 'output', label: 'Output Only' }
  ];

  onMount(async () => {
    await fetchQosStatus();
  });

  async function fetchQosStatus() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/qos/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        qosStatus = data;
      }
    } catch (e) {
      error = 'Failed to fetch QoS status';
    } finally {
      loading = false;
    }
  }

  async function createPolicer() {
    try {
      error = '';
      success = '';

      if (!newPolicer.name || !newPolicer.rate || !newPolicer.burst) {
        error = 'Name, rate, and burst are required';
        return;
      }

      const res = await fetch('/api/qos/policer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: newPolicer.name,
          rate: parseInt(newPolicer.rate),
          burst: parseInt(newPolicer.burst),
          policer_type: newPolicer.policer_type
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Policer created';
        newPolicer = { name: '', rate: '', burst: '', policer_type: 'single_rate_two_color' };
        await fetchQosStatus();
      }
    } catch (e) {
      error = 'Failed to create policer';
    }
  }

  async function deletePolicer(name: string) {
    if (!confirm(`Delete policer '${name}'?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch(`/api/qos/policer/${encodeURIComponent(name)}`, {
        method: 'DELETE'
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Policer deleted';
        await fetchQosStatus();
      }
    } catch (e) {
      error = 'Failed to delete policer';
    }
  }

  async function setInterfaceLimit() {
    try {
      error = '';
      success = '';

      if (!rateLimitForm.interface || !rateLimitForm.rate || !rateLimitForm.burst) {
        error = 'Interface, rate, and burst are required';
        return;
      }

      const res = await fetch(`/api/qos/interface/${encodeURIComponent(rateLimitForm.interface)}/limit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          rate: parseInt(rateLimitForm.rate),
          burst: parseInt(rateLimitForm.burst),
          direction: rateLimitForm.direction
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Rate limit set';
        rateLimitForm = { interface: '', rate: '', burst: '', direction: 'both' };
        await fetchQosStatus();
      }
    } catch (e) {
      error = 'Failed to set rate limit';
    }
  }

  function formatRate(bitsPerSec: number): string {
    if (bitsPerSec >= 1000000000) return `${(bitsPerSec / 1000000000).toFixed(1)} Gbps`;
    if (bitsPerSec >= 1000000) return `${(bitsPerSec / 1000000).toFixed(1)} Mbps`;
    if (bitsPerSec >= 1000) return `${(bitsPerSec / 1000).toFixed(1)} Kbps`;
    return `${bitsPerSec} bps`;
  }
</script>

<svelte:head>
  <title>VectorOS - QoS Management</title>
</svelte:head>

<div class="qos-page">
  <h1>QoS Management</h1>

  <!-- Status Card -->
  <div class="status-card">
    <h2>QoS Overview</h2>
    {#if loading}
      <p>Loading...</p>
    {:else if qosStatus}
      <div class="status-grid">
        <div class="stat-item">
          <span class="stat-value">{qosStatus.total_policers || 0}</span>
          <span class="stat-label">Policers</span>
        </div>
        <div class="stat-item">
          <span class="stat-value">{qosStatus.total_rate_limits || 0}</span>
          <span class="stat-label">Rate Limits</span>
        </div>
        <div class="stat-item">
          <span class="stat-value">{qosStatus.total_dscp_marks || 0}</span>
          <span class="stat-label">DSCP Marks</span>
        </div>
      </div>
      <div class="button-row">
        <button class="btn-secondary" on:click={fetchQosStatus}>Refresh</button>
      </div>
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}
  {#if success}
    <div class="success-card">{success}</div>
  {/if}

  <!-- Create Policer -->
  <div class="config-card">
    <h2>Create Policer</h2>
    <form on:submit|preventDefault={createPolicer}>
      <div class="form-row">
        <div class="form-group">
          <label for="policer-name">Name</label>
          <input type="text" id="policer-name" bind:value={newPolicer.name} placeholder="e.g. voip-policer" />
        </div>
        <div class="form-group">
          <label for="policer-type">Type</label>
          <select id="policer-type" bind:value={newPolicer.policer_type}>
            {#each policerTypes as pt}
              <option value={pt.value}>{pt.label}</option>
            {/each}
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="policer-rate">Rate (bits/sec)</label>
          <input type="number" id="policer-rate" bind:value={newPolicer.rate} placeholder="e.g. 100000000" min="1" />
        </div>
        <div class="form-group">
          <label for="policer-burst">Burst Size (bytes)</label>
          <input type="number" id="policer-burst" bind:value={newPolicer.burst} placeholder="e.g. 15000" min="1" />
        </div>
      </div>
      <button type="submit" class="btn-primary">Create Policer</button>
    </form>
  </div>

  <!-- Interface Rate Limit -->
  <div class="config-card">
    <h2>Set Interface Rate Limit</h2>
    <form on:submit|preventDefault={setInterfaceLimit}>
      <div class="form-row">
        <div class="form-group">
          <label for="rl-interface">Interface</label>
          <input type="text" id="rl-interface" bind:value={rateLimitForm.interface} placeholder="e.g. GigabitEthernet0/0/0" />
        </div>
        <div class="form-group">
          <label for="rl-direction">Direction</label>
          <select id="rl-direction" bind:value={rateLimitForm.direction}>
            {#each directions as d}
              <option value={d.value}>{d.label}</option>
            {/each}
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="rl-rate">Rate (bits/sec)</label>
          <input type="number" id="rl-rate" bind:value={rateLimitForm.rate} placeholder="e.g. 100000000" min="1" />
        </div>
        <div class="form-group">
          <label for="rl-burst">Burst Size (bytes)</label>
          <input type="number" id="rl-burst" bind:value={rateLimitForm.burst} placeholder="e.g. 15000" min="1" />
        </div>
      </div>
      <button type="submit" class="btn-primary">Apply Rate Limit</button>
    </form>
  </div>

  <!-- Active Policers -->
  {#if qosStatus && qosStatus.policers && Object.keys(qosStatus.policers).length > 0}
    <div class="config-card">
      <h2>Active Policers</h2>
      <div class="table-wrapper">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Rate</th>
              <th>Burst</th>
              <th>Interfaces</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each Object.entries(qosStatus.policers) as [name, policer]}
              <tr>
                <td class="name-cell">{name}</td>
                <td><span class="type-badge">{policer.type}</span></td>
                <td>{formatRate(policer.rate)}</td>
                <td>{policer.burst} bytes</td>
                <td>
                  {#if policer.interfaces && policer.interfaces.length > 0}
                    {policer.interfaces.join(', ')}
                  {:else}
                    <span class="no-data">none</span>
                  {/if}
                </td>
                <td>
                  <button class="btn-delete" on:click={() => deletePolicer(name)}>Delete</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  <!-- Active Rate Limits -->
  {#if qosStatus && qosStatus.rate_limits && Object.keys(qosStatus.rate_limits).length > 0}
    <div class="config-card">
      <h2>Active Rate Limits</h2>
      <div class="table-wrapper">
        <table>
          <thead>
            <tr>
              <th>Interface</th>
              <th>Rate</th>
              <th>Burst</th>
              <th>Direction</th>
              <th>Policer</th>
            </tr>
          </thead>
          <tbody>
            {#each Object.entries(qosStatus.rate_limits) as [iface, limit]}
              <tr>
                <td class="name-cell">{iface}</td>
                <td>{formatRate(limit.rate)}</td>
                <td>{limit.burst} bytes</td>
                <td><span class="dir-badge">{limit.direction}</span></td>
                <td>{limit.policer_name}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  <!-- VPP Policer Output -->
  {#if qosStatus && qosStatus.vpp_policer_output && qosStatus.vpp_policer_output !== 'N/A'}
    <div class="config-card">
      <h2>VPP Policer Output</h2>
      <pre class="vpp-output">{qosStatus.vpp_policer_output}</pre>
    </div>
  {/if}
</div>

<style>
  .qos-page {
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

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.5rem;
    margin-bottom: 1rem;
  }

  .stat-item {
    text-align: center;
  }

  .stat-value {
    display: block;
    font-size: 2rem;
    font-weight: bold;
    color: #00ff88;
  }

  .stat-label {
    display: block;
    color: #888;
    font-size: 0.85rem;
    margin-top: 0.25rem;
  }

  .button-row {
    display: flex;
    gap: 1rem;
    margin-top: 1rem;
  }

  .config-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

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

  .btn-delete {
    background: none;
    border: 1px solid #ff4444;
    color: #ff4444;
    padding: 0.3rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .btn-delete:hover {
    background: #ff4444;
    color: #ffffff;
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

  .table-wrapper {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th {
    text-align: left;
    padding: 0.75rem 0.5rem;
    border-bottom: 2px solid #444;
    color: #888;
    font-size: 0.8rem;
    text-transform: uppercase;
  }

  td {
    padding: 0.75rem 0.5rem;
    border-bottom: 1px solid #333;
    color: #e0e0e0;
  }

  .name-cell {
    font-weight: 600;
    color: #00ff88;
  }

  .type-badge {
    background: #16213e;
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    color: #88aaff;
  }

  .dir-badge {
    background: #1a1a2e;
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    color: #ffaa00;
  }

  .no-data {
    color: #555;
    font-style: italic;
  }

  .vpp-output {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
    font-family: 'Courier New', monospace;
    font-size: 0.85rem;
    color: #aaa;
    overflow-x: auto;
    white-space: pre-wrap;
  }
</style>
