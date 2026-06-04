<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  let trafficStatus: any = null;
  let trafficStats: any = null;
  let loading = true;
  let error = '';
  let success = '';
  let autoRefresh = true;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;

  // Interface limit form
  let ifaceForm = {
    interface: '',
    rate: '',
    burst: '150000',
    direction: 'both'
  };

  // IP limit form
  let ipForm = {
    ip: '',
    rate: '',
    burst: '75000'
  };

  // Priority form
  let priorityForm = {
    name: '',
    queue: 'medium'
  };

  // App class form
  let appClassForm = {
    name: '',
    ports: '',
    protocol: 'tcp',
    priority: 'medium',
    description: ''
  };

  // Traffic history for sparkline
  let trafficHistory: { time: string; interfaces: number; ips: number; apps: number }[] = [];
  const MAX_HISTORY = 30;

  const priorityLevels = [
    { value: 'high', label: 'High', color: '#00ff88', weight: 40 },
    { value: 'medium', label: 'Medium', color: '#ffd93d', weight: 35 },
    { value: 'low', label: 'Low', color: '#ff6b6b', weight: 25 },
  ];

  const directions = [
    { value: 'both', label: 'Both (In + Out)' },
    { value: 'input', label: 'Input Only' },
    { value: 'output', label: 'Output Only' },
  ];

  const protocols = [
    { value: 'tcp', label: 'TCP' },
    { value: 'udp', label: 'UDP' },
    { value: '', label: 'Any' },
  ];

  onMount(async () => {
    await fetchAll();
    startAutoRefresh();
  });

  onDestroy(() => {
    stopAutoRefresh();
  });

  function startAutoRefresh() {
    stopAutoRefresh();
    refreshInterval = setInterval(async () => {
      if (autoRefresh) {
        await fetchStats();
      }
    }, 5000);
  }

  function stopAutoRefresh() {
    if (refreshInterval) {
      clearInterval(refreshInterval);
      refreshInterval = null;
    }
  }

  async function fetchAll() {
    try {
      loading = true;
      error = '';
      const [statusRes, statsRes] = await Promise.all([
        fetch('/api/traffic/status').then(r => r.json()),
        fetch('/api/traffic/stats').then(r => r.json()),
      ]);

      if (statusRes.error) {
        error = statusRes.error;
      } else {
        trafficStatus = statusRes;
      }

      if (statsRes.error) {
        error = error ? error + '; ' + statsRes.error : statsRes.error;
      } else {
        trafficStats = statsRes;
        // Update history
        const now = new Date();
        const timeStr = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
        trafficHistory = [
          ...trafficHistory.slice(-(MAX_HISTORY - 1)),
          {
            time: timeStr,
            interfaces: statsRes.total_interface_limits || 0,
            ips: statsRes.total_ip_limits || 0,
            apps: statsRes.total_app_classes || 0,
          }
        ];
      }
    } catch (e) {
      error = 'Failed to fetch traffic control data';
    } finally {
      loading = false;
    }
  }

  async function fetchStats() {
    try {
      error = '';
      const res = await fetch('/api/traffic/stats');
      const data = await res.json();
      if (!data.error) {
        trafficStats = data;
        trafficStatus = data;
        const now = new Date();
        const timeStr = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
        trafficHistory = [
          ...trafficHistory.slice(-(MAX_HISTORY - 1)),
          {
            time: timeStr,
            interfaces: data.total_interface_limits || 0,
            ips: data.total_ip_limits || 0,
            apps: data.total_app_classes || 0,
          }
        ];
      }
    } catch (e) {
      // silently ignore
    }
  }

  async function setInterfaceLimit() {
    try {
      error = '';
      success = '';
      if (!ifaceForm.interface || !ifaceForm.rate) {
        error = 'Interface and rate are required';
        return;
      }

      const res = await fetch('/api/traffic/limit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          type: 'interface',
          target: ifaceForm.interface,
          rate: parseInt(ifaceForm.rate),
          burst: parseInt(ifaceForm.burst) || 150000,
          direction: ifaceForm.direction,
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Interface limit set';
        ifaceForm = { interface: '', rate: '', burst: '150000', direction: 'both' };
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to set interface limit';
    }
  }

  async function removeInterfaceLimit(iface: string) {
    if (!confirm(`Remove bandwidth limit from ${iface}?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch(`/api/traffic/limit/interface/${encodeURIComponent(iface)}`, {
        method: 'DELETE'
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Limit removed';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to remove interface limit';
    }
  }

  async function setIpLimit() {
    try {
      error = '';
      success = '';
      if (!ipForm.ip || !ipForm.rate) {
        error = 'IP and rate are required';
        return;
      }

      const res = await fetch('/api/traffic/limit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          type: 'ip',
          ip: ipForm.ip,
          rate: parseInt(ipForm.rate),
          burst: parseInt(ipForm.burst) || 75000,
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'IP limit set';
        ipForm = { ip: '', rate: '', burst: '75000' };
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to set IP limit';
    }
  }

  async function removeIpLimit(ip: string) {
    if (!confirm(`Remove bandwidth limit from ${ip}?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch(`/api/traffic/limit/ip/${encodeURIComponent(ip)}`, {
        method: 'DELETE'
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'IP limit removed';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to remove IP limit';
    }
  }

  async function setPriority() {
    try {
      error = '';
      success = '';
      if (!priorityForm.name || !priorityForm.queue) {
        error = 'Name and queue are required';
        return;
      }

      const res = await fetch('/api/traffic/priority', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: priorityForm.name,
          queue: priorityForm.queue,
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Priority set';
        priorityForm = { name: '', queue: 'medium' };
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to set priority';
    }
  }

  async function setAppClass() {
    try {
      error = '';
      success = '';
      if (!appClassForm.name) {
        error = 'App class name is required';
        return;
      }

      const res = await fetch('/api/traffic/app-class', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: appClassForm.name,
          ports: appClassForm.ports,
          protocol: appClassForm.protocol,
          priority: appClassForm.priority,
          description: appClassForm.description || undefined,
        })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'App class set';
        appClassForm = { name: '', ports: '', protocol: 'tcp', priority: 'medium', description: '' };
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to set app class';
    }
  }

  async function removeAppClass(name: string) {
    if (!confirm(`Remove app class '${name}'?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch(`/api/traffic/app-class/${encodeURIComponent(name)}`, {
        method: 'DELETE'
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'App class removed';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to remove app class';
    }
  }

  async function loadDefaults() {
    if (!confirm('Load default app QoS classes (gaming, video, voip, download)?')) return;
    try {
      error = '';
      success = '';
      const res = await fetch('/api/traffic/defaults', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Defaults loaded';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to load defaults';
    }
  }

  async function resetAll() {
    if (!confirm('Remove ALL traffic control rules? This cannot be undone.')) return;
    try {
      error = '';
      success = '';
      const res = await fetch('/api/traffic/reset', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'All rules reset';
        await fetchAll();
      }
    } catch (e) {
      error = 'Failed to reset traffic control';
    }
  }

  function formatRate(bitsPerSec: number): string {
    if (bitsPerSec >= 1_000_000_000) return `${(bitsPerSec / 1_000_000_000).toFixed(1)} Gbps`;
    if (bitsPerSec >= 1_000_000) return `${(bitsPerSec / 1_000_000).toFixed(1)} Mbps`;
    if (bitsPerSec >= 1_000) return `${(bitsPerSec / 1_000).toFixed(1)} Kbps`;
    return `${bitsPerSec} bps`;
  }

  function priorityColor(p: string): string {
    if (p === 'high') return '#00ff88';
    if (p === 'medium') return '#ffd93d';
    return '#ff6b6b';
  }

  function getBarWidth(val: number, max: number): number {
    if (max === 0) return 0;
    return Math.max((val / max) * 100, 2);
  }
</script>

<svelte:head>
  <title>VectorOS - Traffic Control</title>
</svelte:head>

<div class="traffic-page">
  <h1>Traffic Control</h1>
  <p class="subtitle">Advanced bandwidth shaping, per-IP limits, priority queues, and application QoS</p>

  <!-- Status Card -->
  <div class="status-card">
    <div class="status-header">
      <h2>Overview</h2>
      <div class="button-row">
        <button class="btn-sm" class:btn-active={autoRefresh} on:click={() => { autoRefresh = !autoRefresh; if (autoRefresh) startAutoRefresh(); else stopAutoRefresh(); }}>
          {autoRefresh ? 'Auto-refresh ON' : 'Auto-refresh OFF'}
        </button>
        <button class="btn-sm btn-secondary" on:click={fetchAll}>Refresh</button>
      </div>
    </div>

    {#if loading}
      <p>Loading...</p>
    {:else if trafficStatus}
      <div class="stat-grid">
        <div class="stat-item">
          <span class="stat-label">Interface Limits</span>
          <span class="stat-value">{trafficStatus.total_interface_limits || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">IP Limits</span>
          <span class="stat-value">{trafficStatus.total_ip_limits || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">App Classes</span>
          <span class="stat-value">{trafficStatus.total_app_classes || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Global Status</span>
          <span class="status-badge" class:enabled={trafficStatus.global_enabled} class:disabled={!trafficStatus.global_enabled}>
            {trafficStatus.global_enabled ? 'ENABLED' : 'DISABLED'}
          </span>
        </div>
      </div>
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}
  {#if success}
    <div class="success-card">{success}</div>
  {/if}

  <!-- Traffic Activity Over Time -->
  {#if trafficHistory.length > 1}
    <div class="chart-card">
      <h2>Active Rules Over Time</h2>
      <div class="sparkline-container">
        {@const maxVal = Math.max(...trafficHistory.map(h => h.interfaces + h.ips + h.apps), 1)}
        <div class="sparkline">
          {#each trafficHistory as point, i}
            {@const h = ((point.interfaces + point.ips + point.apps) / maxVal) * 100}
            <div
              class="spark-bar"
              style="height: {Math.max(h, 2)}%"
              title="{point.time}: {point.interfaces} iface, {point.ips} IPs, {point.apps} apps"
            ></div>
          {/each}
        </div>
        <div class="sparkline-labels">
          <span>{trafficHistory[0]?.time || ''}</span>
          <span>{trafficHistory[Math.floor(trafficHistory.length / 2)]?.time || ''}</span>
          <span>{trafficHistory[trafficHistory.length - 1]?.time || ''}</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- Bandwidth Allocation Visualization -->
  {#if trafficStatus && trafficStatus.interface_limits && Object.keys(trafficStatus.interface_limits).length > 0}
    <div class="chart-card">
      <h2>Bandwidth Allocation</h2>
      <div class="bandwidth-bars">
        {@const maxRate = Math.max(...Object.values(trafficStatus.interface_limits).map((l: any) => l.rate || 0), 1)}
        {#each Object.entries(trafficStatus.interface_limits) as [iface, limit]}
          <div class="bw-row">
            <span class="bw-label">{iface}</span>
            <div class="bw-track">
              <div class="bw-fill" style="width: {getBarWidth((limit as any).rate, maxRate)}%"></div>
            </div>
            <span class="bw-rate">{formatRate((limit as any).rate)}</span>
            <span class="bw-dir">{(limit as any).direction}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Per-Device Bandwidth Usage -->
  {#if trafficStatus && trafficStatus.ip_limits && Object.keys(trafficStatus.ip_limits).length > 0}
    <div class="chart-card">
      <h2>Per-Device Bandwidth Limits</h2>
      <div class="bandwidth-bars">
        {@const maxRate = Math.max(...Object.values(trafficStatus.ip_limits).map((l: any) => l.rate || 0), 1)}
        {#each Object.entries(trafficStatus.ip_limits) as [ip, limit]}
          <div class="bw-row">
            <span class="bw-label">{ip}</span>
            <div class="bw-track">
              <div class="bw-fill bw-fill-ip" style="width: {getBarWidth((limit as any).rate, maxRate)}%"></div>
            </div>
            <span class="bw-rate">{formatRate((limit as any).rate)}</span>
            <button class="btn-x" on:click={() => removeIpLimit(ip)} title="Remove limit">x</button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Priority Queue Visualization -->
  <div class="chart-card">
    <h2>Priority Queues</h2>
    <div class="priority-bars">
      {#each priorityLevels as p}
        <div class="priority-row">
          <span class="priority-label" style="color: {p.color}">{p.label}</span>
          <div class="priority-track">
            <div class="priority-fill" style="width: {p.weight}%; background: {p.color}"></div>
          </div>
          <span class="priority-weight">{p.weight}%</span>
        </div>
      {/each}
    </div>
  </div>

  <div class="grid-2col">
    <!-- Set Interface Limit -->
    <div class="config-card">
      <h2>Set Interface Bandwidth Limit</h2>
      <form on:submit|preventDefault={setInterfaceLimit}>
        <div class="form-row">
          <div class="form-group">
            <label for="iface-name">Interface</label>
            <input type="text" id="iface-name" bind:value={ifaceForm.interface} placeholder="e.g. GigabitEthernet0/0/0" />
          </div>
          <div class="form-group">
            <label for="iface-dir">Direction</label>
            <select id="iface-dir" bind:value={ifaceForm.direction}>
              {#each directions as d}
                <option value={d.value}>{d.label}</option>
              {/each}
            </select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="iface-rate">Rate (bits/sec)</label>
            <input type="number" id="iface-rate" bind:value={ifaceForm.rate} placeholder="e.g. 100000000" min="1" />
          </div>
          <div class="form-group">
            <label for="iface-burst">Burst (bytes)</label>
            <input type="number" id="iface-burst" bind:value={ifaceForm.burst} placeholder="e.g. 150000" min="1" />
          </div>
        </div>
        <button type="submit" class="btn-primary">Apply Interface Limit</button>
      </form>
    </div>

    <!-- Set IP Limit -->
    <div class="config-card">
      <h2>Set Per-IP Bandwidth Limit</h2>
      <form on:submit|preventDefault={setIpLimit}>
        <div class="form-group">
          <label for="ip-addr">IP Address</label>
          <input type="text" id="ip-addr" bind:value={ipForm.ip} placeholder="e.g. 192.168.1.100" />
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="ip-rate">Rate (bits/sec)</label>
            <input type="number" id="ip-rate" bind:value={ipForm.rate} placeholder="e.g. 50000000" min="1" />
          </div>
          <div class="form-group">
            <label for="ip-burst">Burst (bytes)</label>
            <input type="number" id="ip-burst" bind:value={ipForm.burst} placeholder="e.g. 75000" min="1" />
          </div>
        </div>
        <button type="submit" class="btn-primary">Apply IP Limit</button>
      </form>
    </div>
  </div>

  <div class="grid-2col">
    <!-- Set Priority -->
    <div class="config-card">
      <h2>Set Priority Queue</h2>
      <form on:submit|preventDefault={setPriority}>
        <div class="form-group">
          <label for="prio-name">Traffic Class Name</label>
          <input type="text" id="prio-name" bind:value={priorityForm.name} placeholder="e.g. gaming" />
        </div>
        <div class="form-group">
          <label for="prio-level">Priority Level</label>
          <select id="prio-level" bind:value={priorityForm.queue}>
            {#each priorityLevels as p}
              <option value={p.value}>{p.label} (weight {p.weight}%)</option>
            {/each}
          </select>
        </div>
        <button type="submit" class="btn-primary">Set Priority</button>
      </form>
    </div>

    <!-- Set App Class -->
    <div class="config-card">
      <h2>Set Application QoS Class</h2>
      <form on:submit|preventDefault={setAppClass}>
        <div class="form-row">
          <div class="form-group">
            <label for="app-name">App Name</label>
            <input type="text" id="app-name" bind:value={appClassForm.name} placeholder="e.g. gaming" />
          </div>
          <div class="form-group">
            <label for="app-priority">Priority</label>
            <select id="app-priority" bind:value={appClassForm.priority}>
              {#each priorityLevels as p}
                <option value={p.value}>{p.label}</option>
              {/each}
            </select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="app-ports">Ports (comma-separated)</label>
            <input type="text" id="app-ports" bind:value={appClassForm.ports} placeholder="e.g. 443,80 or 3074,27015-27030" />
          </div>
          <div class="form-group">
            <label for="app-proto">Protocol</label>
            <select id="app-proto" bind:value={appClassForm.protocol}>
              {#each protocols as p}
                <option value={p.value}>{p.label}</option>
              {/each}
            </select>
          </div>
        </div>
        <div class="form-group">
          <label for="app-desc">Description</label>
          <input type="text" id="app-desc" bind:value={appClassForm.description} placeholder="Optional description" />
        </div>
        <button type="submit" class="btn-primary">Set App Class</button>
      </form>
    </div>
  </div>

  <!-- Active Interface Limits -->
  {#if trafficStatus && trafficStatus.interface_limits && Object.keys(trafficStatus.interface_limits).length > 0}
    <div class="config-card">
      <h2>Active Interface Limits</h2>
      <div class="table-wrapper">
        <table>
          <thead>
            <tr>
              <th>Interface</th>
              <th>Rate</th>
              <th>Burst</th>
              <th>Direction</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each Object.entries(trafficStatus.interface_limits) as [iface, limit]}
              <tr>
                <td class="name-cell">{iface}</td>
                <td>{formatRate((limit as any).rate)}</td>
                <td>{(limit as any).burst} bytes</td>
                <td><span class="dir-badge">{(limit as any).direction}</span></td>
                <td>
                  <button class="btn-delete" on:click={() => removeInterfaceLimit(iface)}>Remove</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  <!-- Active App Classes -->
  {#if trafficStatus && trafficStatus.app_classes && Object.keys(trafficStatus.app_classes).length > 0}
    <div class="config-card">
      <h2>Application QoS Classes</h2>
      <div class="table-wrapper">
        <table>
          <thead>
            <tr>
              <th>App</th>
              <th>Priority</th>
              <th>DSCP</th>
              <th>Protocol</th>
              <th>Ports</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each Object.entries(trafficStatus.app_classes) as [name, cls]}
              <tr>
                <td class="name-cell">{name}</td>
                <td>
                  <span class="priority-badge" style="background: {(cls as any).priority === 'high' ? '#003322' : (cls as any).priority === 'low' ? '#331111' : '#332200'}; color: {priorityColor((cls as any).priority)}">
                    {(cls as any).priority}
                  </span>
                </td>
                <td>{(cls as any).dscp}</td>
                <td>{(cls as any).protocol || 'any'}</td>
                <td class="ports-cell">{(cls as any).ports || 'all'}</td>
                <td>
                  <button class="btn-delete" on:click={() => removeAppClass(name)}>Remove</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  <!-- VPP Output -->
  {#if trafficStats && trafficStats.vpp_policers && trafficStats.vpp_policers !== 'N/A'}
    <div class="config-card">
      <h2>VPP Policer Output</h2>
      <pre class="vpp-output">{trafficStats.vpp_policers}</pre>
    </div>
  {/if}

  <!-- Reset -->
  <div class="config-card reset-card">
    <h2>Reset All Traffic Control</h2>
    <p class="reset-desc">Remove all interface limits, IP limits, and app classes. This cannot be undone.</p>
    <button class="btn-danger" on:click={resetAll}>Reset All Rules</button>
  </div>
</div>

<style>
  .traffic-page {
    max-width: 1400px;
  }

  .subtitle {
    color: #888;
    margin-bottom: 1.5rem;
    font-size: 0.95rem;
  }

  h1 {
    margin-bottom: 0.5rem;
    color: #00ff88;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
    font-size: 1.1rem;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .status-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 1rem;
    margin-top: 1rem;
  }

  .stat-item {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .stat-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
  }

  .stat-value {
    font-size: 1.4rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  .status-badge {
    padding: 0.2rem 0.8rem;
    border-radius: 0.3rem;
    font-weight: bold;
    font-size: 0.85rem;
    display: inline-block;
    margin-top: 0.3rem;
  }

  .status-badge.enabled { background: #003322; color: #00ff88; }
  .status-badge.disabled { background: #331111; color: #ff4444; }

  .chart-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .sparkline-container { padding: 0.5rem 0; }

  .sparkline {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 60px;
    background: #0f0f23;
    border-radius: 0.5rem;
    padding: 0.5rem;
  }

  .spark-bar {
    flex: 1;
    background: linear-gradient(to top, #00ff88, #00cc66);
    border-radius: 2px 2px 0 0;
    min-width: 4px;
    transition: height 0.3s ease;
  }

  .sparkline-labels {
    display: flex;
    justify-content: space-between;
    font-size: 0.7rem;
    color: #888;
    margin-top: 0.3rem;
    padding: 0 0.5rem;
  }

  /* Bandwidth bars */
  .bandwidth-bars { display: flex; flex-direction: column; gap: 0.5rem; }

  .bw-row {
    display: grid;
    grid-template-columns: 140px 1fr 100px 70px 30px;
    gap: 0.5rem;
    align-items: center;
  }

  .bw-label {
    font-size: 0.85rem;
    color: #e0e0e0;
    font-family: monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bw-track {
    height: 16px;
    background: #0f0f23;
    border-radius: 0.25rem;
    overflow: hidden;
  }

  .bw-fill {
    height: 100%;
    background: linear-gradient(90deg, #00ff88, #00cc66);
    border-radius: 0.25rem;
    transition: width 0.3s ease;
  }

  .bw-fill-ip {
    background: linear-gradient(90deg, #6bcbff, #3399ff);
  }

  .bw-rate {
    font-size: 0.8rem;
    color: #e0e0e0;
    text-align: right;
    font-family: monospace;
  }

  .bw-dir {
    font-size: 0.75rem;
    color: #888;
  }

  /* Priority bars */
  .priority-bars { display: flex; flex-direction: column; gap: 0.75rem; }

  .priority-row {
    display: grid;
    grid-template-columns: 80px 1fr 60px;
    gap: 0.75rem;
    align-items: center;
  }

  .priority-label { font-size: 0.9rem; font-weight: 600; }

  .priority-track {
    height: 20px;
    background: #0f0f23;
    border-radius: 0.25rem;
    overflow: hidden;
  }

  .priority-fill {
    height: 100%;
    border-radius: 0.25rem;
    transition: width 0.3s ease;
  }

  .priority-weight {
    font-size: 0.85rem;
    color: #888;
    text-align: right;
  }

  .grid-2col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.5rem;
    margin-bottom: 1.5rem;
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

  .button-row {
    display: flex;
    gap: 0.5rem;
  }

  .btn-sm {
    padding: 0.4rem 0.8rem;
    font-size: 0.8rem;
    border-radius: 0.4rem;
    border: 1px solid #333;
    background: #16213e;
    color: #e0e0e0;
    cursor: pointer;
  }

  .btn-active {
    background: #003322;
    border-color: #00ff88;
    color: #00ff88;
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

  .btn-delete {
    background: none;
    border: 1px solid #ff4444;
    color: #ff4444;
    padding: 0.3rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .btn-delete:hover { background: #ff4444; color: #fff; }

  .btn-x {
    background: none;
    border: none;
    color: #ff4444;
    cursor: pointer;
    font-size: 0.9rem;
    padding: 0 0.3rem;
  }

  .btn-x:hover { color: #ff6666; }

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

  .table-wrapper { overflow-x: auto; }

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

  .name-cell { font-weight: 600; color: #00ff88; }

  .dir-badge {
    background: #1a1a2e;
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    color: #ffaa00;
  }

  .priority-badge {
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .ports-cell {
    font-family: monospace;
    font-size: 0.8rem;
    color: #88aaff;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .reset-card {
    border: 1px solid #331111;
  }

  .reset-desc {
    color: #888;
    margin-bottom: 1rem;
    font-size: 0.9rem;
  }

  @media (max-width: 900px) {
    .grid-2col { grid-template-columns: 1fr; }
    .bw-row { grid-template-columns: 100px 1fr 80px; }
    .bw-dir, .bw-row .btn-x { display: none; }
  }
</style>
