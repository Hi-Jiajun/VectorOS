<script lang="ts">
  import { onMount } from 'svelte';

  let firewallStatus: any = null;
  let rules: any[] = [];
  let loading = true;
  let error = '';

  // Rule form
  let newRule = {
    action: 'deny',
    src_ip: '',
    dst_ip: '',
    src_port: '',
    dst_port: '',
    protocol: 'ip',
    description: ''
  };

  onMount(async () => {
    await fetchFirewallStatus();
  });

  async function fetchFirewallStatus() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/firewall/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        firewallStatus = data;
        rules = data.rules || [];
      }
    } catch (e) {
      error = 'Failed to fetch firewall status';
    } finally {
      loading = false;
    }
  }

  async function addRule() {
    try {
      error = '';
      const payload: any = {
        action: newRule.action,
        protocol: newRule.protocol || 'ip'
      };

      if (newRule.src_ip) payload.src_ip = newRule.src_ip;
      if (newRule.dst_ip) payload.dst_ip = newRule.dst_ip;
      if (newRule.src_port) payload.src_port = parseInt(newRule.src_port);
      if (newRule.dst_port) payload.dst_port = parseInt(newRule.dst_port);
      if (newRule.description) payload.description = newRule.description;

      const res = await fetch('/api/firewall/add-rule', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        // Reset form
        newRule = { action: 'deny', src_ip: '', dst_ip: '', src_port: '', dst_port: '', protocol: 'ip', description: '' };
        await fetchFirewallStatus();
      }
    } catch (e) {
      error = 'Failed to add rule';
    }
  }

  async function deleteRule(id: number) {
    if (!confirm(`Delete rule #${id}?`)) return;
    try {
      const res = await fetch('/api/firewall/del-rule', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        await fetchFirewallStatus();
      }
    } catch (e) {
      error = 'Failed to delete rule';
    }
  }

  async function toggleFirewall(enable: boolean) {
    try {
      const endpoint = enable ? '/api/firewall/enable' : '/api/firewall/disable';
      const res = await fetch(endpoint, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        await fetchFirewallStatus();
      }
    } catch (e) {
      error = `Failed to ${enable ? 'enable' : 'disable'} firewall`;
    }
  }
</script>

<svelte:head>
  <title>VectorOS - Firewall Rules</title>
</svelte:head>

