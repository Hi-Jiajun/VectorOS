<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  // ---------------------------------------------------------------------------
  // Types
  // ---------------------------------------------------------------------------

  interface ServiceInfo {
    name: string;
    display_name: string;
    state: string;
    description: string;
    last_transition: string;
    error?: string;
  }

  interface ProcessMetric {
    name: string;
    running: boolean;
    pid: number | null;
    mem_rss: number;
    cpu_percent: number;
  }

  interface ServiceMetrics {
    pid: number | null;
    cpu_percent: number;
    mem_rss: number;
  }

  // Service metadata for each known service
  interface ServiceMeta {
    name: string;
    display_name: string;
    description: string;
    category: string;
    configSummary: string;
    processName: string;
    healthEndpoint?: string;
  }

  // ---------------------------------------------------------------------------
  // Known services metadata
  // ---------------------------------------------------------------------------

  const serviceMeta: Record<string, ServiceMeta> = {
    vpp: {
      name: 'vpp',
      display_name: 'VPP Data Plane',
      description: 'Vector Packet Processing - High-performance userspace packet forwarding engine based on DPDK',
      category: 'Core',
      configSummary: 'Socket: /run/vpp/api.sock | Plugin mode: dpdk',
      processName: 'vpp',
    },
    pppoe: {
      name: 'pppoe',
      display_name: 'PPPoE Client',
      description: 'PPP over Ethernet dial-up connection for WAN connectivity',
      category: 'WAN',
      configSummary: 'Protocol: PPPoE over Ethernet',
      processName: '',
    },
    dhcp: {
      name: 'dhcp',
      display_name: 'DHCP Server',
      description: 'DHCP address allocation for LAN clients via dnsmasq',
      category: 'LAN',
      configSummary: 'Range: 192.168.1.100-200 | Lease: 86400s',
      processName: 'dnsmasq',
    },
    dns: {
      name: 'dns',
      display_name: 'DNS Forwarder',
      description: 'DNS resolution and upstream forwarding via dnsmasq',
      category: 'LAN',
      configSummary: 'Upstream: 8.8.8.8, 1.1.1.1 | Cache: 1000 entries',
      processName: 'dnsmasq',
    },
    nat: {
      name: 'nat',
      display_name: 'NAT',
      description: 'Network Address Translation for outbound traffic',
      category: 'Core',
      configSummary: 'VPP NAT plugin | Inside/Outside interfaces configured',
      processName: '',
    },
    firewall: {
      name: 'firewall',
      display_name: 'Firewall',
      description: 'Stateful packet filtering and ACL management',
      category: 'Security',
      configSummary: 'Default policy: block | Rules: managed via Firewall page',
      processName: '',
    },
  };

  // Service categories for grouping
  const categories = ['All', 'Core', 'WAN', 'LAN', 'Security'];

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let services: ServiceInfo[] = [];
  let processMetrics: ProcessMetric[] = [];
  let loading = true;
  let error = '';
  let success = '';
  let actionInProgress: string | null = null;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // UI state
  let activeTab: 'overview' | 'details' | 'logs' | 'health' = 'overview';
  let categoryFilter = 'All';
  let selectedService: string | null = null;
  let searchText = '';

  // Service logs
  let logServiceName = '';
  let logLines = 200;
  let logKeyword = '';
  let serviceLogs: any[] = [];
  let logsLoading = false;

  // Auto-start configuration (stored locally per service)
  let autoStart: Record<string, boolean> = loadAutoStart();

  // Health check state
  let healthResults: Record<string, { status: string; latency: number; lastCheck: string }> = {};
  let healthChecking = false;

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  onMount(() => {
    fetchAll();
    refreshInterval = setInterval(fetchAll, 10000);
  });

  onDestroy(() => {
    if (refreshInterval) clearInterval(refreshInterval);
  });

  // ---------------------------------------------------------------------------
  // Data fetching
  // ---------------------------------------------------------------------------

  async function fetchAll() {
    try {
      if (services.length === 0) loading = true;
      error = '';

      const [servicesRes, monitorRes] = await Promise.all([
        fetch('/api/services').then(r => r.json()),
        fetch('/api/monitor/metrics').then(r => r.json()).catch(() => ({ status: 'error', metrics: null })),
      ]);

      if (servicesRes.status === 'ok') {
        services = servicesRes.services || [];
      } else {
        error = servicesRes.error || 'Failed to fetch services';
      }

      // Extract process metrics for resource usage display
      if (monitorRes.status === 'ok' && monitorRes.metrics?.processes) {
        processMetrics = monitorRes.metrics.processes;
      }
    } catch (e) {
      error = 'Failed to connect to server';
    } finally {
      loading = false;
    }
  }

  async function fetchServiceLogs(name: string) {
    logServiceName = name;
    activeTab = 'logs';
    logsLoading = true;
    serviceLogs = [];

    try {
      const res = await fetch('/api/logs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sources: name,
          level: 'debug',
          lines: logLines,
          filter: logKeyword || undefined,
          limit: 200,
        }),
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        serviceLogs = data.logs || [];
      }
    } catch (e) {
      error = `Failed to fetch logs for ${name}`;
    } finally {
      logsLoading = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Service actions
  // ---------------------------------------------------------------------------

  async function serviceAction(name: string, action: string) {
    try {
      actionInProgress = `${name}-${action}`;
      error = '';
      const res = await fetch(`/api/services/${name}/${action}`, { method: 'POST' });
      const data = await res.json();
      if (data.status === 'ok') {
        const idx = services.findIndex(s => s.name === name);
        if (idx >= 0) {
          services[idx] = data.service;
          services = [...services];
        }
        success = `${data.service.display_name} ${action} completed`;
        setTimeout(() => success = '', 3000);
      } else {
        error = data.error || `${action} failed`;
      }
    } catch (e) {
      error = `Failed to ${action} service`;
    } finally {
      actionInProgress = null;
    }
  }

  // ---------------------------------------------------------------------------
  // Auto-start management
  // ---------------------------------------------------------------------------

  function loadAutoStart(): Record<string, boolean> {
    try {
      const saved = localStorage.getItem('vectoros_autostart');
      return saved ? JSON.parse(saved) : {};
    } catch {
      return {};
    }
  }

  function saveAutoStart() {
    try {
      localStorage.setItem('vectoros_autostart', JSON.stringify(autoStart));
    } catch { /* ignore */ }
  }

  function toggleAutoStart(name: string) {
    autoStart[name] = !autoStart[name];
    autoStart = { ...autoStart };
    saveAutoStart();
    success = `Auto-start for ${serviceMeta[name]?.display_name || name} ${autoStart[name] ? 'enabled' : 'disabled'}`;
    setTimeout(() => success = '', 3000);
  }

  // ---------------------------------------------------------------------------
  // Health checks
  // ---------------------------------------------------------------------------

  async function runHealthChecks() {
    healthChecking = true;
    for (const svc of services) {
      const start = Date.now();
      try {
        const res = await fetch(`/api/services/${svc.name}/status`);
        const data = await res.json();
        const latency = Date.now() - start;
        healthResults[svc.name] = {
          status: data.status === 'ok' ? 'healthy' : 'unhealthy',
          latency,
          lastCheck: new Date().toISOString(),
        };
      } catch {
        healthResults[svc.name] = {
          status: 'unreachable',
          latency: Date.now() - start,
          lastCheck: new Date().toISOString(),
        };
      }
    }
    healthResults = { ...healthResults };
    healthChecking = false;
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

  function uptime(iso: string): string {
    try {
      const then = new Date(iso).getTime();
      const now = Date.now();
      const diff = Math.floor((now - then) / 1000);
      if (diff < 60) return `${diff}s ago`;
      if (diff < 3600) return `${Math.floor(diff / 60)}m ${diff % 60}s`;
      if (diff < 86400) return `${Math.floor(diff / 3600)}h ${Math.floor((diff % 3600) / 60)}m`;
      return `${Math.floor(diff / 86400)}d ${Math.floor((diff % 86400) / 3600)}h`;
    } catch {
      return iso;
    }
  }

  function formatTime(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  function getProcessInfo(serviceName: string): ServiceMetrics | null {
    const meta = serviceMeta[serviceName];
    if (!meta || !meta.processName) return null;

    // Try to match process by name
    const proc = processMetrics.find(p =>
      p.name.toLowerCase().includes(meta.processName.toLowerCase())
    );
    if (proc) {
      return {
        pid: proc.pid,
        cpu_percent: proc.cpu_percent,
        mem_rss: proc.mem_rss,
      };
    }
    return null;
  }

  function healthColor(status: string): string {
    switch (status) {
      case 'healthy': return '#00ff88';
      case 'unhealthy': return '#ff4444';
      case 'unreachable': return '#ff8800';
      default: return '#666';
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

  function clearMsg() { error = ''; success = ''; }

  // Filtered services
  $: filteredServices = services.filter(s => {
    if (categoryFilter !== 'All') {
      const meta = serviceMeta[s.name];
      if (!meta || meta.category !== categoryFilter) return false;
    }
    if (searchText) {
      const q = searchText.toLowerCase();
      return s.display_name.toLowerCase().includes(q) ||
             s.name.toLowerCase().includes(q) ||
             s.description.toLowerCase().includes(q);
    }
    return true;
  });

  $: selectedServiceInfo = selectedService ? services.find(s => s.name === selectedService) : null;
  $: selectedServiceMeta = selectedService ? serviceMeta[selectedService] : null;
</script>

<svelte:head>
  <title>VectorOS - Service Manager</title>
</svelte:head>

<div class="svc-page">
  <!-- Header -->
  <div class="svc-header">
    <div class="svc-title-row">
      <h1>Service Manager</h1>
      <div class="svc-header-right">
        <span class="svc-count">{services.filter(s => s.state === 'running').length}/{services.length} running</span>
        <button class="btn-refresh" on:click={fetchAll} disabled={loading}>
          {loading ? 'Loading...' : 'Refresh'}
        </button>
      </div>
    </div>
  </div>

  <!-- Messages -->
  {#if error}
    <div class="msg-error">{error} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}
  {#if success}
    <div class="msg-success">{success} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}

  <!-- Tab Navigation -->
  <div class="svc-tabs">
    {#each [
      { id: 'overview', label: 'Service Overview' },
      { id: 'details', label: 'Service Details' },
      { id: 'logs', label: 'Service Logs' },
      { id: 'health', label: 'Health & Monitoring' },
    ] as tab}
      <button
        class="svc-tab"
        class:svc-tab-active={activeTab === tab.id}
        on:click={() => { activeTab = tab.id; if (tab.id === 'health') runHealthChecks(); }}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  {#if loading && services.length === 0}
    <div class="svc-loading">Loading services...</div>
  {:else}

    <!-- ================================================================== -->
    <!-- TAB: Service Overview                                               -->
    <!-- ================================================================== -->
    {#if activeTab === 'overview'}
      <!-- Filter bar -->
      <div class="filter-bar">
        <div class="filter-categories">
          {#each categories as cat}
            <button
              class="filter-btn"
              class:filter-active={categoryFilter === cat}
              on:click={() => categoryFilter = cat}
            >
              {cat}
            </button>
          {/each}
        </div>
        <input
          type="text"
          class="search-input"
          bind:value={searchText}
          placeholder="Search services..."
        />
      </div>

      <!-- Summary cards -->
      <div class="summary-grid">
        <div class="summary-card">
          <span class="summary-value">{services.length}</span>
          <span class="summary-label">Total Services</span>
        </div>
        <div class="summary-card summary-running">
          <span class="summary-value">{services.filter(s => s.state === 'running').length}</span>
          <span class="summary-label">Running</span>
        </div>
        <div class="summary-card summary-stopped">
          <span class="summary-value">{services.filter(s => s.state === 'stopped').length}</span>
          <span class="summary-label">Stopped</span>
        </div>
        <div class="summary-card summary-failed">
          <span class="summary-value">{services.filter(s => s.state === 'failed').length}</span>
          <span class="summary-label">Failed</span>
        </div>
      </div>

      <!-- Service cards -->
      {#if filteredServices.length === 0}
        <div class="svc-empty">No services found matching the filter.</div>
      {:else}
        <div class="svc-grid">
          {#each filteredServices as svc}
            {@const meta = serviceMeta[svc.name]}
            {@const procInfo = getProcessInfo(svc.name)}
            <div
              class="svc-card"
              class:svc-card-selected={selectedService === svc.name}
              class:svc-card-running={svc.state === 'running'}
              class:svc-card-failed={svc.state === 'failed'}
              on:click={() => { selectedService = svc.name; activeTab = 'details'; }}
            >
              <!-- Card header -->
              <div class="svc-card-header">
                <div class="svc-card-title">
                  <span class="svc-state-dot" style="color: {stateColor(svc.state)}">{stateIcon(svc.state)}</span>
                  <div>
                    <h3>{svc.display_name}</h3>
                    <span class="svc-category">{meta?.category || 'Unknown'}</span>
                  </div>
                </div>
                <span class="svc-state-badge" style="background: {stateColor(svc.state)}15; color: {stateColor(svc.state)}; border: 1px solid {stateColor(svc.state)}40">
                  {svc.state}
                </span>
              </div>

              <!-- Description -->
              <p class="svc-desc">{svc.description}</p>

              <!-- Error -->
              {#if svc.error}
                <div class="svc-error">
                  <strong>Error:</strong> {svc.error}
                </div>
              {/if}

              <!-- Metrics row -->
              <div class="svc-metrics-row">
                <div class="svc-metric">
                  <span class="svc-metric-label">Uptime</span>
                  <span class="svc-metric-value">{uptime(svc.last_transition)}</span>
                </div>
                {#if procInfo}
                  <div class="svc-metric">
                    <span class="svc-metric-label">PID</span>
                    <span class="svc-metric-value">{procInfo.pid ?? '-'}</span>
                  </div>
                  <div class="svc-metric">
                    <span class="svc-metric-label">CPU</span>
                    <span class="svc-metric-value" style="color: {procInfo.cpu_percent > 80 ? '#ff4444' : procInfo.cpu_percent > 50 ? '#ffaa00' : '#00ff88'}">
                      {procInfo.cpu_percent.toFixed(1)}%
                    </span>
                  </div>
                  <div class="svc-metric">
                    <span class="svc-metric-label">Memory</span>
                    <span class="svc-metric-value">{formatBytes(procInfo.mem_rss)}</span>
                  </div>
                {/if}
                {#if autoStart[svc.name] !== undefined}
                  <div class="svc-metric">
                    <span class="svc-metric-label">Auto-start</span>
                    <span class="svc-metric-value" style="color: {autoStart[svc.name] ? '#00ff88' : '#666'}">
                      {autoStart[svc.name] ? 'On' : 'Off'}
                    </span>
                  </div>
                {/if}
              </div>

              <!-- Config summary -->
              {#if meta?.configSummary}
                <div class="svc-config-summary">
                  <span class="svc-config-text">{meta.configSummary}</span>
                </div>
              {/if}

              <!-- Actions -->
              <div class="svc-card-actions">
                {#if svc.state === 'stopped' || svc.state === 'failed'}
                  <button
                    class="btn-action btn-start"
                    on:click|stopPropagation={() => serviceAction(svc.name, 'start')}
                    disabled={actionInProgress !== null}
                  >
                    {actionInProgress === `${svc.name}-start` ? 'Starting...' : 'Start'}
                  </button>
                {:else if svc.state === 'running'}
                  <button
                    class="btn-action btn-stop"
                    on:click|stopPropagation={() => serviceAction(svc.name, 'stop')}
                    disabled={actionInProgress !== null}
                  >
                    {actionInProgress === `${svc.name}-stop` ? 'Stopping...' : 'Stop'}
                  </button>
                  <button
                    class="btn-action btn-restart"
                    on:click|stopPropagation={() => serviceAction(svc.name, 'restart')}
                    disabled={actionInProgress !== null}
                  >
                    {actionInProgress === `${svc.name}-restart` ? 'Restarting...' : 'Restart'}
                  </button>
                  <button
                    class="btn-action btn-reload"
                    on:click|stopPropagation={() => serviceAction(svc.name, 'reload')}
                    disabled={actionInProgress !== null}
                  >
                    {actionInProgress === `${svc.name}-reload` ? 'Reloading...' : 'Reload'}
                  </button>
                {:else}
                  <span class="svc-transitioning">{svc.state}...</span>
                {/if}
                <button
                  class="btn-action btn-logs"
                  on:click|stopPropagation={() => fetchServiceLogs(svc.name)}
                  title="View logs"
                >
                  Logs
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Service Details                                                -->
    <!-- ================================================================== -->
    {#if activeTab === 'details'}
      {#if !selectedService}
        <div class="svc-empty">
          <p>Select a service from the Overview tab to view its details.</p>
        </div>
      {:else if selectedServiceInfo && selectedServiceMeta}
        {@const svc = selectedServiceInfo}
        {@const meta = selectedServiceMeta}
        {@const procInfo = getProcessInfo(svc.name)}

        <div class="detail-header">
          <button class="btn-back" on:click={() => activeTab = 'overview'}>Back to Overview</button>
          <h2>{svc.display_name}</h2>
          <span class="svc-state-badge lg" style="background: {stateColor(svc.state)}15; color: {stateColor(svc.state)}; border: 1px solid {stateColor(svc.state)}40">
            {stateIcon(svc.state)} {svc.state}
          </span>
        </div>

        <div class="detail-grid">
          <!-- General info -->
          <div class="detail-card">
            <h3>General Information</h3>
            <div class="detail-rows">
              <div class="detail-row">
                <span class="detail-key">Name</span>
                <span class="detail-val">{svc.name}</span>
              </div>
              <div class="detail-row">
                <span class="detail-key">Display Name</span>
                <span class="detail-val">{svc.display_name}</span>
              </div>
              <div class="detail-row">
                <span class="detail-key">Description</span>
                <span class="detail-val">{svc.description}</span>
              </div>
              <div class="detail-row">
                <span class="detail-key">Category</span>
                <span class="detail-val">{meta.category}</span>
              </div>
              <div class="detail-row">
                <span class="detail-key">Current State</span>
                <span class="detail-val" style="color: {stateColor(svc.state)}">{svc.state}</span>
              </div>
              <div class="detail-row">
                <span class="detail-key">Last Transition</span>
                <span class="detail-val">{formatTime(svc.last_transition)}</span>
              </div>
              {#if svc.error}
                <div class="detail-row">
                  <span class="detail-key">Error</span>
                  <span class="detail-val error-text">{svc.error}</span>
                </div>
              {/if}
            </div>
          </div>

          <!-- Resource usage -->
          <div class="detail-card">
            <h3>Resource Usage</h3>
            {#if procInfo}
              <div class="detail-rows">
                <div class="detail-row">
                  <span class="detail-key">PID</span>
                  <span class="detail-val">{procInfo.pid ?? 'N/A'}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-key">CPU Usage</span>
                  <span class="detail-val">{procInfo.cpu_percent.toFixed(2)}%</span>
                </div>
                <div class="detail-row">
                  <span class="detail-key">Memory (RSS)</span>
                  <span class="detail-val">{formatBytes(procInfo.mem_rss)}</span>
                </div>
              </div>
              <!-- CPU bar -->
              <div class="resource-bar-section">
                <span class="resource-bar-label">CPU</span>
                <div class="resource-bar">
                  <div class="resource-bar-fill" style="width: {Math.min(procInfo.cpu_percent, 100)}%; background: {procInfo.cpu_percent > 80 ? '#ff4444' : procInfo.cpu_percent > 50 ? '#ffaa00' : '#00ff88'}"></div>
                </div>
              </div>
            {:else}
              <div class="svc-empty small">No process metrics available for this service.</div>
            {/if}
          </div>

          <!-- Configuration -->
          <div class="detail-card">
            <h3>Configuration Summary</h3>
            <p class="config-text">{meta.configSummary}</p>
            <div class="detail-rows" style="margin-top: 1rem;">
              <div class="detail-row">
                <span class="detail-key">Auto-start</span>
                <button
                  class="toggle-switch"
                  class:toggle-on={autoStart[svc.name]}
                  on:click={() => toggleAutoStart(svc.name)}
                >
                  {autoStart[svc.name] ? 'Enabled' : 'Disabled'}
                </button>
              </div>
            </div>
          </div>

          <!-- Actions -->
          <div class="detail-card">
            <h3>Service Actions</h3>
            <div class="action-buttons">
              {#if svc.state === 'stopped' || svc.state === 'failed'}
                <button
                  class="btn-action lg btn-start"
                  on:click={() => serviceAction(svc.name, 'start')}
                  disabled={actionInProgress !== null}
                >
                  {actionInProgress === `${svc.name}-start` ? 'Starting...' : 'Start Service'}
                </button>
              {:else if svc.state === 'running'}
                <button
                  class="btn-action lg btn-stop"
                  on:click={() => serviceAction(svc.name, 'stop')}
                  disabled={actionInProgress !== null}
                >
                  {actionInProgress === `${svc.name}-stop` ? 'Stopping...' : 'Stop Service'}
                </button>
                <button
                  class="btn-action lg btn-restart"
                  on:click={() => serviceAction(svc.name, 'restart')}
                  disabled={actionInProgress !== null}
                >
                  {actionInProgress === `${svc.name}-restart` ? 'Restarting...' : 'Restart Service'}
                </button>
                <button
                  class="btn-action lg btn-reload"
                  on:click={() => serviceAction(svc.name, 'reload')}
                  disabled={actionInProgress !== null}
                >
                  {actionInProgress === `${svc.name}-reload` ? 'Reloading...' : 'Reload Config'}
                </button>
              {:else}
                <span class="svc-transitioning">{svc.state}...</span>
              {/if}
              <button
                class="btn-action lg btn-logs"
                on:click={() => fetchServiceLogs(svc.name)}
              >
                View Logs
              </button>
            </div>
          </div>
        </div>
      {/if}
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Service Logs                                                   -->
    <!-- ================================================================== -->
    {#if activeTab === 'logs'}
      <div class="logs-controls">
        <div class="filter-row">
          <div class="form-group">
            <label>Service</label>
            <select bind:value={logServiceName} on:change={() => { if (logServiceName) fetchServiceLogs(logServiceName); }}>
              <option value="">Select a service...</option>
              {#each services as svc}
                <option value={svc.name}>{svc.display_name}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Lines</label>
            <input type="number" bind:value={logLines} min="50" max="1000" step="50" />
          </div>
          <div class="form-group">
            <label>Keyword Filter</label>
            <input type="text" bind:value={logKeyword} placeholder="Filter logs..." />
          </div>
          <div class="form-group form-group-btn">
            <button class="btn-action btn-primary" on:click={() => { if (logServiceName) fetchServiceLogs(logServiceName); }} disabled={!logServiceName || logsLoading}>
              {logsLoading ? 'Loading...' : 'Fetch Logs'}
            </button>
          </div>
        </div>
      </div>

      <div class="logs-display">
        <div class="logs-header">
          <h3>Logs for {logServiceName ? services.find(s => s.name === logServiceName)?.display_name || logServiceName : 'No Service Selected'}</h3>
          <span class="logs-count">{serviceLogs.length} entries</span>
        </div>

        {#if logsLoading}
          <div class="svc-loading">Loading logs...</div>
        {:else if serviceLogs.length === 0}
          <div class="svc-empty small">No log entries. Select a service and click Fetch Logs.</div>
        {:else}
          <div class="log-table">
            <div class="log-header">
              <span class="col-ts">Timestamp</span>
              <span class="col-level">Level</span>
              <span class="col-source">Source</span>
              <span class="col-msg">Message</span>
            </div>
            {#each serviceLogs as log}
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
    {/if}

    <!-- ================================================================== -->
    <!-- TAB: Health & Monitoring                                            -->
    <!-- ================================================================== -->
    {#if activeTab === 'health'}
      <div class="health-controls">
        <button class="btn-action btn-primary" on:click={runHealthChecks} disabled={healthChecking}>
          {healthChecking ? 'Running checks...' : 'Run Health Checks'}
        </button>
        <button class="btn-action btn-refresh" on:click={fetchAll}>Refresh Metrics</button>
      </div>

      <!-- Health results -->
      <div class="health-grid">
        {#each services as svc}
          {@const meta = serviceMeta[svc.name]}
          {@const health = healthResults[svc.name]}
          <div class="health-card" style="border-left: 3px solid {health ? healthColor(health.status) : '#333'}">
            <div class="health-card-header">
              <div>
                <h4>{svc.display_name}</h4>
                <span class="health-category">{meta?.category || 'Unknown'}</span>
              </div>
              {#if health}
                <span class="health-badge" style="background: {healthColor(health.status)}20; color: {healthColor(health.status)}">
                  {health.status}
                </span>
              {:else}
                <span class="health-badge" style="background: #33333320; color: #666">Not checked</span>
              {/if}
            </div>

            <div class="health-details">
              <div class="health-row">
                <span class="health-key">State</span>
                <span class="health-val" style="color: {stateColor(svc.state)}">{svc.state}</span>
              </div>
              {#if health}
                <div class="health-row">
                  <span class="health-key">API Latency</span>
                  <span class="health-val">{health.latency}ms</span>
                </div>
                <div class="health-row">
                  <span class="health-key">Last Check</span>
                  <span class="health-val">{formatTime(health.lastCheck)}</span>
                </div>
              {/if}
              <div class="health-row">
                <span class="health-key">Auto-start</span>
                <span class="health-val" style="color: {autoStart[svc.name] ? '#00ff88' : '#666'}">
                  {autoStart[svc.name] ? 'Enabled' : 'Disabled'}
                </span>
              </div>
              {#if svc.error}
                <div class="health-error">
                  {svc.error}
                </div>
              {/if}
            </div>

            <div class="health-actions">
              {#if svc.state === 'failed'}
                <button
                  class="btn-action btn-restart sm"
                  on:click={() => serviceAction(svc.name, 'restart')}
                  disabled={actionInProgress !== null}
                >
                  Auto-restart
                </button>
              {/if}
              <button
                class="btn-action btn-logs sm"
                on:click={() => fetchServiceLogs(svc.name)}
              >
                View Logs
              </button>
            </div>
          </div>
        {/each}
      </div>

      <!-- Monitoring notes -->
      <div class="monitor-notes">
        <h3>Monitoring Notes</h3>
        <ul>
          <li>Health checks verify the service API endpoint is reachable and responsive.</li>
          <li>Failed services can be restarted manually or will be auto-restarted if auto-start is enabled.</li>
          <li>Resource usage is displayed from system-wide process monitoring (see Monitor page for details).</li>
          <li>Service state is refreshed automatically every 10 seconds.</li>
        </ul>
      </div>
    {/if}

  {/if}
</div>

<style>
  .svc-page { max-width: 1400px; }

  /* Header */
  .svc-header { margin-bottom: 1.5rem; }
  .svc-title-row { display: flex; justify-content: space-between; align-items: center; }
  h1 { color: #00ff88; margin: 0; font-size: 1.6rem; }
  h2 { color: #e0e0e0; margin: 0; font-size: 1.3rem; }
  h3 { color: #e0e0e0; margin: 0 0 0.75rem 0; font-size: 1rem; }
  h4 { color: #e0e0e0; margin: 0; font-size: 0.95rem; }
  .svc-header-right { display: flex; align-items: center; gap: 1rem; }
  .svc-count { font-size: 0.9rem; color: #888; }

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

  /* Tabs */
  .svc-tabs {
    display: flex; gap: 0; margin-bottom: 1.5rem;
    border-bottom: 2px solid #333;
  }
  .svc-tab {
    background: none; border: none; color: #888; padding: 0.7rem 1.2rem;
    font-size: 0.9rem; cursor: pointer; border-bottom: 2px solid transparent;
    margin-bottom: -2px; transition: all 0.15s; border-radius: 0;
  }
  .svc-tab:hover { color: #e0e0e0; background: #16213e; }
  .svc-tab-active { color: #00ff88; border-bottom-color: #00ff88; font-weight: 600; }

  /* Loading / Empty */
  .svc-loading { color: #888; text-align: center; padding: 3rem; }
  .svc-empty { color: #666; text-align: center; padding: 2rem; }
  .svc-empty.small { padding: 1rem; font-size: 0.9rem; }

  /* Filter bar */
  .filter-bar {
    display: flex; justify-content: space-between; align-items: center;
    margin-bottom: 1.5rem; gap: 1rem;
  }
  .filter-categories { display: flex; gap: 0.5rem; }
  .filter-btn {
    background: #16213e; border: 1px solid #333; color: #888;
    padding: 0.4rem 0.8rem; border-radius: 0.4rem; cursor: pointer;
    font-size: 0.8rem; transition: all 0.15s;
  }
  .filter-btn:hover { border-color: #555; color: #ccc; }
  .filter-active { background: #003322; border-color: #00ff88; color: #00ff88; }
  .search-input {
    background: #0f0f23; color: #e0e0e0; border: 1px solid #333;
    padding: 0.5rem 0.75rem; border-radius: 0.4rem; font-size: 0.85rem;
    min-width: 200px;
  }
  .search-input:focus { outline: none; border-color: #00ff88; }

  /* Summary grid */
  .summary-grid {
    display: grid; grid-template-columns: repeat(4, 1fr); gap: 1rem; margin-bottom: 1.5rem;
  }
  .summary-card {
    background: #1a1a2e; padding: 1.25rem; border-radius: 0.75rem; text-align: center;
    border: 1px solid #333;
  }
  .summary-value { display: block; font-size: 2rem; font-weight: bold; color: #e0e0e0; }
  .summary-label { font-size: 0.8rem; color: #888; text-transform: uppercase; }
  .summary-running .summary-value { color: #00ff88; }
  .summary-stopped .summary-value { color: #888; }
  .summary-failed .summary-value { color: #ff4444; }

  /* Service grid */
  .svc-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(380px, 1fr)); gap: 1rem;
  }

  /* Service card */
  .svc-card {
    background: #1a1a2e; border: 1px solid #333; border-radius: 0.75rem;
    padding: 1.25rem; transition: all 0.2s; cursor: pointer;
  }
  .svc-card:hover { border-color: #555; background: #1e1e34; }
  .svc-card-selected { border-color: #00ff88; }
  .svc-card-running { border-left: 3px solid #00ff88; }
  .svc-card-failed { border-left: 3px solid #ff4444; }

  .svc-card-header {
    display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;
  }
  .svc-card-title { display: flex; align-items: center; gap: 0.6rem; }
  .svc-card-title h3 { margin: 0; font-size: 1.05rem; }
  .svc-category {
    font-size: 0.7rem; color: #666; text-transform: uppercase;
    letter-spacing: 0.05em; margin-top: 0.1rem; display: block;
  }
  .svc-state-dot { font-size: 1.1rem; }

  .svc-state-badge {
    font-size: 0.7rem; padding: 0.2rem 0.6rem; border-radius: 1rem;
    text-transform: uppercase; font-weight: 600; letter-spacing: 0.05em;
  }
  .svc-state-badge.lg { font-size: 0.8rem; padding: 0.3rem 0.8rem; }

  .svc-desc { color: #999; font-size: 0.85rem; margin: 0.25rem 0 0.75rem; line-height: 1.4; }

  .svc-error {
    background: #ff444415; border: 1px solid #ff444440; color: #ff8888;
    padding: 0.5rem 0.75rem; border-radius: 0.4rem; font-size: 0.8rem; margin-bottom: 0.75rem;
  }

  /* Metrics row */
  .svc-metrics-row {
    display: flex; flex-wrap: wrap; gap: 0.75rem; margin-bottom: 0.75rem;
  }
  .svc-metric { display: flex; flex-direction: column; gap: 0.1rem; }
  .svc-metric-label { font-size: 0.65rem; color: #666; text-transform: uppercase; letter-spacing: 0.05em; }
  .svc-metric-value { font-size: 0.85rem; color: #ccc; font-weight: 500; }

  /* Config summary */
  .svc-config-summary {
    background: #0f0f23; padding: 0.5rem 0.75rem; border-radius: 0.4rem;
    margin-bottom: 0.75rem;
  }
  .svc-config-text { font-size: 0.75rem; color: #888; font-family: monospace; }

  /* Card actions */
  .svc-card-actions { display: flex; gap: 0.4rem; flex-wrap: wrap; }

  .svc-transitioning { color: #ffaa00; font-style: italic; font-size: 0.85rem; }

  /* Buttons */
  .btn-action {
    padding: 0.35rem 0.7rem; border: 1px solid transparent; border-radius: 0.35rem;
    cursor: pointer; font-size: 0.8rem; font-weight: 500; transition: all 0.15s;
    color: #fff; background: #333;
  }
  .btn-action:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-action.sm { padding: 0.25rem 0.5rem; font-size: 0.75rem; }
  .btn-action.lg { padding: 0.5rem 1rem; font-size: 0.85rem; }
  .btn-action:hover:not(:disabled) { filter: brightness(1.2); }

  .btn-start { background: #00aa55; }
  .btn-stop { background: #aa3333; }
  .btn-restart { background: #aa7700; }
  .btn-reload { background: #3366aa; }
  .btn-logs { background: #444; border-color: #555; }
  .btn-logs:hover:not(:disabled) { border-color: #00ff88; color: #00ff88; }
  .btn-primary { background: #00ff88; color: #0f0f23; font-weight: 600; }
  .btn-refresh { background: #16213e; border: 1px solid #333; color: #aaa; }
  .btn-refresh:hover:not(:disabled) { border-color: #555; color: #fff; }
  .btn-back {
    background: none; border: 1px solid #444; color: #888; padding: 0.35rem 0.7rem;
    border-radius: 0.35rem; cursor: pointer; font-size: 0.8rem; margin-bottom: 1rem;
  }
  .btn-back:hover { border-color: #00ff88; color: #00ff88; }

  /* Detail page */
  .detail-header {
    display: flex; align-items: center; gap: 1rem; margin-bottom: 1.5rem;
    flex-wrap: wrap;
  }

  .detail-grid {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(350px, 1fr)); gap: 1rem;
  }

  .detail-card {
    background: #1a1a2e; border: 1px solid #333; border-radius: 0.75rem; padding: 1.25rem;
  }

  .detail-rows { display: flex; flex-direction: column; gap: 0.5rem; }
  .detail-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.4rem 0; border-bottom: 1px solid #222;
  }
  .detail-key { font-size: 0.8rem; color: #888; }
  .detail-val { font-size: 0.85rem; color: #e0e0e0; }
  .error-text { color: #ff4444; }

  .config-text { font-size: 0.85rem; color: #aaa; font-family: monospace; }

  .resource-bar-section { margin-top: 0.75rem; }
  .resource-bar-label { font-size: 0.75rem; color: #888; display: block; margin-bottom: 0.3rem; }
  .resource-bar { height: 6px; background: #333; border-radius: 3px; overflow: hidden; }
  .resource-bar-fill { height: 100%; border-radius: 3px; transition: width 0.5s ease; }

  .action-buttons { display: flex; gap: 0.5rem; flex-wrap: wrap; }

  .toggle-switch {
    padding: 0.3rem 0.7rem; border-radius: 0.3rem; font-size: 0.75rem;
    font-weight: 600; cursor: pointer; border: 1px solid #555;
    background: #333; color: #888; transition: all 0.15s;
  }
  .toggle-on { border-color: #00ff88; color: #00ff88; background: #003322; }

  /* Logs */
  .logs-controls { margin-bottom: 1rem; }
  .filter-row {
    display: flex; gap: 1rem; align-items: flex-end; flex-wrap: wrap;
    background: #1a1a2e; padding: 1rem 1.25rem; border-radius: 0.75rem;
  }
  .form-group { display: flex; flex-direction: column; gap: 0.3rem; }
  .form-group label { font-size: 0.75rem; color: #888; }
  .form-group-btn { justify-content: flex-end; }

  .filter-row input, .filter-row select {
    background: #0f0f23; color: #e0e0e0; border: 1px solid #333;
    padding: 0.5rem 0.75rem; border-radius: 0.4rem; font-size: 0.85rem;
  }
  .filter-row input:focus, .filter-row select:focus { outline: none; border-color: #00ff88; }

  .logs-display {
    background: #1a1a2e; border-radius: 0.75rem; padding: 1.25rem;
  }
  .logs-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }
  .logs-count { font-size: 0.8rem; color: #666; }

  .log-table { font-family: 'Courier New', monospace; font-size: 0.8rem; overflow-x: auto; }
  .log-header, .log-row {
    display: grid; grid-template-columns: 170px 60px 90px 1fr; gap: 0.75rem;
    padding: 0.4rem 0; border-bottom: 1px solid #2a2a3e; align-items: start;
  }
  .log-header { font-weight: bold; color: #666; border-bottom: 1px solid #444; text-transform: uppercase; font-size: 0.7rem; }
  .log-row { color: #ccc; }
  .col-ts { color: #888; }
  .col-level { font-weight: bold; text-transform: uppercase; }
  .col-source { color: #00aaff; }
  .col-msg { word-break: break-word; }

  /* Health */
  .health-controls { display: flex; gap: 0.75rem; margin-bottom: 1.5rem; }

  .health-grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 1rem;
    margin-bottom: 1.5rem;
  }
  .health-card {
    background: #1a1a2e; border: 1px solid #333; border-radius: 0.75rem; padding: 1.25rem;
  }
  .health-card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.75rem; }
  .health-category { font-size: 0.7rem; color: #666; text-transform: uppercase; }
  .health-badge {
    font-size: 0.7rem; padding: 0.2rem 0.6rem; border-radius: 1rem;
    text-transform: uppercase; font-weight: 600;
  }
  .health-details { margin-bottom: 0.75rem; }
  .health-row {
    display: flex; justify-content: space-between; padding: 0.3rem 0;
    border-bottom: 1px solid #222; font-size: 0.8rem;
  }
  .health-key { color: #888; }
  .health-val { color: #e0e0e0; }
  .health-error { color: #ff4444; font-size: 0.8rem; margin-top: 0.5rem; padding: 0.4rem; background: #ff444415; border-radius: 0.3rem; }
  .health-actions { display: flex; gap: 0.4rem; }

  .monitor-notes {
    background: #1a1a2e; border: 1px solid #333; border-radius: 0.75rem; padding: 1.25rem;
  }
  .monitor-notes h3 { margin-bottom: 0.5rem; }
  .monitor-notes ul { padding-left: 1.5rem; }
  .monitor-notes li { color: #888; font-size: 0.85rem; margin-bottom: 0.3rem; }

  /* Responsive */
  @media (max-width: 900px) {
    .summary-grid { grid-template-columns: repeat(2, 1fr); }
    .svc-grid { grid-template-columns: 1fr; }
    .detail-grid { grid-template-columns: 1fr; }
    .health-grid { grid-template-columns: 1fr; }
    .filter-bar { flex-direction: column; align-items: stretch; }
    .filter-categories { flex-wrap: wrap; }
  }
</style>
