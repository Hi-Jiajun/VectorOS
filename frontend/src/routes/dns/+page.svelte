<script lang="ts">
  import { onMount } from 'svelte';

  // ── State ───────────────────────────────────────────────────────
  let activeTab = 'config';
  let dnsStatus: any = null;
  let loading = true;
  let error = '';
  let success = '';
  let saving = false;

  // Configuration form
  let config = {
    enabled: false,
    upstream_primary: '8.8.8.8',
    upstream_secondary: '1.1.1.1',
    upstream_v6_primary: '2001:4860:4860::8888',
    upstream_v6_secondary: '2606:4700:4700::1111',
    cache_size: 1000,
    interface: 'lan0',
  };

  // Advanced: Custom DNS records
  let customRecords: { type: string; name: string; value: string; ttl: number }[] = [];
  let newRecord = { type: 'A', name: '', value: '', ttl: 3600 };
  let editingRecordIndex: number | null = null;

  // Advanced: DNS blocking
  let blockingEnabled = false;
  let blocklists = [
    { name: 'Steven Black\'s Unified Hosts', url: 'https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts', enabled: true },
    { name: 'AdGuard DNS Filter', url: 'https://adguardteam.github.io/AdGuardSDNSFilter/Filters/filter.txt', enabled: false },
  ];
  let newBlocklist = { name: '', url: '' };

  // Cache stats (simulated for frontend)
  let cacheStats = {
    hits: 0,
    misses: 0,
    size: 0,
    insertions: 0,
    evictions: 0,
  };

  // ── Lifecycle ──────────────────────────────────────────────────
  onMount(async () => {
    await fetchStatus();
  });

  // ── Data fetching ──────────────────────────────────────────────
  async function fetchStatus() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/dns/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        dnsStatus = data;
        config.enabled = data.status === 'active';

        // Populate config from status
        if (data.upstream && data.upstream.length >= 1) {
          config.upstream_primary = data.upstream[0] || '8.8.8.8';
        }
        if (data.upstream && data.upstream.length >= 2) {
          config.upstream_secondary = data.upstream[1] || '1.1.1.1';
        }
        if (data.upstream_v6 && data.upstream_v6.length >= 1) {
          config.upstream_v6_primary = data.upstream_v6[0] || '2001:4860:4860::8888';
        }
        if (data.upstream_v6 && data.upstream_v6.length >= 2) {
          config.upstream_v6_secondary = data.upstream_v6[1] || '2606:4700:4700::1111';
        }
        if (data.cache_size) {
          config.cache_size = data.cache_size;
        }
        if (data.interface) {
          config.interface = data.interface;
        }
      }
    } catch (e) {
      error = 'Failed to fetch DNS status';
    } finally {
      loading = false;
    }
  }

  // ── Enable / Save ─────────────────────────────────────────────
  async function saveConfig() {
    try {
      saving = true;
      error = '';
      success = '';

      const upstream = [config.upstream_primary, config.upstream_secondary]
        .filter(s => s.trim())
        .join(',');

      const upstream_v6 = [config.upstream_v6_primary, config.upstream_v6_secondary]
        .filter(s => s.trim())
        .join(',');

      const payload: any = {
        upstream,
        upstream_v6,
        interface: config.interface,
        cache_size: config.cache_size,
      };

      const res = await fetch('/api/dns/enable', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'DNS configuration saved successfully';
        config.enabled = true;
        await fetchStatus();
      }
    } catch (e) {
      error = 'Failed to save DNS configuration';
    } finally {
      saving = false;
    }
  }

  // ── Custom DNS Records ─────────────────────────────────────────
  function addRecord() {
    if (!newRecord.name || !newRecord.value) return;
    if (editingRecordIndex !== null) {
      customRecords[editingRecordIndex] = { ...newRecord };
      editingRecordIndex = null;
    } else {
      customRecords = [...customRecords, { ...newRecord }];
    }
    newRecord = { type: 'A', name: '', value: '', ttl: 3600 };
  }

  function editRecord(index: number) {
    newRecord = { ...customRecords[index] };
    editingRecordIndex = index;
  }

  function removeRecord(index: number) {
    customRecords = customRecords.filter((_, i) => i !== index);
    if (editingRecordIndex === index) {
      editingRecordIndex = null;
      newRecord = { type: 'A', name: '', value: '', ttl: 3600 };
    }
  }

  function cancelEditRecord() {
    editingRecordIndex = null;
    newRecord = { type: 'A', name: '', value: '', ttl: 3600 };
  }

  // ── Blocklists ────────────────────────────────────────────────
  function addBlocklist() {
    if (!newBlocklist.name || !newBlocklist.url) return;
    blocklists = [...blocklists, { ...newBlocklist, enabled: true }];
    newBlocklist = { name: '', url: '' };
  }

  function removeBlocklist(index: number) {
    blocklists = blocklists.filter((_, i) => i !== index);
  }

  function toggleBlocklist(index: number) {
    blocklists[index].enabled = !blocklists[index].enabled;
    blocklists = [...blocklists];
  }

  // ── Helpers ────────────────────────────────────────────────────
  function cacheHitRate(): string {
    const total = cacheStats.hits + cacheStats.misses;
    if (total === 0) return '0%';
    return ((cacheStats.hits / total) * 100).toFixed(1) + '%';
  }

  function clearMessages() {
    error = '';
    success = '';
  }
