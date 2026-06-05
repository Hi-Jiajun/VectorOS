<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // ── Types ──────────────────────────────────────────────────────────
  interface DhcpLease {
    mac: string;
    ip: string;
    hostname: string;
    expires: string;
  }

  interface DhcpStatusData {
    status: string;
    leases: DhcpLease[];
  }

  interface StaticLease {
    mac: string;
    ip: string;
    hostname: string;
  }

  // ── State ──────────────────────────────────────────────────────────
  let activeTab: 'status' | 'config' | 'leases' | 'statistics' | 'advanced' = 'status';
  let dhcpStatus: DhcpStatusData | null = null;
  let leases: DhcpLease[] = [];
  let loading = true;
  let error = '';
  let success = '';
  let saving = false;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // Configuration form
  let config = {
    enabled: false,
    interface: 'lan0',
    start_ip: '192.168.1.100',
    end_ip: '192.168.1.200',
    gateway: '192.168.1.1',
    lease_time: 86400,
    dns_mode: 'auto' as 'auto' | 'custom',
    dns_servers: '8.8.8.8,1.1.1.1',
  };

  // Static leases
  let staticLeases: StaticLease[] = [];
  let newStaticLease: StaticLease = { mac: '', ip: '', hostname: '' };
  let editingStaticIndex: number | null = null;

  // DHCP options
  let dhcpOptions = {
    ntp_servers: '',
    domain_name: '',
    domain_search: '',
    mtu: '',
    wins_server: '',
  };

  // ── Lifecycle ──────────────────────────────────────────────────────
  onMount(async () => {
    await fetchStatus();
    refreshInterval = setInterval(fetchStatus, 5000);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  // ── Data fetching ──────────────────────────────────────────────────
  async function fetchStatus() {
    try {
      if (!dhcpStatus) loading = true;
      error = '';
      const res = await fetch('/api/dhcp/status');
      const data: DhcpStatusData = await res.json();
      if ((data as any).error) {
        error = (data as any).error;
      } else {
        dhcpStatus = data;
        leases = data.leases || [];
        config.enabled = data.status === 'active';
      }
    } catch (e) {
      error = 'Failed to fetch DHCP status';
    } finally {
      loading = false;
    }
  }

  // ── Enable / Disable ──────────────────────────────────────────────
  async function toggleDhcp() {
    try {
      saving = true;
      error = '';
      success = '';

      if (config.enabled) {
        // Enable the server
        const payload: any = {
          interface: config.interface,
          start_ip: config.start_ip,
          end_ip: config.end_ip,
          gateway: config.gateway,
          lease_time: config.lease_time,
        };
        if (config.dns_mode === 'custom') {
          payload.dns_servers = config.dns_servers;
        }

        const res = await fetch('/api/dhcp/enable', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
        });
        const data = await res.json();
        if (data.error) {
          error = data.error;
          config.enabled = false;
        } else {
          success = data.message || 'DHCP server enabled successfully';
        }
      } else {
        // Disable the server
        const res = await fetch('/api/dhcp/disable', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
        });
        const data = await res.json();
        if (data.error) {
          error = data.error;
          config.enabled = true;
        } else {
          success = data.message || 'DHCP server disabled successfully';
        }
      }

      await fetchStatus();
    } catch (e) {
      error = 'Failed to toggle DHCP server';
    } finally {
      saving = false;
    }
  }

  async function saveConfig() {
    try {
      saving = true;
      error = '';
      success = '';

      const payload: any = {
        interface: config.interface,
        start_ip: config.start_ip,
        end_ip: config.end_ip,
        gateway: config.gateway,
        lease_time: config.lease_time,
      };
      if (config.dns_mode === 'custom') {
        payload.dns_servers = config.dns_servers;
      }

      const res = await fetch('/api/dhcp/enable', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'DHCP configuration saved successfully';
        config.enabled = true;
        await fetchStatus();
      }
    } catch (e) {
      error = 'Failed to save DHCP configuration';
    } finally {
      saving = false;
    }
  }

  // ── Static Leases ─────────────────────────────────────────────────
  function addStaticLease() {
    if (!newStaticLease.mac || !newStaticLease.ip) return;
    if (editingStaticIndex !== null) {
      staticLeases[editingStaticIndex] = { ...newStaticLease };
      editingStaticIndex = null;
    } else {
      staticLeases = [...staticLeases, { ...newStaticLease }];
    }
    newStaticLease = { mac: '', ip: '', hostname: '' };
  }

  function editStaticLease(index: number) {
    newStaticLease = { ...staticLeases[index] };
    editingStaticIndex = index;
  }

  function removeStaticLease(index: number) {
    staticLeases = staticLeases.filter((_, i) => i !== index);
    if (editingStaticIndex === index) {
      editingStaticIndex = null;
      newStaticLease = { mac: '', ip: '', hostname: '' };
    }
  }

  function cancelEditStatic() {
    editingStaticIndex = null;
    newStaticLease = { mac: '', ip: '', hostname: '' };
  }

  // ── Helpers ────────────────────────────────────────────────────────
  function formatLeaseExpiry(expires: string): string {
    if (!expires || expires === '0') return 'Permanent';
    const ts = parseInt(expires, 10);
    if (isNaN(ts)) return expires;
    const now = Math.floor(Date.now() / 1000);
    const remaining = ts - now;
    if (remaining <= 0) return 'Expired';
    const hours = Math.floor(remaining / 3600);
    const mins = Math.floor((remaining % 3600) / 60);
    if (hours > 24) {
      const days = Math.floor(hours / 24);
      return `${days}d ${hours % 24}h remaining`;
    }
    return `${hours}h ${mins}m remaining`;
  }

  function leaseIsExpired(expires: string): boolean {
    if (!expires || expires === '0') return false;
    const ts = parseInt(expires, 10);
    if (isNaN(ts)) return false;
    return ts <= Math.floor(Date.now() / 1000);
  }

  function leaseTimeDisplay(seconds: number): string {
    if (seconds >= 86400) return `${seconds / 86400} day(s)`;
    if (seconds >= 3600) return `${seconds / 3600} hour(s)`;
    return `${seconds / 60} minute(s)`;
  }

  function ipToInt(ip: string): number {
    const parts = ip.split('.').map(Number);
    return ((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]) >>> 0;
  }

  function getPoolSize(): number {
    const start = ipToInt(config.start_ip);
    const end = ipToInt(config.end_ip);
    if (end >= start) return end - start + 1;
    return 0;
  }

  function getActiveLeasesCount(): number {
    return leases.filter(l => !leaseIsExpired(l.expires)).length;
  }

  function getExpiredLeasesCount(): number {
    return leases.filter(l => leaseIsExpired(l.expires)).length;
  }

  function getPoolUtilization(): number {
    const poolSize = getPoolSize();
    if (poolSize <= 0) return 0;
    return Math.min(100, (getActiveLeasesCount() / poolSize) * 100);
  }

  function barColor(percent: number): string {
    if (percent > 90) return '#ff4444';
    if (percent > 75) return '#ff8800';
    if (percent > 50) return '#ffaa00';
    return '#00ff88';
  }

  function clearMessages() {
    error = '';
    success = '';
  }
