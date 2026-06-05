<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // ---------------------------------------------------------------------------
  // Types
  // ---------------------------------------------------------------------------

  interface SystemInfo {
    hostname: string;
    os_version: string;
    kernel_version: string;
    uptime: string;
    architecture: string;
    cpu_count: number;
    memory_total: number;
    disk_total: number;
  }

  interface ServiceStatus {
    name: string;
    display_name: string;
    state: string;
  }

  interface NetworkConfig {
    wan_interface: string;
    lan_interface: string;
    wan_dhcp: boolean;
    wan_ip: string;
    wan_netmask: string;
    wan_gateway: string;
    lan_ip: string;
    lan_netmask: string;
    lan_dhcp_enabled: boolean;
  }

  interface DnsConfig {
    upstream: string[];
    cache_size: number;
    listen_address: string;
  }

  interface LogEntry {
    timestamp: string;
    level: string;
    source: string;
    message: string;
  }

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let activeTab: 'system' | 'network' | 'services' | 'actions' | 'logs' = 'system';
  let loading = true;
  let error = '';
  let success = '';
  let actionInProgress = '';

  // System info
  let systemInfo: SystemInfo | null = null;

  // Services
  let services: ServiceStatus[] = [];

  // Network config
  let networkConfig: NetworkConfig = {
    wan_interface: 'eth0',
    lan_interface: 'eth1',
    wan_dhcp: true,
    wan_ip: '',
    wan_netmask: '',
    wan_gateway: '',
    lan_ip: '192.168.1.1',
    lan_netmask: '255.255.255.0',
    lan_dhcp_enabled: true,
  };
  let networkLoading = false;
  let networkSaving = false;

  // DNS config
  let dnsConfig: DnsConfig = {
    upstream: ['8.8.8.8', '1.1.1.1'],
    cache_size: 1000,
    listen_address: '127.0.0.1',
  };
  let dnsSaving = false;

  // Logs
  let logSource = 'vpp,dnsmasq,vectoros';
  let logLevel = 'debug';
  let logLines = 500;
  let logKeyword = '';
  let logLimit = 100;
  let logEntries: LogEntry[] = [];
  let logsLoading = false;

  // Backup/restore
  let backupDescription = '';
  let restoreJson = '';
  let restoreLoading = false;
  let backupResult: any = null;
  let restoreResult: any = null;

  // Factory reset
  let factoryResetConfirm = '';
  let factoryResetLoading = false;

  // Refresh
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  onMount(() => {
    loadAll();
    refreshInterval = setInterval(loadSystemInfo, 30000);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  // ---------------------------------------------------------------------------
  // Data loading
  // ---------------------------------------------------------------------------

  async function loadAll() {
    loading = true;
    error = '';
    try {
      await Promise.all([
        loadSystemInfo(),
        loadServices(),
        loadNetworkConfig(),
        loadDnsConfig(),
      ]);
    } catch (e) {
      error = 'Failed to load system data';
    } finally {
      loading = false;
    }
  }

  async function loadSystemInfo() {
    try {
      const res = await fetch('/api/system');
      const data = await res.json();
      if (data.status === 'ok') {
        systemInfo = data.info || data;
      }
    } catch {
      // Silently handle - will show stale data
    }
  }

  async function loadServices() {
    try {
      const res = await fetch('/api/services');
      const data = await res.json();
      if (data.status === 'ok') {
        services = data.services || [];
      }
    } catch {
      // Silently handle
    }
  }

  async function loadNetworkConfig() {
    networkLoading = true;
    try {
      const res = await fetch('/api/config');
      const data = await res.json();
      if (data.status === 'ok' && data.config) {
        const cfg = data.config;
        networkConfig = {
          wan_interface: cfg.network?.wan_interface || 'eth0',
          lan_interface: cfg.network?.lan_interface || 'eth1',
          wan_dhcp: cfg.network?.wan_dhcp ?? true,
          wan_ip: cfg.network?.wan_ip || '',
          wan_netmask: cfg.network?.wan_netmask || '',
          wan_gateway: cfg.network?.wan_gateway || '',
          lan_ip: cfg.network?.lan_ip || '192.168.1.1',
          lan_netmask: cfg.network?.lan_netmask || '255.255.255.0',
          lan_dhcp_enabled: cfg.dhcp?.enabled ?? true,
        };
        if (cfg.dns) {
          dnsConfig = {
            upstream: cfg.dns.upstream || ['8.8.8.8', '1.1.1.1'],
            cache_size: cfg.dns.cache_size || 1000,
            listen_address: cfg.dns.listen_address || '127.0.0.1',
          };
        }
      }
    } catch {
      // Use defaults
    } finally {
      networkLoading = false;
    }
  }

  async function loadDnsConfig() {
    // DNS config is loaded as part of network config
  }

  // ---------------------------------------------------------------------------
  // Service actions
  // ---------------------------------------------------------------------------

  async function serviceAction(name: string, action: string) {
    actionInProgress = `${name}-${action}`;
    error = '';
    success = '';
    try {
      const res = await fetch(`/api/services/${name}/${action}`, { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        success = `${name} ${action} completed`;
        setTimeout(() => success = '', 3000);
        await loadServices();
      } else {
        error = data.error || `${action} failed`;
      }
    } catch {
      error = `Failed to ${action} service`;
    } finally {
      actionInProgress = '';
    }
  }

  // ---------------------------------------------------------------------------
  // Network save
  // ---------------------------------------------------------------------------

  async function saveNetworkConfig() {
    networkSaving = true;
    error = '';
    success = '';
    try {
      // Save WAN interface
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'network.wan_interface', value: networkConfig.wan_interface })
      });
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'network.lan_interface', value: networkConfig.lan_interface })
      });
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'network.wan_dhcp', value: networkConfig.wan_dhcp })
      });
      if (!networkConfig.wan_dhcp) {
        await fetch('/api/config/set', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: 'network.wan_ip', value: networkConfig.wan_ip })
        });
        await fetch('/api/config/set', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: 'network.wan_netmask', value: networkConfig.wan_netmask })
        });
        await fetch('/api/config/set', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path: 'network.wan_gateway', value: networkConfig.wan_gateway })
        });
      }
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'network.lan_ip', value: networkConfig.lan_ip })
      });
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'network.lan_netmask', value: networkConfig.lan_netmask })
      });

      // Commit changes
      const commitRes = await fetch('/api/config/commit', { method: 'POST' });
      const commitData = await commitRes.json();

      if (commitData.error) {
        error = commitData.error;
      } else {
        success = 'Network configuration saved';
        setTimeout(() => success = '', 3000);
      }
    } catch {
      error = 'Failed to save network configuration';
    } finally {
      networkSaving = false;
    }
  }

  // ---------------------------------------------------------------------------
  // DNS save
  // ---------------------------------------------------------------------------

  async function saveDnsConfig() {
    dnsSaving = true;
    error = '';
    success = '';
    try {
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'dns.upstream', value: dnsConfig.upstream })
      });
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'dns.cache_size', value: dnsConfig.cache_size })
      });
      await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: 'dns.listen_address', value: dnsConfig.listen_address })
      });

      const commitRes = await fetch('/api/config/commit', { method: 'POST' });
      const commitData = await commitRes.json();

      if (commitData.error) {
        error = commitData.error;
      } else {
        success = 'DNS configuration saved';
        setTimeout(() => success = '', 3000);
      }
    } catch {
      error = 'Failed to save DNS configuration';
    } finally {
      dnsSaving = false;
    }
  }

  // ---------------------------------------------------------------------------
  // System actions
  // ---------------------------------------------------------------------------

  async function rebootSystem() {
    if (!confirm('Are you sure you want to reboot the system?')) return;
    actionInProgress = 'reboot';
    error = '';
    try {
      const res = await fetch('/api/system/reboot', { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        success = 'System reboot initiated. The page will reload shortly.';
      } else {
        error = data.error || 'Reboot failed';
      }
    } catch {
      error = 'Failed to send reboot command';
    } finally {
      actionInProgress = '';
    }
  }

  async function shutdownSystem() {
    if (!confirm('Are you sure you want to shut down the system? This will power off the router.')) return;
    actionInProgress = 'shutdown';
    error = '';
    try {
      const res = await fetch('/api/system/shutdown', { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        success = 'System shutdown initiated.';
      } else {
        error = data.error || 'Shutdown failed';
      }
    } catch {
      error = 'Failed to send shutdown command';
    } finally {
      actionInProgress = '';
    }
  }

  async function factoryReset() {
    if (factoryResetConfirm !== 'RESET') {
      error = 'Type RESET to confirm factory reset';
      return;
    }
    if (!confirm('WARNING: This will erase ALL configuration and restore factory defaults. Continue?')) return;
    factoryResetLoading = true;
    error = '';
    try {
      const res = await fetch('/api/system/factory-reset', { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        success = 'Factory reset initiated. The system will reboot.';
        factoryResetConfirm = '';
      } else {
        error = data.error || 'Factory reset failed';
      }
    } catch {
      error = 'Failed to perform factory reset';
    } finally {
      factoryResetLoading = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Backup / Restore
  // ---------------------------------------------------------------------------

  async function backupConfig() {
    error = '';
    try {
      const params = new URLSearchParams();
      if (backupDescription) params.set('description', backupDescription);
      const res = await fetch(`/api/config/export?${params.toString()}`);
      const data = await res.json();

      if (data.status === 'error') {
        error = data.error || 'Backup failed';
        return;
      }

      // Download
      const blob = new Blob([data.data], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `vectoros-backup-${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);

      backupResult = { status: 'ok', message: 'Backup downloaded successfully' };
    } catch {
      error = 'Failed to create backup';
    }
  }

  async function handleRestoreFile(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (e) => {
      restoreJson = (e.target?.result as string) || '';
      restoreResult = null;
    };
    reader.readAsText(file);
  }

  async function restoreConfig() {
    if (!restoreJson.trim()) {
      error = 'Please select or paste a backup file first';
      return;
    }
    if (!confirm('Restore this configuration? Current settings will be overwritten.')) return;
    restoreLoading = true;
    error = '';
    restoreResult = null;
    try {
      const res = await fetch('/api/config/import', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          export_json: restoreJson,
          overwrite: true,
          auto_commit: true,
          description: 'Config restore from backup'
        })
      });
      const data = await res.json();
      if (data.status === 'error') {
        error = data.error || 'Restore failed';
      } else {
        restoreResult = data;
        success = 'Configuration restored successfully';
        setTimeout(() => success = '', 3000);
      }
    } catch {
      error = 'Failed to restore configuration';
    } finally {
      restoreLoading = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Logs
  // ---------------------------------------------------------------------------

  async function fetchLogs() {
    logsLoading = true;
    error = '';
    logEntries = [];
    try {
      const res = await fetch('/api/logs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sources: logSource,
          level: logLevel,
          lines: logLines,
          filter: logKeyword || undefined,
          limit: logLimit,
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        logEntries = data.logs || [];
      }
    } catch {
      error = 'Failed to fetch logs';
    } finally {
      logsLoading = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

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
      case 'running': return '●';
      case 'stopped': return '○';
      case 'starting': return '◑';
      case 'stopping': return '◑';
      case 'failed': return '✖';
      default: return '○';
    }
  }

  function levelColor(level: string): string {
    switch (level?.toLowerCase()) {
      case 'error': return '#ff4444';
      case 'warn': return '#ffaa00';
      case 'info': return '#00ff88';
      case 'debug': return '#888888';
      default: return '#e0e0e0';
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  function clearMsg() { error = ''; success = ''; }

  // DNS upstream management
  let newUpstream = '';
  function addUpstream() {
    if (newUpstream.trim() && !dnsConfig.upstream.includes(newUpstream.trim())) {
      dnsConfig.upstream = [...dnsConfig.upstream, newUpstream.trim()];
      newUpstream = '';
    }
  }
  function removeUpstream(addr: string) {
    dnsConfig.upstream = dnsConfig.upstream.filter(a => a !== addr);
  }
</script>

<svelte:head>
  <title>VectorOS - System Settings</title>
</svelte:head>

<div class="settings-page">
  <h1>System Settings</h1>

  <!-- Messages -->
  {#if error}
    <div class="msg-error">{error} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}
  {#if success}
    <div class="msg-success">{success} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}

  <!-- Tab Navigation -->
  <div class="tab-bar">
    <button class="tab" class:active={activeTab === 'system'} on:click={() => activeTab = 'system'}>
      System Information
    </button>
    <button class="tab" class:active={activeTab === 'network'} on:click={() => activeTab = 'network'}>
      Network Settings
    </button>
    <button class="tab" class:active={activeTab === 'services'} on:click={() => { activeTab = 'services'; loadServices(); }}>
      Service Management
    </button>
    <button class="tab" class:active={activeTab === 'actions'} on:click={() => activeTab = 'actions'}>
      System Actions
    </button>
    <button class="tab" class:active={activeTab === 'logs'} on:click={() => activeTab = 'logs'}>
      Logs
    </button>
  </div>

  {#if loading}
    <div class="loading">Loading system data...</div>
  {:else}

    <!-- ================================================================== -->
    <!-- TAB: System Information                                             -->
    <!-- ================================================================== -->
    {#if activeTab === 'system'}
      <div class="settings-grid">
        <!-- System Info Card -->
        <div class="card">
          <h2>System Information</h2>
          {#if systemInfo}
            <div class="info-rows">
              <div class="info-row">
                <span class="info-key">Hostname</span>
                <span class="info-val">{systemInfo.hostname || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">OS Version</span>
                <span class="info-val">{systemInfo.os_version || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">Kernel Version</span>
                <span class="info-val">{systemInfo.kernel_version || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">Architecture</span>
                <span class="info-val">{systemInfo.architecture || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">Uptime</span>
                <span class="info-val uptime-val">{systemInfo.uptime || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">CPU Cores</span>
                <span class="info-val">{systemInfo.cpu_count || 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">Memory</span>
                <span class="info-val">{systemInfo.memory_total ? formatBytes(systemInfo.memory_total) : 'N/A'}</span>
              </div>
              <div class="info-row">
                <span class="info-key">Disk</span>
                <span class="info-val">{systemInfo.disk_total ? formatBytes(systemInfo.disk_total) : 'N/A'}</span>
              </div>
            </div>
          {:else}
            <p class="no-data">Unable to retrieve system information</p>
          {/if}
        </div>

        <!-- Services Overview Card -->
        <div class="card">
          <h2>Services Overview</h2>
          {#if services.length > 0}
            <div class="svc-list">
              {#each services as svc}
                <div class="svc-row">
                  <div class="svc-info">
                    <span class="svc-dot" style="color: {stateColor(svc.state)}">{stateIcon(svc.state)}</span>
                    <span class="svc-name">{svc.display_name || svc.name}</span>
                  </div>
                  <span class="svc-badge" style="background: {stateColor(svc.state)}15; color: {stateColor(svc.state)}; border: 1px solid {stateColor(svc.state)}40">
                    {svc.state}
                  </span>
                </div>
              {/each}
            </div>
            <div class="card-footer">
              <a href="/services">Manage services...</a>
            </div>
          {:else}
            <p class="no-data">No service data available</p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Network Settings                                               -->
    <!-- ================================================================== -->
    {#if activeTab === 'network'}
      <div class="settings-grid">
        <!-- WAN Configuration -->
        <div class="card">
          <h2>WAN Interface</h2>
          {#if networkLoading}
            <p class="no-data">Loading network configuration...</p>
          {:else}
            <div class="form-group">
              <label for="wan-iface">Interface Name</label>
              <input type="text" id="wan-iface" bind:value={networkConfig.wan_interface} placeholder="e.g. eth0" />
            </div>
            <div class="form-group">
              <label class="checkbox-label">
                <input type="checkbox" bind:checked={networkConfig.wan_dhcp} />
                <span>Use DHCP (Automatic IP)</span>
              </label>
            </div>
            {#if !networkConfig.wan_dhcp}
              <div class="form-group">
                <label for="wan-ip">IP Address</label>
                <input type="text" id="wan-ip" bind:value={networkConfig.wan_ip} placeholder="e.g. 10.0.0.100" />
              </div>
              <div class="form-group">
                <label for="wan-mask">Netmask</label>
                <input type="text" id="wan-mask" bind:value={networkConfig.wan_netmask} placeholder="e.g. 255.255.255.0" />
              </div>
              <div class="form-group">
                <label for="wan-gw">Gateway</label>
                <input type="text" id="wan-gw" bind:value={networkConfig.wan_gateway} placeholder="e.g. 10.0.0.1" />
              </div>
            {/if}
            <div class="form-actions">
              <button class="btn-save" on:click={saveNetworkConfig} disabled={networkSaving}>
                {networkSaving ? 'Saving...' : 'Save WAN Config'}
              </button>
            </div>
          {/if}
        </div>

        <!-- LAN Configuration -->
        <div class="card">
          <h2>LAN Interface</h2>
          <div class="form-group">
            <label for="lan-iface">Interface Name</label>
            <input type="text" id="lan-iface" bind:value={networkConfig.lan_interface} placeholder="e.g. eth1" />
          </div>
          <div class="form-group">
            <label for="lan-ip">LAN IP Address</label>
            <input type="text" id="lan-ip" bind:value={networkConfig.lan_ip} placeholder="e.g. 192.168.1.1" />
          </div>
          <div class="form-group">
            <label for="lan-mask">LAN Netmask</label>
            <input type="text" id="lan-mask" bind:value={networkConfig.lan_netmask} placeholder="e.g. 255.255.255.0" />
          </div>
          <div class="form-group">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={networkConfig.lan_dhcp_enabled} />
              <span>Enable DHCP Server on LAN</span>
            </label>
          </div>
          <div class="form-actions">
            <button class="btn-save" on:click={saveNetworkConfig} disabled={networkSaving}>
              {networkSaving ? 'Saving...' : 'Save LAN Config'}
            </button>
          </div>
        </div>

        <!-- DNS Configuration -->
        <div class="card full-width">
          <h2>DNS Settings</h2>
          <div class="form-group">
            <label for="dns-listen">Listen Address</label>
            <input type="text" id="dns-listen" bind:value={dnsConfig.listen_address} placeholder="e.g. 127.0.0.1" />
          </div>
          <div class="form-group">
            <label for="dns-cache">Cache Size</label>
            <input type="number" id="dns-cache" bind:value={dnsConfig.cache_size} min="100" max="10000" step="100" />
          </div>
          <div class="form-group">
            <label>Upstream DNS Servers</label>
            <div class="upstream-list">
              {#each dnsConfig.upstream as addr}
                <div class="upstream-item">
                  <span class="upstream-addr">{addr}</span>
                  <button class="btn-remove" on:click={() => removeUpstream(addr)} title="Remove">x</button>
                </div>
              {/each}
            </div>
            <div class="upstream-add">
              <input type="text" bind:value={newUpstream} placeholder="8.8.4.4" on:keydown={(e) => e.key === 'Enter' && addUpstream()} />
              <button class="btn-add" on:click={addUpstream}>Add</button>
            </div>
          </div>
          <div class="form-actions">
            <button class="btn-save" on:click={saveDnsConfig} disabled={dnsSaving}>
              {dnsSaving ? 'Saving...' : 'Save DNS Config'}
            </button>
          </div>
        </div>
      </div>
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Service Management                                             -->
    <!-- ================================================================== -->
    {#if activeTab === 'services'}
      <div class="svc-controls">
        <button class="btn-refresh" on:click={loadServices} disabled={loading}>
          {loading ? 'Loading...' : 'Refresh'}
        </button>
      </div>

      {#if services.length === 0}
        <div class="card">
          <p class="no-data">No services found</p>
        </div>
      {:else}
        <div class="svc-grid">
          {#each services as svc}
            <div class="card svc-card">
              <div class="svc-card-header">
                <div class="svc-card-title">
                  <span class="svc-dot lg" style="color: {stateColor(svc.state)}">{stateIcon(svc.state)}</span>
                  <div>
                    <h3>{svc.display_name || svc.name}</h3>
                    <span class="svc-name-small">{svc.name}</span>
                  </div>
                </div>
                <span class="svc-badge" style="background: {stateColor(svc.state)}15; color: {stateColor(svc.state)}; border: 1px solid {stateColor(svc.state)}40">
                  {svc.state}
                </span>
              </div>
              <div class="svc-actions">
                {#if svc.state === 'stopped' || svc.state === 'failed'}
                  <button
                    class="btn-action btn-start"
                    on:click={() => serviceAction(svc.name, 'start')}
                    disabled={actionInProgress !== ''}
                  >
                    {actionInProgress === `${svc.name}-start` ? 'Starting...' : 'Start'}
                  </button>
                {:else if svc.state === 'running'}
                  <button
                    class="btn-action btn-stop"
                    on:click={() => serviceAction(svc.name, 'stop')}
                    disabled={actionInProgress !== ''}
                  >
                    {actionInProgress === `${svc.name}-stop` ? 'Stopping...' : 'Stop'}
                  </button>
                  <button
                    class="btn-action btn-restart"
                    on:click={() => serviceAction(svc.name, 'restart')}
                    disabled={actionInProgress !== ''}
                  >
                    {actionInProgress === `${svc.name}-restart` ? 'Restarting...' : 'Restart'}
                  </button>
                {:else}
                  <span class="transitioning-text">{svc.state}...</span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: System Actions                                                 -->
    <!-- ================================================================== -->
    {#if activeTab === 'actions'}
      <div class="settings-grid">
        <!-- Reboot -->
        <div class="card">
          <h2>Reboot System</h2>
          <p class="card-desc">Restart the router. All active connections will be dropped.</p>
          <div class="form-actions">
            <button
              class="btn-action btn-restart"
              on:click={rebootSystem}
              disabled={actionInProgress !== ''}
            >
              {actionInProgress === 'reboot' ? 'Rebooting...' : 'Reboot System'}
            </button>
          </div>
        </div>

        <!-- Shutdown -->
        <div class="card">
          <h2>Shutdown System</h2>
          <p class="card-desc">Power off the router. You will need to physically power it back on.</p>
          <div class="form-actions">
            <button
              class="btn-action btn-stop"
              on:click={shutdownSystem}
              disabled={actionInProgress !== ''}
            >
              {actionInProgress === 'shutdown' ? 'Shutting down...' : 'Shutdown System'}
            </button>
          </div>
        </div>

        <!-- Factory Reset -->
        <div class="card full-width danger-zone">
          <h2>Factory Reset</h2>
          <p class="card-desc">
            Erase all configuration and restore factory defaults. This action cannot be undone.
          </p>
          <div class="form-group">
            <label for="reset-confirm">Type <strong>RESET</strong> to confirm:</label>
            <input type="text" id="reset-confirm" bind:value={factoryResetConfirm} placeholder="RESET" />
          </div>
          <div class="form-actions">
            <button
              class="btn-action btn-danger"
              on:click={factoryReset}
              disabled={factoryResetLoading || factoryResetConfirm !== 'RESET'}
            >
              {factoryResetLoading ? 'Resetting...' : 'Factory Reset'}
            </button>
          </div>
        </div>

        <!-- Backup -->
        <div class="card">
          <h2>Backup Configuration</h2>
          <p class="card-desc">Download the current router configuration as a JSON file.</p>
          <div class="form-group">
            <label for="backup-desc">Description (optional)</label>
            <input type="text" id="backup-desc" bind:value={backupDescription} placeholder="e.g. Before upgrade" />
          </div>
          <div class="form-actions">
            <button class="btn-action btn-primary" on:click={backupConfig}>Download Backup</button>
          </div>
          {#if backupResult}
            <div class="result-msg ok">Backup downloaded successfully</div>
          {/if}
        </div>

        <!-- Restore -->
        <div class="card">
          <h2>Restore Configuration</h2>
          <p class="card-desc">Import a previously saved backup file to restore configuration.</p>
          <div class="form-group">
            <label for="restore-file">Select Backup File</label>
            <input type="file" id="restore-file" accept=".json" on:change={handleRestoreFile} />
          </div>
          <div class="form-group">
            <label for="restore-json">Or Paste JSON</label>
            <textarea id="restore-json" bind:value={restoreJson} rows="6" placeholder='{"version": "1.0", ...}' class="json-textarea"></textarea>
          </div>
          <div class="form-actions">
            <button
              class="btn-action btn-primary"
              on:click={restoreConfig}
              disabled={restoreLoading || !restoreJson.trim()}
            >
              {restoreLoading ? 'Restoring...' : 'Restore Configuration'}
            </button>
          </div>
          {#if restoreResult}
            <div class="result-msg ok">Configuration restored successfully</div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Logs                                                           -->
    <!-- ================================================================== -->
    {#if activeTab === 'logs'}
      <div class="card log-controls">
        <h2>Log Filters</h2>
        <div class="filter-row">
          <div class="form-group">
            <label for="log-source">Sources</label>
            <input type="text" id="log-source" bind:value={logSource} placeholder="vpp,dnsmasq,vectoros" />
          </div>
          <div class="form-group">
            <label for="log-level">Min Level</label>
            <select id="log-level" bind:value={logLevel}>
              <option value="debug">Debug</option>
              <option value="info">Info</option>
              <option value="warn">Warn</option>
              <option value="error">Error</option>
            </select>
          </div>
          <div class="form-group">
            <label for="log-lines">Lines</label>
            <input type="number" id="log-lines" bind:value={logLines} min="100" max="5000" step="100" />
          </div>
          <div class="form-group">
            <label for="log-keyword">Keyword</label>
            <input type="text" id="log-keyword" bind:value={logKeyword} placeholder="Filter keyword..." on:keydown={(e) => e.key === 'Enter' && fetchLogs()} />
          </div>
          <div class="form-group">
            <label for="log-limit">Max Results</label>
            <input type="number" id="log-limit" bind:value={logLimit} min="10" max="500" step="10" />
          </div>
          <div class="form-group form-actions-inline">
            <button class="btn-save" on:click={fetchLogs} disabled={logsLoading}>
              {logsLoading ? 'Loading...' : 'Fetch Logs'}
            </button>
          </div>
        </div>
      </div>

      <!-- Log display -->
      <div class="card log-display">
        <div class="log-header-row">
          <h2>Logs ({logEntries.length} entries)</h2>
        </div>
        {#if logsLoading}
          <p class="no-data">Loading logs...</p>
        {:else if logEntries.length === 0}
          <p class="no-data">No log entries found. Click Fetch Logs to retrieve.</p>
        {:else}
          <div class="log-table">
            <div class="log-table-header">
              <span class="col-ts">Timestamp</span>
              <span class="col-level">Level</span>
              <span class="col-source">Source</span>
              <span class="col-msg">Message</span>
            </div>
            {#each logEntries as log}
              <div class="log-table-row">
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
    {/if}

  {/if}
</div>

<style>
  .settings-page { max-width: 1200px; }

  h1 {
    color: #00ff88;
    margin: 0 0 1.5rem 0;
    font-size: 1.6rem;
  }

  h2 {
    color: #e0e0e0;
    margin: 0 0 1rem 0;
    font-size: 1.1rem;
  }

  h3 {
    color: #e0e0e0;
    margin: 0;
    font-size: 1rem;
  }

  /* Tabs */
  .tab-bar {
    display: flex;
    gap: 0;
    border-bottom: 2px solid #333;
    margin-bottom: 1.5rem;
  }
  .tab {
    background: none;
    border: none;
    color: #888;
    padding: 0.7rem 1.2rem;
    font-size: 0.9rem;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    transition: all 0.15s;
    border-radius: 0;
  }
  .tab:hover { color: #e0e0e0; background: #16213e; }
  .tab.active { color: #00ff88; border-bottom-color: #00ff88; font-weight: 600; }

  /* Loading */
  .loading { color: #888; text-align: center; padding: 3rem; }
  .no-data { color: #666; text-align: center; padding: 1.5rem; }

  /* Messages */
  .msg-error {
    background: #2e1a1a; border: 1px solid #ff4444; color: #ff4444;
    padding: 0.75rem 1rem; border-radius: 0.5rem; margin-bottom: 1rem;
    display: flex; justify-content: space-between; align-items: center;
  }
  .msg-success {
    background: #1a2e1a; border: 1px solid #00ff88; color: #00ff88;
    padding: 0.75rem 1rem; border-radius: 0.5rem; margin-bottom: 1rem;
    display: flex; justify-content: space-between; align-items: center;
  }
  .msg-close { background: none; border: none; color: inherit; cursor: pointer; font-size: 1rem; padding: 0 0.3rem; }

  /* Cards */
  .card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    border: 1px solid #333;
  }

  .card-footer {
    margin-top: 1rem;
    padding-top: 0.75rem;
    border-top: 1px solid #333;
  }
  .card-footer a {
    color: #00ff88;
    font-size: 0.85rem;
  }

  .card-desc {
    color: #888;
    font-size: 0.9rem;
    margin: 0 0 1rem 0;
    line-height: 1.5;
  }

  .full-width { grid-column: 1 / -1; }

  .danger-zone {
    border-color: #ff444440;
    background: #1a1a2e;
  }
  .danger-zone h2 { color: #ff4444; }

  /* Grid layouts */
  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
    gap: 1rem;
  }

  /* Info rows */
  .info-rows { display: flex; flex-direction: column; }
  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.6rem 0;
    border-bottom: 1px solid #222;
  }
  .info-row:last-child { border-bottom: none; }
  .info-key { font-size: 0.85rem; color: #888; }
  .info-val { font-size: 0.9rem; color: #e0e0e0; font-weight: 500; }
  .uptime-val { color: #00ff88; }

  /* Forms */
  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-bottom: 0.75rem;
  }
  .form-group label {
    font-size: 0.8rem;
    color: #888;
  }
  .form-group input,
  .form-group select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.5rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.9rem;
  }
  .form-group input:focus,
  .form-group select:focus {
    outline: none;
    border-color: #00ff88;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    font-size: 0.9rem;
    color: #ccc;
  }
  .checkbox-label:hover { color: #e0e0e0; }
  .checkbox-label input[type="checkbox"] {
    accent-color: #00ff88;
    width: 1rem;
    height: 1rem;
  }

  .form-actions { margin-top: 0.5rem; }
  .form-actions-inline { justify-content: flex-end; display: flex; align-items: flex-end; }

  /* Buttons */
  .btn-save {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .btn-save:hover:not(:disabled) { opacity: 0.9; }
  .btn-save:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-refresh {
    background: #16213e;
    border: 1px solid #333;
    color: #aaa;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .btn-refresh:hover:not(:disabled) { border-color: #555; color: #fff; }

  .btn-action {
    padding: 0.4rem 0.8rem;
    border: 1px solid transparent;
    border-radius: 0.35rem;
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: all 0.15s;
    color: #fff;
    background: #333;
  }
  .btn-action:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-action:hover:not(:disabled) { filter: brightness(1.2); }

  .btn-start { background: #00aa55; }
  .btn-stop { background: #aa3333; }
  .btn-restart { background: #aa7700; }
  .btn-danger { background: #ff4444; color: #fff; font-weight: 600; }
  .btn-primary { background: #00ff88; color: #0f0f23; font-weight: 600; }

  /* Upstream DNS list */
  .upstream-list {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 0.5rem;
  }
  .upstream-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.4rem 0.75rem;
    background: #0f0f23;
    border: 1px solid #333;
    border-radius: 0.4rem;
  }
  .upstream-addr {
    font-family: monospace;
    color: #e0e0e0;
    font-size: 0.9rem;
  }
  .btn-remove {
    background: none;
    border: 1px solid #555;
    color: #888;
    width: 1.4rem;
    height: 1.4rem;
    border-radius: 0.2rem;
    cursor: pointer;
    font-size: 0.7rem;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .btn-remove:hover { border-color: #ff4444; color: #ff4444; }

  .upstream-add {
    display: flex;
    gap: 0.5rem;
  }
  .upstream-add input {
    flex: 1;
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.5rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.9rem;
  }
  .upstream-add input:focus { outline: none; border-color: #00ff88; }
  .btn-add {
    background: #333;
    border: 1px solid #555;
    color: #e0e0e0;
    padding: 0.5rem 0.75rem;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .btn-add:hover { border-color: #00ff88; color: #00ff88; }

  /* Services grid */
  .svc-controls {
    display: flex;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }
  .svc-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 1rem;
  }
  .svc-card { padding: 1.25rem; }
  .svc-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.75rem;
  }
  .svc-card-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .svc-card-title h3 { margin: 0; font-size: 1.05rem; }
  .svc-name-small {
    font-size: 0.7rem;
    color: #666;
    display: block;
    margin-top: 0.1rem;
  }
  .svc-dot { font-size: 1.1rem; }
  .svc-dot.lg { font-size: 1.3rem; }
  .svc-badge {
    font-size: 0.7rem;
    padding: 0.2rem 0.6rem;
    border-radius: 1rem;
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.05em;
  }
  .svc-actions { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .svc-list { display: flex; flex-direction: column; gap: 0.4rem; }
  .svc-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 0;
    border-bottom: 1px solid #222;
  }
  .svc-row:last-child { border-bottom: none; }
  .svc-info { display: flex; align-items: center; gap: 0.5rem; }
  .svc-name { color: #e0e0e0; font-size: 0.9rem; }
  .transitioning-text { color: #ffaa00; font-style: italic; font-size: 0.85rem; }

  /* Result messages */
  .result-msg {
    margin-top: 0.75rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.4rem;
    font-size: 0.85rem;
  }
  .result-msg.ok {
    background: #003322;
    color: #00ff88;
    border: 1px solid #00ff8840;
  }

  /* Textarea */
  .json-textarea {
    width: 100%;
    background: #0a0a1a;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.75rem;
    border-radius: 0.4rem;
    font-family: monospace;
    font-size: 0.85rem;
    resize: vertical;
    min-height: 120px;
  }
  .json-textarea:focus { outline: none; border-color: #00ff88; }

  /* Logs */
  .log-controls { margin-bottom: 1rem; }
  .filter-row {
    display: flex;
    gap: 1rem;
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .filter-row .form-group {
    min-width: 140px;
  }

  .log-display { margin-bottom: 1rem; }
  .log-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .log-table {
    font-family: 'Courier New', monospace;
    font-size: 0.8rem;
    overflow-x: auto;
  }
  .log-table-header,
  .log-table-row {
    display: grid;
    grid-template-columns: 170px 60px 100px 1fr;
    gap: 0.75rem;
    padding: 0.4rem 0;
    border-bottom: 1px solid #2a2a3e;
    align-items: start;
  }
  .log-table-header {
    font-weight: bold;
    color: #666;
    border-bottom: 1px solid #444;
    text-transform: uppercase;
    font-size: 0.7rem;
  }
  .log-table-row { color: #ccc; }
  .col-ts { color: #888; }
  .col-level { font-weight: bold; text-transform: uppercase; }
  .col-source { color: #00aaff; }
  .col-msg { word-break: break-word; }

  /* Responsive */
  @media (max-width: 900px) {
    .settings-grid { grid-template-columns: 1fr; }
    .svc-grid { grid-template-columns: 1fr; }
    .tab-bar { flex-wrap: wrap; }
    .filter-row { flex-direction: column; }
    .filter-row .form-group { min-width: 100%; }
  }
</style>