</script>

<svelte:head>
  <title>VectorOS - DNS Configuration</title>
</svelte:head>

<div class="dns-page">
  <div class="header-row">
    <h1>DNS Configuration</h1>
    <div class="header-actions">
      <span class="status-badge" class:active={config.enabled} class:inactive={!config.enabled}>
        {config.enabled ? 'Enabled' : 'Disabled'}
      </span>
      <button class="btn btn-refresh" on:click={fetchStatus} disabled={loading}>
        {loading ? 'Refreshing...' : 'Refresh'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="error-banner">
      <span>{error}</span>
      <button class="btn-close" on:click={clearMessages}>&times;</button>
    </div>
  {/if}

  {#if success}
    <div class="success-banner">
      <span>{success}</span>
      <button class="btn-close" on:click={clearMessages}>&times;</button>
    </div>
  {/if}

  <!-- Tabs -->
  <div class="tabs">
    <button class="tab" class:active={activeTab === 'config'} on:click={() => activeTab = 'config'}>
      Configuration
    </button>
    <button class="tab" class:active={activeTab === 'status'} on:click={() => activeTab = 'status'}>
      Status & Cache
    </button>
    <button class="tab" class:active={activeTab === 'advanced'} on:click={() => activeTab = 'advanced'}>
      Advanced
    </button>
  </div>

  <!-- Configuration Tab -->
  {#if activeTab === 'config'}
    <div class="card">
      <h2>DNS Forwarder Settings</h2>
      <form on:submit|preventDefault={saveConfig}>
        <div class="form-row">
          <div class="form-group">
            <label for="dns-interface">Listen Interface</label>
            <select id="dns-interface" bind:value={config.interface}>
              <option value="lan0">lan0 (LAN)</option>
              <option value="lan1">lan1 (LAN)</option>
              <option value="all">All interfaces</option>
            </select>
          </div>
          <div class="form-group">
            <label for="cache-size">Cache Size</label>
            <input type="number" id="cache-size" bind:value={config.cache_size} min="0" max="100000" step="100" />
          </div>
        </div>

        <div class="section-label">Upstream DNS Servers (IPv4)</div>
        <div class="form-row">
          <div class="form-group">
            <label for="upstream-primary">Primary DNS</label>
            <input type="text" id="upstream-primary" bind:value={config.upstream_primary} placeholder="8.8.8.8" />
          </div>
          <div class="form-group">
            <label for="upstream-secondary">Secondary DNS</label>
            <input type="text" id="upstream-secondary" bind:value={config.upstream_secondary} placeholder="1.1.1.1" />
          </div>
        </div>

        <div class="section-label">Upstream DNS Servers (IPv6)</div>
        <div class="form-row">
          <div class="form-group">
            <label for="upstream-v6-primary">Primary DNS</label>
            <input type="text" id="upstream-v6-primary" bind:value={config.upstream_v6_primary} placeholder="2001:4860:4860::8888" />
          </div>
          <div class="form-group">
            <label for="upstream-v6-secondary">Secondary DNS</label>
            <input type="text" id="upstream-v6-secondary" bind:value={config.upstream_v6_secondary} placeholder="2606:4700:4700::1111" />
          </div>
        </div>

        <button type="submit" class="btn btn-save" disabled={saving}>
          {saving ? 'Saving...' : 'Save & Apply'}
        </button>
      </form>
    </div>
  {/if}

  <!-- Status Tab -->
  {#if activeTab === 'status'}
    <div class="card">
      <h2>DNS Resolver Status</h2>
      {#if loading}
        <p class="loading-text">Loading status...</p>
      {:else if dnsStatus}
        <div class="status-grid">
          <div class="status-item">
            <span class="status-label">Service Status</span>
            <span class="status-val" class:text-green={dnsStatus.status === 'active'} class:text-muted={dnsStatus.status !== 'active'}>
              {dnsStatus.status}
            </span>
          </div>
          <div class="status-item">
            <span class="status-label">Listen Interface</span>
            <span class="status-val">{dnsStatus.interface || 'N/A'}</span>
          </div>
          <div class="status-item">
            <span class="status-label">Cache Size</span>
            <span class="status-val">{dnsStatus.cache_size || 0}</span>
          </div>
          <div class="status-item">
            <span class="status-label">IPv4 Upstream</span>
            <span class="status-val mono">{(dnsStatus.upstream || []).join(', ') || 'N/A'}</span>
          </div>
          <div class="status-item">
            <span class="status-label">IPv6 Upstream</span>
            <span class="status-val mono">{(dnsStatus.upstream_v6 || []).join(', ') || 'N/A'}</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- Cache Statistics -->
    <div class="card">
      <h2>Cache Statistics</h2>
      <div class="stats-grid">
        <div class="stat-card">
          <span class="stat-label">Cache Size</span>
          <span class="stat-value">{cacheStats.size}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Hit Rate</span>
          <span class="stat-value">{cacheHitRate()}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Cache Hits</span>
          <span class="stat-value text-green">{cacheStats.hits}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Cache Misses</span>
          <span class="stat-value text-muted">{cacheStats.misses}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Insertions</span>
          <span class="stat-value">{cacheStats.insertions}</span>
        </div>
        <div class="stat-card">
          <span class="stat-label">Evictions</span>
          <span class="stat-value text-muted">{cacheStats.evictions}</span>
        </div>
      </div>
    </div>

    <!-- Upstream Test -->
    <div class="card">
      <h2>Upstream Connectivity</h2>
      <p class="card-desc">Test DNS resolution through configured upstream servers.</p>
      <div class="upstream-list">
        {#each (dnsStatus?.upstream || []) as server}
          <div class="upstream-item">
            <span class="upstream-icon text-green">&#9679;</span>
            <span class="mono">{server}</span>
            <span class="upstream-tag">IPv4</span>
          </div>
        {/each}
        {#each (dnsStatus?.upstream_v6 || []) as server}
          <div class="upstream-item">
            <span class="upstream-icon text-green">&#9679;</span>
            <span class="mono">{server}</span>
            <span class="upstream-tag">IPv6</span>
          </div>
        {/each}
        {#if (!dnsStatus?.upstream || dnsStatus.upstream.length === 0) && (!dnsStatus?.upstream_v6 || dnsStatus.upstream_v6.length === 0)}
          <p class="empty-hint">No upstream servers configured.</p>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Advanced Tab -->
  {#if activeTab === 'advanced'}
    <!-- Custom DNS Records -->
    <div class="card">
      <h2>Custom DNS Records</h2>
      <p class="card-desc">Add static DNS entries for local resolution.</p>

      <form class="inline-form" on:submit|preventDefault={addRecord}>
        <div class="form-row form-row-4">
          <div class="form-group">
            <label for="record-type">Type</label>
            <select id="record-type" bind:value={newRecord.type}>
              <option value="A">A</option>
              <option value="AAAA">AAAA</option>
              <option value="CNAME">CNAME</option>
            </select>
          </div>
          <div class="form-group">
            <label for="record-name">Name</label>
            <input type="text" id="record-name" bind:value={newRecord.name} placeholder="server.lan" />
          </div>
          <div class="form-group">
            <label for="record-value">Value</label>
            <input type="text" id="record-value" bind:value={newRecord.value} placeholder="192.168.1.10" />
          </div>
          <div class="form-group">
            <label for="record-ttl">TTL (sec)</label>
            <input type="number" id="record-ttl" bind:value={newRecord.ttl} min="60" max="86400" step="60" />
          </div>
        </div>
        <div class="form-actions">
          {#if editingRecordIndex !== null}
            <button type="submit" class="btn btn-sm btn-primary">Update</button>
            <button type="button" class="btn btn-sm btn-secondary" on:click={cancelEditRecord}>Cancel</button>
          {:else}
            <button type="submit" class="btn btn-sm btn-primary">Add Record</button>
          {/if}
        </div>
      </form>

      {#if customRecords.length > 0}
        <div class="table-wrapper" style="margin-top: 1rem;">
          <table class="data-table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Name</th>
                <th>Value</th>
                <th>TTL</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each customRecords as record, i}
                <tr>
                  <td><span class="type-badge">{record.type}</span></td>
                  <td class="mono">{record.name}</td>
                  <td class="mono">{record.value}</td>
                  <td>{record.ttl}s</td>
                  <td class="actions-cell">
                    <button class="btn-icon" title="Edit" on:click={() => editRecord(i)}>&#9998;</button>
                    <button class="btn-icon btn-icon-danger" title="Remove" on:click={() => removeRecord(i)}>&#10005;</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="empty-hint">No custom DNS records configured.</p>
      {/if}
    </div>

    <!-- DNS Blocking -->
    <div class="card">
      <h2>DNS Blocking (Ad Blocking)</h2>
      <p class="card-desc">Block advertisements and malicious domains using blocklists.</p>

      <div class="form-group">
        <label class="toggle-label">
          <span class="toggle-switch" class:enabled={blockingEnabled}>
            <input type="checkbox" bind:checked={blockingEnabled} />
            <span class="toggle-slider"></span>
          </span>
          <span>DNS Blocking {blockingEnabled ? 'Enabled' : 'Disabled'}</span>
        </label>
      </div>

      {#if blockingEnabled}
        <div class="blocklist-section">
          <div class="section-label" style="margin-top: 1rem;">Active Blocklists</div>

          {#if blocklists.length > 0}
            <div class="blocklist-list">
              {#each blocklists as bl, i}
                <div class="blocklist-item" class:disabled={!bl.enabled}>
                  <label class="toggle-label" style="flex: 1;">
                    <span class="toggle-switch toggle-sm" class:enabled={bl.enabled}>
                      <input type="checkbox" checked={bl.enabled} on:change={() => toggleBlocklist(i)} />
                      <span class="toggle-slider"></span>
                    </span>
                    <div class="blocklist-info">
                      <span class="blocklist-name">{bl.name}</span>
                      <span class="blocklist-url">{bl.url}</span>
                    </div>
                  </label>
                  <button class="btn-icon btn-icon-danger" title="Remove" on:click={() => removeBlocklist(i)}>&#10005;</button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="empty-hint">No blocklists configured.</p>
          {/if}

          <form class="inline-form" style="margin-top: 1rem;" on:submit|preventDefault={addBlocklist}>
            <div class="form-row">
              <div class="form-group">
                <label for="bl-name">List Name</label>
                <input type="text" id="bl-name" bind:value={newBlocklist.name} placeholder="My Blocklist" />
              </div>
              <div class="form-group">
                <label for="bl-url">List URL</label>
                <input type="text" id="bl-url" bind:value={newBlocklist.url} placeholder="https://..." />
              </div>
            </div>
            <button type="submit" class="btn btn-sm btn-primary">Add Blocklist</button>
          </form>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .dns-page {
    max-width: 900px;
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

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .status-badge {
    font-size: 0.75rem;
    padding: 0.3rem 0.8rem;
    border-radius: 1rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-badge.active {
    background: #00ff8820;
    color: #00ff88;
    border: 1px solid #00ff8840;
  }

  .status-badge.inactive {
    background: #66666620;
    color: #888;
    border: 1px solid #66666640;
  }

  /* Tabs */
  .tabs {
    display: flex;
    gap: 0;
    margin-bottom: 1.5rem;
    border-bottom: 1px solid #333;
  }

  .tab {
    background: none;
    color: #888;
    border: none;
    padding: 0.75rem 1.25rem;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    font-size: 0.9rem;
    font-weight: 500;
    transition: all 0.2s;
    border-radius: 0;
  }

  .tab:hover {
    color: #e0e0e0;
    background: none;
    opacity: 1;
  }

  .tab.active {
    color: #00ff88;
    border-bottom-color: #00ff88;
  }

  /* Cards */
  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
    font-size: 1.15rem;
  }

  .card-desc {
    color: #888;
    font-size: 0.9rem;
    margin: -0.5rem 0 1rem;
  }

  .section-label {
    font-size: 0.8rem;
    color: #00ff88;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 0.5rem;
    margin-top: 0.5rem;
  }

  /* Forms */
  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-row-4 {
    grid-template-columns: 0.6fr 1.2fr 1.5fr 0.7fr;
  }

  .form-actions {
    display: flex;
    gap: 0.5rem;
  }

  .inline-form {
    background: #16213e;
    padding: 1rem;
    border-radius: 0.5rem;
  }

  label {
    font-size: 0.85rem;
    color: #888;
  }

  input, select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.65rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.95rem;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  /* Toggle Switch */
  .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    cursor: pointer;
    font-size: 0.95rem;
    color: #e0e0e0;
  }

  .toggle-switch {
    position: relative;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
  }

  .toggle-switch.toggle-sm {
    width: 36px;
    height: 20px;
  }

  .toggle-switch input {
    opacity: 0;
    width: 0;
    height: 0;
    padding: 0;
    border: none;
  }

  .toggle-slider {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: #444;
    border-radius: 12px;
    transition: background 0.3s;
    cursor: pointer;
  }

  .toggle-slider::before {
    content: '';
    position: absolute;
    width: 18px;
    height: 18px;
    left: 3px;
    top: 3px;
    background: #e0e0e0;
    border-radius: 50%;
    transition: transform 0.3s;
  }

  .toggle-switch.toggle-sm .toggle-slider::before {
    width: 14px;
    height: 14px;
  }

  .toggle-switch.enabled .toggle-slider {
    background: #00ff88;
  }

  .toggle-switch.enabled .toggle-slider::before {
    transform: translateX(20px);
  }

  .toggle-switch.toggle-sm.enabled .toggle-slider::before {
    transform: translateX(16px);
  }

  /* Buttons */
  .btn {
    padding: 0.5rem 1rem;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    transition: all 0.2s;
    color: #fff;
    background: #333;
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

  .btn-save {
    background: #00ff88;
    color: #0f0f23;
    font-weight: 600;
    padding: 0.7rem 1.5rem;
  }

  .btn-save:hover:not(:disabled) {
    opacity: 0.9;
  }

  .btn-sm {
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
  }

  .btn-primary {
    background: #00ff88;
    color: #0f0f23;
  }

  .btn-secondary {
    background: #444;
    color: #e0e0e0;
  }

  .btn-close {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 1.2rem;
    padding: 0 0.25rem;
    opacity: 0.7;
  }

  .btn-close:hover {
    opacity: 1;
  }

  /* Banners */
  .error-banner {
    background: #ff444422;
    border: 1px solid #ff4444;
    color: #ff8888;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .success-banner {
    background: #00ff8822;
    border: 1px solid #00ff88;
    color: #00ff88;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  /* Status Grid */
  .status-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .status-item {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    padding: 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .status-label {
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-val {
    font-size: 0.95rem;
    color: #e0e0e0;
  }

  /* Stats Grid */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
  }

  .stat-card {
    background: #16213e;
    padding: 1rem;
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-label {
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .stat-value {
    font-size: 1.25rem;
    font-weight: bold;
    color: #00ff88;
  }

  .text-green { color: #00ff88; }
  .text-muted { color: #888; }

  /* Upstream List */
  .upstream-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .upstream-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 0.75rem;
    background: #16213e;
    border-radius: 0.4rem;
  }

  .upstream-icon {
    font-size: 0.6rem;
  }

  .upstream-tag {
    margin-left: auto;
    font-size: 0.7rem;
    color: #888;
    background: #333;
    padding: 0.15rem 0.5rem;
    border-radius: 0.75rem;
  }

  /* Table */
  .table-wrapper {
    overflow-x: auto;
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  .data-table th {
    text-align: left;
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid #333;
    color: #888;
    font-weight: 600;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .data-table td {
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid #222;
    color: #e0e0e0;
  }

  .data-table tbody tr:hover {
    background: #16213e;
  }

  .mono {
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    font-size: 0.85rem;
  }

  .type-badge {
    background: #00ff8820;
    color: #00ff88;
    padding: 0.15rem 0.5rem;
    border-radius: 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .actions-cell {
    display: flex;
    gap: 0.25rem;
  }

  .btn-icon {
    background: none;
    border: 1px solid #444;
    color: #aaa;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    border-radius: 0.3rem;
    font-size: 0.85rem;
    transition: all 0.2s;
  }

  .btn-icon:hover {
    border-color: #00ff88;
    color: #00ff88;
  }

  .btn-icon-danger:hover {
    border-color: #ff4444;
    color: #ff4444;
  }

  /* Blocklist */
  .blocklist-section {
    margin-top: 0.5rem;
  }

  .blocklist-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .blocklist-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 0.75rem;
    background: #16213e;
    border-radius: 0.4rem;
    border: 1px solid #333;
  }

  .blocklist-item.disabled {
    opacity: 0.5;
  }

  .blocklist-info {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .blocklist-name {
    font-size: 0.9rem;
    color: #e0e0e0;
  }

  .blocklist-url {
    font-size: 0.75rem;
    color: #666;
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 500px;
  }

  /* Empty states */
  .empty-hint {
    color: #555;
    font-size: 0.85rem;
    font-style: italic;
    padding: 0.5rem 0;
  }

  .loading-text {
    color: #888;
    font-style: italic;
  }
</style>