</script>

<svelte:head>
  <title>VectorOS - DHCP Management</title>
</svelte:head>

<div class="dhcp-page">
  <!-- Header -->
  <div class="header-row">
    <h1>DHCP Management</h1>
    <div class="header-actions">
      <span class="status-badge" class:active={config.enabled} class:inactive={!config.enabled}>
        {config.enabled ? 'Server Running' : 'Server Stopped'}
      </span>
      <button class="btn btn-refresh" on:click={fetchStatus} disabled={loading}>
        {loading ? 'Refreshing...' : 'Refresh'}
      </button>
    </div>
  </div>

  <!-- Banners -->
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
    <button class="tab" class:active={activeTab === 'status'} on:click={() => activeTab = 'status'}>
      Status
    </button>
    <button class="tab" class:active={activeTab === 'config'} on:click={() => activeTab = 'config'}>
      Configuration
    </button>
    <button class="tab" class:active={activeTab === 'leases'} on:click={() => activeTab = 'leases'}>
      Leases {leases.length > 0 ? `(${leases.length})` : ''}
    </button>
    <button class="tab" class:active={activeTab === 'statistics'} on:click={() => activeTab = 'statistics'}>
      Statistics
    </button>
    <button class="tab" class:active={activeTab === 'advanced'} on:click={() => activeTab = 'advanced'}>
      Advanced
    </button>
  </div>

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- STATUS TAB                                                     -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'status'}
    <!-- Quick Stats -->
    <div class="stats-grid">
      <div class="stat-card">
        <span class="stat-label">Server Status</span>
        <span class="stat-value" class:text-green={config.enabled} class:text-red={!config.enabled}>
          {config.enabled ? 'Running' : 'Stopped'}
        </span>
      </div>
      <div class="stat-card">
        <span class="stat-label">Active Leases</span>
        <span class="stat-value">{getActiveLeasesCount()}</span>
      </div>
      <div class="stat-card">
        <span class="stat-label">Pool Size</span>
        <span class="stat-value">{getPoolSize()}</span>
        <span class="stat-sub">{config.start_ip} - {config.end_ip}</span>
      </div>
      <div class="stat-card">
        <span class="stat-label">Pool Utilization</span>
        <span class="stat-value" style="color: {barColor(getPoolUtilization())}">
          {getPoolUtilization().toFixed(1)}%
        </span>
        <div class="mini-progress">
          <div class="mini-progress-fill" style="width: {getPoolUtilization()}%; background: {barColor(getPoolUtilization())}"></div>
        </div>
      </div>
    </div>

    <!-- Server Control -->
    <div class="card">
      <h2>Server Control</h2>
      <div class="control-row">
        <div class="control-info">
          <p class="control-desc">
            {#if config.enabled}
              The DHCP server is currently running on <strong>{config.interface}</strong>, serving addresses in the range <strong>{config.start_ip} - {config.end_ip}</strong>.
            {:else}
              The DHCP server is currently stopped. Enable it to start assigning IP addresses to clients.
            {/if}
          </p>
        </div>
        <label class="toggle-label">
          <span class="toggle-switch" class:enabled={config.enabled}>
            <input type="checkbox" checked={config.enabled} on:change={toggleDhcp} disabled={saving} />
            <span class="toggle-slider"></span>
          </span>
          <span class="toggle-text">{config.enabled ? 'ON' : 'OFF'}</span>
        </label>
      </div>
    </div>

    <!-- Recent Leases Preview -->
    {#if leases.length > 0}
      <div class="card">
        <div class="card-header">
          <h2>Recent Leases</h2>
          <button class="btn btn-sm btn-secondary" on:click={() => activeTab = 'leases'}>View All</button>
        </div>
        <div class="table-wrapper">
          <table class="data-table">
            <thead>
              <tr>
                <th>MAC Address</th>
                <th>IP Address</th>
                <th>Hostname</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each leases.slice(0, 5) as lease}
                <tr>
                  <td class="mono">{lease.mac}</td>
                  <td class="mono">{lease.ip}</td>
                  <td>{lease.hostname || '-'}</td>
                  <td>
                    {#if leaseIsExpired(lease.expires)}
                      <span class="lease-status expired">Expired</span>
                    {:else}
                      <span class="lease-status active">Active</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- CONFIGURATION TAB                                              -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'config'}
    <div class="card">
      <h2>DHCP Server Settings</h2>
      <form on:submit|preventDefault={saveConfig}>
        <div class="form-row">
          <div class="form-group">
            <label for="dhcp-interface">Interface</label>
            <select id="dhcp-interface" bind:value={config.interface}>
              <option value="lan0">lan0 (LAN)</option>
              <option value="lan1">lan1 (LAN)</option>
            </select>
          </div>
          <div class="form-group">
            <label for="lease-time">Lease Time</label>
            <select id="lease-time" bind:value={config.lease_time}>
              <option value={3600}>1 hour</option>
              <option value={43200}>12 hours</option>
              <option value={86400}>1 day (default)</option>
              <option value={604800}>7 days</option>
              <option value={2592000}>30 days</option>
            </select>
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="start-ip">IP Range Start</label>
            <input type="text" id="start-ip" bind:value={config.start_ip} placeholder="192.168.1.100" />
          </div>
          <div class="form-group">
            <label for="end-ip">IP Range End</label>
            <input type="text" id="end-ip" bind:value={config.end_ip} placeholder="192.168.1.200" />
          </div>
        </div>

        <div class="form-group">
          <label for="gateway">Gateway IP</label>
          <input type="text" id="gateway" bind:value={config.gateway} placeholder="192.168.1.1" />
        </div>

        <div class="form-group">
          <label>DNS Mode</label>
          <div class="radio-group">
            <label class="radio-label">
              <input type="radio" bind:group={config.dns_mode} value="auto" />
              <span>Auto (use system upstream DNS)</span>
            </label>
            <label class="radio-label">
              <input type="radio" bind:group={config.dns_mode} value="custom" />
              <span>Custom DNS servers</span>
            </label>
          </div>
        </div>

        {#if config.dns_mode === 'custom'}
          <div class="form-group">
            <label for="dns-servers">DNS Servers (comma-separated)</label>
            <input type="text" id="dns-servers" bind:value={config.dns_servers} placeholder="8.8.8.8,1.1.1.1" />
          </div>
        {/if}

        <div class="form-actions">
          <button type="submit" class="btn btn-save" disabled={saving}>
            {saving ? 'Saving...' : 'Save Configuration'}
          </button>
        </div>
      </form>
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- LEASES TAB                                                     -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'leases'}
    <!-- Lease Filters -->
    <div class="card">
      <div class="card-header">
        <h2>Active Leases</h2>
        <div class="card-actions">
          <span class="count-badge">{getActiveLeasesCount()} active</span>
          <button class="btn btn-sm btn-secondary" on:click={fetchStatus}>Refresh</button>
        </div>
      </div>

      {#if loading}
        <p class="loading-text">Loading leases...</p>
      {:else if leases.length === 0}
        <div class="empty-state">
          <p>No DHCP leases found.</p>
          <p class="hint">Leases will appear here when clients connect to the DHCP server.</p>
        </div>
      {:else}
        <div class="table-wrapper">
          <table class="data-table">
            <thead>
              <tr>
                <th>MAC Address</th>
                <th>IP Address</th>
                <th>Hostname</th>
                <th>Expires</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each leases as lease}
                <tr class:row-expired={leaseIsExpired(lease.expires)}>
                  <td class="mono">{lease.mac}</td>
                  <td class="mono">{lease.ip}</td>
                  <td>{lease.hostname || '-'}</td>
                  <td>{formatLeaseExpiry(lease.expires)}</td>
                  <td>
                    {#if leaseIsExpired(lease.expires)}
                      <span class="lease-status expired">Expired</span>
                    {:else}
                      <span class="lease-status active">Active</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- Expired Leases -->
    {#if getExpiredLeasesCount() > 0}
      <div class="card">
        <div class="card-header">
          <h2>Expired Leases</h2>
          <span class="count-badge expired-badge">{getExpiredLeasesCount()} expired</span>
        </div>
        <div class="table-wrapper">
          <table class="data-table">
            <thead>
              <tr>
                <th>MAC Address</th>
                <th>IP Address</th>
                <th>Hostname</th>
                <th>Expired</th>
              </tr>
            </thead>
            <tbody>
              {#each leases.filter(l => leaseIsExpired(l.expires)) as lease}
                <tr class="row-expired">
                  <td class="mono">{lease.mac}</td>
                  <td class="mono">{lease.ip}</td>
                  <td>{lease.hostname || '-'}</td>
                  <td class="text-muted">{formatLeaseExpiry(lease.expires)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- STATISTICS TAB                                                 -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'statistics'}
    <div class="stats-grid">
      <div class="stat-card large">
        <span class="stat-label">Total Leases</span>
        <span class="stat-value">{leases.length}</span>
      </div>
      <div class="stat-card large">
        <span class="stat-label">Active Leases</span>
        <span class="stat-value text-green">{getActiveLeasesCount()}</span>
      </div>
      <div class="stat-card large">
        <span class="stat-label">Expired Leases</span>
        <span class="stat-value text-red">{getExpiredLeasesCount()}</span>
      </div>
      <div class="stat-card large">
        <span class="stat-label">Available Addresses</span>
        <span class="stat-value">{Math.max(0, getPoolSize() - getActiveLeasesCount())}</span>
      </div>
    </div>

    <!-- Pool Utilization -->
    <div class="card">
      <h2>Address Pool Utilization</h2>
      <div class="pool-visualization">
        <div class="pool-bar-container">
          <div class="pool-bar">
            <div
              class="pool-bar-fill"
              style="width: {getPoolUtilization()}%; background: {barColor(getPoolUtilization())}"
            ></div>
          </div>
        </div>
        <div class="pool-details">
          <div class="pool-detail">
            <span class="pool-detail-label">Pool Range</span>
            <span class="pool-detail-value">{config.start_ip} - {config.end_ip}</span>
          </div>
          <div class="pool-detail">
            <span class="pool-detail-label">Total Addresses</span>
            <span class="pool-detail-value">{getPoolSize()}</span>
          </div>
          <div class="pool-detail">
            <span class="pool-detail-label">Used</span>
            <span class="pool-detail-value" style="color: {barColor(getPoolUtilization())}">{getActiveLeasesCount()}</span>
          </div>
          <div class="pool-detail">
            <span class="pool-detail-label">Available</span>
            <span class="pool-detail-value">{Math.max(0, getPoolSize() - getActiveLeasesCount())}</span>
          </div>
          <div class="pool-detail">
            <span class="pool-detail-label">Utilization</span>
            <span class="pool-detail-value" style="color: {barColor(getPoolUtilization())}">{getPoolUtilization().toFixed(1)}%</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Server Info -->
    <div class="card">
      <h2>Server Information</h2>
      <div class="info-grid">
        <div class="info-item">
          <span class="info-label">Status</span>
          <span class="info-value" class:text-green={config.enabled} class:text-red={!config.enabled}>
            {config.enabled ? 'Running' : 'Stopped'}
          </span>
        </div>
        <div class="info-item">
          <span class="info-label">Interface</span>
          <span class="info-value">{config.interface}</span>
        </div>
        <div class="info-item">
          <span class="info-label">Gateway</span>
          <span class="info-value mono">{config.gateway}</span>
        </div>
        <div class="info-item">
          <span class="info-label">Lease Time</span>
          <span class="info-value">{leaseTimeDisplay(config.lease_time)}</span>
        </div>
        <div class="info-item">
          <span class="info-label">DNS Servers</span>
          <span class="info-value mono">{config.dns_mode === 'auto' ? 'System default' : config.dns_servers}</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- ADVANCED TAB                                                   -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'advanced'}
    <!-- Static Leases -->
    <div class="card">
      <h2>Static Leases (MAC-IP Binding)</h2>
      <p class="card-desc">Assign fixed IP addresses to specific devices based on their MAC address.</p>

      <form class="inline-form" on:submit|preventDefault={addStaticLease}>
        <div class="form-row form-row-3">
          <div class="form-group">
            <label for="static-mac">MAC Address</label>
            <input type="text" id="static-mac" bind:value={newStaticLease.mac} placeholder="AA:BB:CC:DD:EE:FF" />
          </div>
          <div class="form-group">
            <label for="static-ip">IP Address</label>
            <input type="text" id="static-ip" bind:value={newStaticLease.ip} placeholder="192.168.1.50" />
          </div>
          <div class="form-group">
            <label for="static-hostname">Hostname (optional)</label>
            <input type="text" id="static-hostname" bind:value={newStaticLease.hostname} placeholder="my-server" />
          </div>
        </div>
        <div class="form-actions">
          {#if editingStaticIndex !== null}
            <button type="submit" class="btn btn-sm btn-primary">Update</button>
            <button type="button" class="btn btn-sm btn-secondary" on:click={cancelEditStatic}>Cancel</button>
          {:else}
            <button type="submit" class="btn btn-sm btn-primary">Add Static Lease</button>
          {/if}
        </div>
      </form>

      {#if staticLeases.length > 0}
        <div class="table-wrapper" style="margin-top: 1rem;">
          <table class="data-table">
            <thead>
              <tr>
                <th>MAC Address</th>
                <th>IP Address</th>
                <th>Hostname</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each staticLeases as lease, i}
                <tr>
                  <td class="mono">{lease.mac}</td>
                  <td class="mono">{lease.ip}</td>
                  <td>{lease.hostname || '-'}</td>
                  <td class="actions-cell">
                    <button class="btn-icon" title="Edit" on:click={() => editStaticLease(i)}>&#9998;</button>
                    <button class="btn-icon btn-icon-danger" title="Remove" on:click={() => removeStaticLease(i)}>&#10005;</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="empty-hint">No static leases configured.</p>
      {/if}
    </div>

    <!-- DHCP Options -->
    <div class="card">
      <h2>DHCP Options</h2>
      <p class="card-desc">Configure additional DHCP options provided to clients.</p>

      <div class="form-grid">
        <div class="form-group">
          <label for="ntp-servers">NTP Servers (comma-separated)</label>
          <input type="text" id="ntp-servers" bind:value={dhcpOptions.ntp_servers} placeholder="192.168.1.1, pool.ntp.org" />
        </div>
        <div class="form-group">
          <label for="domain-name">Domain Name</label>
          <input type="text" id="domain-name" bind:value={dhcpOptions.domain_name} placeholder="lan" />
        </div>
        <div class="form-group">
          <label for="domain-search">Domain Search (comma-separated)</label>
          <input type="text" id="domain-search" bind:value={dhcpOptions.domain_search} placeholder="lan,example.com" />
        </div>
        <div class="form-group">
          <label for="dhcp-mtu">MTU</label>
          <input type="text" id="dhcp-mtu" bind:value={dhcpOptions.mtu} placeholder="1500" />
        </div>
        <div class="form-group">
          <label for="wins-server">WINS Server</label>
          <input type="text" id="wins-server" bind:value={dhcpOptions.wins_server} placeholder="192.168.1.1" />
        </div>
      </div>

      <button class="btn btn-save" style="margin-top: 1rem;" on:click={() => success = 'DHCP options saved (local only)'}>
        Save DHCP Options
      </button>
    </div>
  {/if}
</div>

<style>
  .dhcp-page {
    max-width: 1000px;
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
    overflow-x: auto;
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
    white-space: nowrap;
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

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }

  .card-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
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

  .count-badge {
    background: #00ff8820;
    color: #00ff88;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    border-radius: 1rem;
    font-weight: 600;
  }

  .count-badge.expired-badge {
    background: #ff444420;
    color: #ff8888;
  }

  /* Stats Grid */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .stat-card {
    background: #1a1a2e;
    padding: 1rem;
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-card.large {
    padding: 1.25rem;
  }

  .stat-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .stat-value {
    font-size: 1.25rem;
    font-weight: bold;
    color: #00ff88;
  }

  .stat-card.large .stat-value {
    font-size: 1.75rem;
  }

  .stat-sub {
    font-size: 0.8rem;
    color: #666;
  }

  .mini-progress {
    height: 4px;
    background: #333;
    border-radius: 2px;
    overflow: hidden;
    margin-top: 0.5rem;
  }

  .mini-progress-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.5s ease;
  }

  .text-green {
    color: #00ff88;
  }

  .text-red {
    color: #ff4444;
  }

  .text-muted {
    color: #888;
  }

  /* Server Control */
  .control-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 2rem;
  }

  .control-info {
    flex: 1;
  }

  .control-desc {
    color: #aaa;
    font-size: 0.9rem;
    line-height: 1.5;
  }

  .control-desc strong {
    color: #e0e0e0;
  }

  /* Toggle Switch */
  .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    cursor: pointer;
    flex-shrink: 0;
  }

  .toggle-text {
    font-size: 0.85rem;
    font-weight: 600;
    color: #e0e0e0;
    min-width: 2rem;
  }

  .toggle-switch {
    position: relative;
    width: 52px;
    height: 28px;
    flex-shrink: 0;
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
    border-radius: 14px;
    transition: background 0.3s;
    cursor: pointer;
  }

  .toggle-slider::before {
    content: '';
    position: absolute;
    width: 22px;
    height: 22px;
    left: 3px;
    top: 3px;
    background: #e0e0e0;
    border-radius: 50%;
    transition: transform 0.3s;
  }

  .toggle-switch.enabled .toggle-slider {
    background: #00ff88;
  }

  .toggle-switch.enabled .toggle-slider::before {
    transform: translateX(24px);
  }

  .toggle-switch input:disabled ~ .toggle-slider {
    opacity: 0.5;
    cursor: not-allowed;
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

  .form-row-3 {
    grid-template-columns: 2fr 1.5fr 1.5fr;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
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

  /* Radio buttons */
  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .radio-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    color: #e0e0e0;
    font-size: 0.9rem;
  }

  .radio-label input[type="radio"] {
    width: auto;
    padding: 0;
    accent-color: #00ff88;
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

  .row-expired {
    opacity: 0.5;
  }

  .lease-status {
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    border-radius: 0.25rem;
    font-weight: 600;
    text-transform: uppercase;
  }

  .lease-status.active {
    background: #003322;
    color: #00ff88;
  }

  .lease-status.expired {
    background: #330000;
    color: #ff6666;
  }

  /* Pool Visualization */
  .pool-visualization {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .pool-bar-container {
    width: 100%;
  }

  .pool-bar {
    height: 20px;
    background: #333;
    border-radius: 4px;
    overflow: hidden;
  }

  .pool-bar-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.5s ease;
  }

  .pool-details {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 1rem;
  }

  .pool-detail {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .pool-detail-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .pool-detail-value {
    font-size: 1.1rem;
    font-weight: bold;
    color: #e0e0e0;
  }

  /* Info Grid */
  .info-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
  }

  .info-item {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem;
    background: #16213e;
    border-radius: 0.5rem;
  }

  .info-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .info-value {
    font-size: 1rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  /* Empty states */
  .empty-state {
    text-align: center;
    padding: 2rem;
    color: #666;
  }

  .empty-state p {
    margin: 0.25rem 0;
  }

  .hint {
    font-size: 0.85rem;
    color: #555;
  }

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