<div class="firewall-page">
  <h1>Firewall Rules</h1>

  <!-- Status Card -->
  <div class="status-card">
    <h2>Firewall Status</h2>
    {#if loading}
      <p>Loading...</p>
    {:else if firewallStatus}
      <div class="status-info">
        <p>
          Status:
          <span class="status-badge" class:enabled={firewallStatus.enabled} class:disabled={!firewallStatus.enabled}>
            {firewallStatus.enabled ? 'ENABLED' : 'DISABLED'}
          </span>
        </p>
        <p>Total Rules: {firewallStatus.total_rules || 0}</p>
        <p>Active Rules: {firewallStatus.active_rules || 0}</p>
      </div>
      <div class="button-row">
        {#if firewallStatus.enabled}
          <button class="btn-danger" on:click={() => toggleFirewall(false)}>Disable Firewall</button>
        {:else}
          <button class="btn-primary" on:click={() => toggleFirewall(true)}>Enable Firewall</button>
        {/if}
        <button class="btn-secondary" on:click={fetchFirewallStatus}>Refresh</button>
      </div>
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  <!-- Add Rule Form -->
  <div class="config-card">
    <h2>Add Firewall Rule</h2>
    <form on:submit|preventDefault={addRule}>
      <div class="form-row">
        <div class="form-group">
          <label for="action">Action</label>
          <select id="action" bind:value={newRule.action}>
            <option value="permit">Permit (Allow)</option>
            <option value="deny">Deny (Block)</option>
          </select>
        </div>

        <div class="form-group">
          <label for="protocol">Protocol</label>
          <select id="protocol" bind:value={newRule.protocol}>
            <option value="ip">IP (Any)</option>
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="icmp">ICMP</option>
          </select>
        </div>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="src_ip">Source IP (CIDR)</label>
          <input type="text" id="src_ip" bind:value={newRule.src_ip} placeholder="e.g. 192.168.1.0/24" />
        </div>

        <div class="form-group">
          <label for="dst_ip">Destination IP (CIDR)</label>
          <input type="text" id="dst_ip" bind:value={newRule.dst_ip} placeholder="e.g. 10.0.0.0/8" />
        </div>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="src_port">Source Port</label>
          <input type="number" id="src_port" bind:value={newRule.src_port} placeholder="Any" min="1" max="65535" />
        </div>

        <div class="form-group">
          <label for="dst_port">Destination Port</label>
          <input type="number" id="dst_port" bind:value={newRule.dst_port} placeholder="e.g. 80, 443" min="1" max="65535" />
        </div>
      </div>

      <div class="form-group">
        <label for="description">Description (optional)</label>
        <input type="text" id="description" bind:value={newRule.description} placeholder="Rule description..." />
      </div>

      <button type="submit" class="btn-primary">Add Rule</button>
    </form>
  </div>

  <!-- Rules List -->
  <div class="rules-card">
    <h2>Current Rules</h2>

    {#if rules.length === 0}
      <p class="no-data">No firewall rules configured</p>
    {:else}
      <div class="rules-table">
        <div class="rule-header">
          <span class="col-id">ID</span>
          <span class="col-action">Action</span>
          <span class="col-proto">Protocol</span>
          <span class="col-src">Source</span>
          <span class="col-dst">Destination</span>
          <span class="col-desc">Description</span>
          <span class="col-actions">Actions</span>
        </div>

        {#each rules as rule}
          <div class="rule-row">
            <span class="col-id">#{rule.id}</span>
            <span class="col-action">
              <span class="action-badge" class:permit={rule.action === 'permit'} class:deny={rule.action === 'deny'}>
                {rule.action === 'permit' ? 'ALLOW' : 'BLOCK'}
              </span>
            </span>
            <span class="col-proto">{rule.protocol || 'ip'}</span>
            <span class="col-src">
              {rule.src_ip || '*'}
              {#if rule.src_port}: {rule.src_port}{/if}
            </span>
            <span class="col-dst">
              {rule.dst_ip || '*'}
              {#if rule.dst_port}: {rule.dst_port}{/if}
            </span>
            <span class="col-desc">{rule.description || '-'}</span>
            <span class="col-actions">
              <button class="btn-delete" on:click={() => deleteRule(rule.id)}>Delete</button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .firewall-page {
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

  .status-info p {
    margin: 0.4rem 0;
  }

  .status-badge {
    padding: 0.2rem 0.8rem;
    border-radius: 0.3rem;
    font-weight: bold;
    font-size: 0.85rem;
  }

  .status-badge.enabled {
    background: #003322;
    color: #00ff88;
  }

  .status-badge.disabled {
    background: #331111;
    color: #ff4444;
  }

  .button-row {
    display: flex;
    gap: 1rem;
    margin-top: 1rem;
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

  .btn-danger {
    background: #ff4444;
    color: #ffffff;
    border: none;
    padding: 0.6rem 1.2rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
  }

  .btn-danger:hover { opacity: 0.9; }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #ff4444;
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

  .config-card button[type="submit"] {
    margin-top: 0.5rem;
  }

  .rules-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .no-data {
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  .rules-table {
    font-size: 0.9rem;
    overflow-x: auto;
  }

  .rule-header, .rule-row {
    display: grid;
    grid-template-columns: 50px 70px 70px 1fr 1fr 1fr 80px;
    gap: 0.5rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid #333;
    align-items: center;
  }

  .rule-header {
    font-weight: bold;
    color: #888;
    border-bottom: 1px solid #555;
  }

  .rule-row {
    color: #e0e0e0;
  }

  .col-id {
    color: #888;
  }

  .action-badge {
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: bold;
  }

  .action-badge.permit {
    background: #003322;
    color: #00ff88;
  }

  .action-badge.deny {
    background: #331111;
    color: #ff4444;
  }

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
</style>
