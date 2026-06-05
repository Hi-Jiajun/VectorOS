<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { initWebSocket, wsStatus } from '$lib/stores/websocket';
  import { getWebSocket, type WsMessage, type ConnectionStatus } from '$lib/websocket';
  import type { Unsubscriber } from 'svelte/store';

  // ── Types ──────────────────────────────────────────────────────────

  interface PppoeClientStatus {
    status: string;
    interface?: string;
    ip_address?: string;
    dns_server?: string;
    uptime?: number;
    local_ip?: string;
    remote_ip?: string;
    mtu?: number;
    mru?: number;
    username?: string;
  }

  interface PppoeClientsResponse {
    status: string;
    clients?: PppoeClientStatus[];
    message?: string;
  }

  interface AutoConnectStatus {
    status: string;
    running: boolean;
    config: AutoConnectConfig;
    total_reconnects: number;
    consecutive_failures: number;
    current_retry_interval: number;
    last_health_check: string | null;
    history: HistoryEntry[];
  }

  interface AutoConnectConfig {
    enabled: boolean;
    max_retries: number;
    retry_interval: number;
    backoff_factor: number;
    max_retry_interval: number;
    check_interval: number;
    health_check_interval: number;
  }

  interface HistoryEntry {
    timestamp: string;
    event_type: string;
    message: string;
  }

  interface VfInterface {
    name: string;
    state: string;
    mtu: number;
    mac_address?: string;
    driver?: string;
  }

  // ── ISP Presets ────────────────────────────────────────────────────

  const ISP_PRESETS = [
    { name: 'China Telecom', icon: '☁', usernameSuffix: '@ct', placeholder: 'e.g. 13800001111@ct' },
    { name: 'China Unicom', icon: '⚡', usernameSuffix: '@cu', placeholder: 'e.g. 13800001111@cu' },
    { name: 'China Mobile', icon: '☕', usernameSuffix: '@cm', placeholder: 'e.g. 13800001111@cm' },
    { name: 'Custom', icon: '✎', usernameSuffix: '', placeholder: 'Your PPPoE username' }
  ];

  // ── State ──────────────────────────────────────────────────────────

  let currentStep = 1;
  const totalSteps = 4;

  // Wizard form data
  let selectedInterface = 'enp1s0';
  let username = '';
  let password = '';
  let showPassword = false;
  let selectedIsp = 'Custom';
  let mtu = 1492;
  let mru = 1492;
  let dnsMode: 'isp' | 'auto' | 'custom' = 'isp';
  let customDns = '8.8.8.8, 1.1.1.1';
  let addDefaultRoute4 = true;
  let addDefaultRoute6 = true;
  let acName = '';
  let serviceName = '';

  // Connection state
  let pppoeClients: PppoeClientStatus[] = [];
  let connectionStatus: PppoeClientStatus | null = null;
  let isConnecting = false;
  let connectError = '';
  let connectSuccess = '';

  // Auto-connect state
  let autoStatus: AutoConnectStatus | null = null;
  let autoLoading = true;
  let autoConfig: AutoConnectConfig = {
    enabled: false,
    max_retries: 0,
    retry_interval: 5,
    backoff_factor: 2.0,
    max_retry_interval: 300,
    check_interval: 10,
    health_check_interval: 60
  };

  // Interface list
  let vfInterfaces: VfInterface[] = [];
  let ifacesLoading = true;

  // Uptime display
  let uptimeStr = '';
  let uptimeInterval: ReturnType<typeof setInterval>;

  // WebSocket
  let unsubWsStatus: Unsubscriber | null = null;
  let unsubWsMessage: (() => void) | null = null;
  let wsConnStatus: ConnectionStatus = 'disconnected';

  // ── Lifecycle ──────────────────────────────────────────────────────

  onMount(async () => {
    initWebSocket();
    unsubWsStatus = wsStatus.subscribe((v) => { wsConnStatus = v; });

    const ws = getWebSocket();
    unsubWsMessage = ws.onMessage((msg: WsMessage) => {
      if (msg.type === 'InterfaceUpdate') {
        const idx = vfInterfaces.findIndex((i) => i.name === msg.name);
        if (idx >= 0) {
          vfInterfaces[idx] = { ...vfInterfaces[idx], state: msg.state };
          vfInterfaces = [...vfInterfaces];
        }
      }
    });

    await Promise.all([fetchVfInterfaces(), fetchPppoeStatus(), fetchAutoStatus()]);
    uptimeInterval = setInterval(updateUptime, 1000);
  });

  onDestroy(() => {
    if (uptimeInterval) clearInterval(uptimeInterval);
    unsubWsStatus?.();
    unsubWsMessage?.();
  });

  // ── API calls ──────────────────────────────────────────────────────

  async function fetchVfInterfaces() {
    try {
      ifacesLoading = true;
      const res = await fetch('/api/interfaces');
      const data = await res.json();
      if (data.interfaces) {
        vfInterfaces = data.interfaces.filter((i: VfInterface) =>
          i.name.startsWith('enp') || i.name.startsWith('vf')
        );
        if (vfInterfaces.length === 0) {
          // Fallback to hardcoded VF names
          vfInterfaces = [
            { name: 'enp1s0', state: 'down', mtu: 1500 },
            { name: 'enp2s0', state: 'down', mtu: 1500 },
            { name: 'enp3s0', state: 'down', mtu: 1500 }
          ];
        }
      }
    } catch {
      vfInterfaces = [
        { name: 'enp1s0', state: 'unknown', mtu: 1500 },
        { name: 'enp2s0', state: 'unknown', mtu: 1500 },
        { name: 'enp3s0', state: 'unknown', mtu: 1500 }
      ];
    } finally {
      ifacesLoading = false;
    }
  }

  async function fetchPppoeStatus() {
    try {
      const res = await fetch('/api/pppoe/clients');
      const data: PppoeClientsResponse = await res.json();
      if (!data.error && data.clients) {
        pppoeClients = data.clients;
        connectionStatus = data.clients.find((c) => c.status === 'connected') || null;
      }
    } catch {
      // Silent
    }
  }

  async function fetchAutoStatus() {
    try {
      autoLoading = true;
      const res = await fetch('/api/pppoe/autoconnect/status');
      autoStatus = await res.json();
      if (autoStatus?.config) {
        autoConfig = { ...autoConfig, ...autoStatus.config };
      }
    } catch {
      // Silent
    } finally {
      autoLoading = false;
    }
  }

  async function toggleInterfaceUp(name: string) {
    const iface = vfInterfaces.find((i) => i.name === name);
    if (!iface) return;
    const newState = iface.state === 'up' ? 'down' : 'up';
    try {
      const res = await fetch(`/api/interfaces/${encodeURIComponent(name)}/${newState}`, { method: 'POST' });
      const data = await res.json();
      if (!data.error) {
        await fetchVfInterfaces();
      }
    } catch {
      // Silent
    }
  }

  async function connectPppoe() {
    if (!username || !selectedInterface) {
      connectError = 'Username and interface are required';
      return;
    }

    isConnecting = true;
    connectError = '';
    connectSuccess = '';

    try {
      const res = await fetch('/api/pppoe/create', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          username,
          password,
          interface: selectedInterface,
          ac_name: acName || undefined,
          service_name: serviceName || undefined,
          mtu,
          mru,
          use_peer_dns: dnsMode === 'isp',
          custom_dns: dnsMode === 'custom' ? customDns : undefined,
          add_default_route4,
          add_default_route6
        })
      });
      const data = await res.json();
      if (data.error) {
        connectError = data.error;
      } else {
        connectSuccess = data.message || 'PPPoE connection initiated';
        // Poll status a few times to confirm
        for (let i = 0; i < 5; i++) {
          await new Promise((r) => setTimeout(r, 1000));
          await fetchPppoeStatus();
          if (connectionStatus?.status === 'connected') break;
        }
      }
    } catch {
      connectError = 'Failed to connect: server unreachable';
    } finally {
      isConnecting = false;
    }
  }

  async function disconnectPppoe() {
    try {
      const res = await fetch('/api/pppoe/disconnect', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        connectError = data.error;
      } else {
        connectionStatus = null;
        connectSuccess = 'Disconnected';
        await fetchPppoeStatus();
      }
    } catch {
      connectError = 'Failed to disconnect';
    }
  }

  // Auto-connect handlers
  async function startAutoConnect() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password })
      });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch {
      // Silent
    }
  }

  async function stopAutoConnect() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/stop', { method: 'POST' });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch {
      // Silent
    }
  }

  async function saveAutoConfig() {
    try {
      const res = await fetch('/api/pppoe/autoconnect/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(autoConfig)
      });
      autoStatus = await res.json();
      await fetchAutoStatus();
    } catch {
      // Silent
    }
  }

  // ── Uptime ─────────────────────────────────────────────────────────

  function updateUptime() {
    if (connectionStatus?.uptime) {
      uptimeStr = formatUptime(connectionStatus.uptime);
    }
  }

  function formatUptime(seconds: number): string {
    const d = Math.floor(seconds / 86400);
    const h = Math.floor((seconds % 86400) / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  // ── Wizard navigation ──────────────────────────────────────────────

  function nextStep() {
    if (currentStep < totalSteps) currentStep++;
  }

  function prevStep() {
    if (currentStep > 1) currentStep--;
  }

  function goToStep(step: number) {
    if (step >= 1 && step <= totalSteps) currentStep = step;
  }

  // ── Helpers ────────────────────────────────────────────────────────

  function statusColor(status: string): string {
    switch (status) {
      case 'connected': return '#00ff88';
      case 'connecting':
      case 'retrying': return '#ffaa00';
      case 'failed': return '#ff4444';
      case 'disabled':
      case 'idle': return '#666';
      default: return '#666';
    }
  }

  function applyIspPreset(isp: typeof ISP_PRESETS[number]) {
    selectedIsp = isp.name;
    if (isp.usernameSuffix && !username.includes('@')) {
      // Only append if the username doesn't already have an ISP suffix
    }
  }

  function formatTimestamp(ts: string | null): string {
    if (!ts) return 'N/A';
    try { return new Date(ts).toLocaleString(); } catch { return ts; }
  }

  function eventTypeLabel(type: string): string {
    const labels: Record<string, string> = {
      connected: 'Connected', disconnected: 'Disconnected', connect_attempt: 'Connecting',
      connect_failed: 'Failed', health_check_ok: 'Health OK', health_check_failed: 'Health Fail',
      retry_scheduled: 'Retry Pending', started: 'Started', stopped: 'Stopped',
      dns_refresh: 'DNS Refresh', error: 'Error', max_retries_reached: 'Max Retries'
    };
    return labels[type] || type;
  }

  function eventTypeColor(type: string): string {
    switch (type) {
      case 'connected': return '#00ff88';
      case 'disconnected': return '#ff4444';
      case 'connect_failed': return '#ff6644';
      case 'health_check_ok': return '#00ff88';
      case 'health_check_failed': return '#ffaa00';
      case 'error': return '#ff4444';
      case 'max_retries_reached': return '#ff4444';
      default: return '#888';
    }
  }

  // Step validation
  $: step1Valid = selectedInterface !== '' && vfInterfaces.find((i) => i.name === selectedInterface);
  $: step2Valid = username.trim().length > 0;
  $: step3Valid = mtu >= 576 && mtu <= 1500 && mru >= 576 && mru <= 1500;
  $: canProceed = currentStep === 1 ? step1Valid : currentStep === 2 ? step2Valid : currentStep === 3 ? step3Valid : true;
</script>

<svelte:head>
  <title>VectorOS - PPPoE Wizard</title>
</svelte:head>

<div class="pppoe-page">
  <!-- ── Connection Status (shown when connected) ── -->
  {#if connectionStatus && connectionStatus.status === 'connected'}
    <div class="connected-banner">
      <div class="connected-header">
        <span class="connected-dot"></span>
        <h2>PPPoE Connected</h2>
        <button class="btn-disconnect" on:click={disconnectPppoe}>Disconnect</button>
      </div>
      <div class="connected-stats">
        <div class="connected-stat">
          <span class="cs-label">Interface</span>
          <span class="cs-value">{connectionStatus.interface || selectedInterface}</span>
        </div>
        <div class="connected-stat">
          <span class="cs-label">Local IP</span>
          <span class="cs-value ip">{connectionStatus.local_ip || connectionStatus.ip_address || 'N/A'}</span>
        </div>
        <div class="connected-stat">
          <span class="cs-label">Remote IP</span>
          <span class="cs-value">{connectionStatus.remote_ip || 'N/A'}</span>
        </div>
        <div class="connected-stat">
          <span class="cs-label">DNS</span>
          <span class="cs-value">{connectionStatus.dns_server || 'N/A'}</span>
        </div>
        <div class="connected-stat">
          <span class="cs-label">Uptime</span>
          <span class="cs-value uptime">{uptimeStr || formatUptime(connectionStatus.uptime || 0)}</span>
        </div>
        <div class="connected-stat">
          <span class="cs-label">MTU / MRU</span>
          <span class="cs-value">{connectionStatus.mtu || mtu} / {connectionStatus.mru || mru}</span>
        </div>
      </div>
    </div>
  {/if}

  <h1>PPPoE Connection Wizard</h1>

  <!-- ── Wizard Steps Indicator ── -->
  <div class="stepper">
    {#each [
      { num: 1, label: 'Interface', icon: '🌐' },
      { num: 2, label: 'Credentials', icon: '🔑' },
      { num: 3, label: 'Options', icon: '⚙' },
      { num: 4, label: 'Connect', icon: '✔' }
    ] as step}
      <button
        class="step"
        class:active={currentStep === step.num}
        class:completed={currentStep > step.num}
        on:click={() => goToStep(step.num)}
      >
        <span class="step-num">
          {#if currentStep > step.num}
            &#10003;
          {:else}
            {step.num}
          {/if}
        </span>
        <span class="step-label">{step.label}</span>
      </button>
      {#if step.num < totalSteps}
        <div class="step-line" class:filled={currentStep > step.num}></div>
      {/if}
    {/each}
  </div>

  <!-- ── Step Content ── -->
  <div class="wizard-card">
    {#if currentStep === 1}
      <!-- STEP 1: Select Interface -->
      <div class="step-content">
        <h2>Select WAN Interface</h2>
        <p class="step-desc">Choose a VF interface for your PPPoE connection. The interface must be up to connect.</p>

        {#if ifacesLoading}
          <div class="loading-row">
            <div class="spinner"></div>
            <span>Loading interfaces...</span>
          </div>
        {:else}
          <div class="iface-grid">
            {#each vfInterfaces as iface}
              <button
                class="iface-card"
                class:selected={selectedInterface === iface.name}
                class:iface-up={iface.state === 'up'}
                on:click={() => { selectedInterface = iface.name; }}
              >
                <div class="iface-card-header">
                  <span class="iface-card-dot" class:up={iface.state === 'up'} class:down={iface.state !== 'up'}></span>
                  <span class="iface-card-name">{iface.name}</span>
                  {#if selectedInterface === iface.name}
                    <span class="iface-card-check">&#10003;</span>
                  {/if}
                </div>
                <div class="iface-card-details">
                  <span class="iface-card-state" class:state-up={iface.state === 'up'} class:state-down={iface.state !== 'up'}>
                    {iface.state}
                  </span>
                  <span class="iface-card-meta">MTU {iface.mtu}</span>
                  {#if iface.mac_address}
                    <span class="iface-card-mac">{iface.mac_address}</span>
                  {/if}
                </div>
                <button
                  class="btn-toggle-iface"
                  on:click|stopPropagation={() => toggleInterfaceUp(iface.name)}
                  title={iface.state === 'up' ? 'Bring down' : 'Bring up'}
                >
                  {iface.state === 'up' ? '⏻ Bring Down' : '⏻ Bring Up'}
                </button>
              </button>
            {/each}
          </div>

          {#if vfInterfaces.length === 0}
            <div class="empty-hint">
              No VF interfaces found. Bind interfaces on the <a href="/interfaces">Interfaces</a> page first.
            </div>
          {/if}
        {/if}
      </div>

    {:else if currentStep === 2}
      <!-- STEP 2: Enter Credentials -->
      <div class="step-content">
        <h2>Enter Credentials</h2>
        <p class="step-desc">Provide your PPPoE username and password from your ISP.</p>

        <!-- ISP Presets -->
        <div class="isp-section">
          <label class="form-label">ISP Preset</label>
          <div class="isp-grid">
            {#each ISP_PRESETS as isp}
              <button
                class="isp-btn"
                class:selected={selectedIsp === isp.name}
                on:click={() => applyIspPreset(isp)}
              >
                <span class="isp-icon">{isp.icon}</span>
                <span class="isp-name">{isp.name}</span>
              </button>
            {/each}
          </div>
        </div>

        <!-- Username -->
        <div class="form-group">
          <label class="form-label" for="pppoe-username">Username</label>
          <input
            type="text"
            id="pppoe-username"
            bind:value={username}
            placeholder={ISP_PRESETS.find((p) => p.name === selectedIsp)?.placeholder || 'Your PPPoE username'}
            class:input-error={currentStep === 2 && !step2Valid}
          />
          {#if currentStep === 2 && !step2Valid}
            <span class="field-error">Username is required</span>
          {/if}
          {#if selectedIsp !== 'Custom'}
            <span class="field-hint">ISP suffix: {ISP_PRESETS.find((p) => p.name === selectedIsp)?.usernameSuffix}</span>
          {/if}
        </div>

        <!-- Password -->
        <div class="form-group">
          <label class="form-label" for="pppoe-password">Password</label>
          <div class="password-field">
            <input
              type={showPassword ? 'text' : 'password'}
              id="pppoe-password"
              bind:value={password}
              placeholder="PPPoE password"
            />
            <button
              class="btn-toggle-pw"
              type="button"
              on:click={() => showPassword = !showPassword}
              title={showPassword ? 'Hide password' : 'Show password'}
            >
              {showPassword ? '👁' : '🔒'}
            </button>
          </div>
        </div>

        <!-- AC Name / Service Name (advanced) -->
        <details class="advanced-section">
          <summary>Advanced Options</summary>
          <div class="form-group">
            <label class="form-label" for="ac-name">AC Name Filter (optional)</label>
            <input type="text" id="ac-name" bind:value={acName} placeholder="Leave empty for any" />
          </div>
          <div class="form-group">
            <label class="form-label" for="svc-name">Service Name (optional)</label>
            <input type="text" id="svc-name" bind:value={serviceName} placeholder="Leave empty for any" />
          </div>
        </details>
      </div>

    {:else if currentStep === 3}
      <!-- STEP 3: Configure Options -->
      <div class="step-content">
        <h2>Configure Options</h2>
        <p class="step-desc">Adjust MTU/MRU, DNS, and routing settings for your connection.</p>

        <!-- MTU / MRU -->
        <div class="form-row-2">
          <div class="form-group">
            <label class="form-label" for="pppoe-mtu">MTU</label>
            <input type="number" id="pppoe-mtu" bind:value={mtu} min="576" max="1500" />
            <span class="field-hint">PPPoE standard: 1492</span>
          </div>
          <div class="form-group">
            <label class="form-label" for="pppoe-mru">MRU</label>
            <input type="number" id="pppoe-mru" bind:value={mru} min="576" max="1500" />
            <span class="field-hint">PPPoE standard: 1492</span>
          </div>
        </div>

        <!-- DNS Settings -->
        <div class="form-group">
          <label class="form-label">DNS Settings</label>
          <div class="dns-options">
            <button
              class="dns-option"
              class:selected={dnsMode === 'isp'}
              on:click={() => dnsMode = 'isp'}
            >
              <span class="dns-opt-title">Use ISP DNS</span>
              <span class="dns-opt-desc">Accept DNS servers from your ISP automatically</span>
            </button>
            <button
              class="dns-option"
              class:selected={dnsMode === 'auto'}
              on:click={() => dnsMode = 'auto'}
            >
              <span class="dns-opt-title">System Default</span>
              <span class="dns-opt-desc">Use the system's default DNS resolver</span>
            </button>
            <button
              class="dns-option"
              class:selected={dnsMode === 'custom'}
              on:click={() => dnsMode = 'custom'}
            >
              <span class="dns-opt-title">Custom DNS</span>
              <span class="dns-opt-desc">Specify your own DNS servers</span>
            </button>
          </div>
          {#if dnsMode === 'custom'}
            <input
              type="text"
              bind:value={customDns}
              placeholder="8.8.8.8, 1.1.1.1"
              class="custom-dns-input"
            />
          {/if}
        </div>

        <!-- Default Routes -->
        <div class="form-group">
          <label class="form-label">Default Route</label>
          <div class="toggle-group">
            <label class="toggle-label">
              <input type="checkbox" bind:checked={addDefaultRoute4} />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
              IPv4 Default Route
            </label>
            <label class="toggle-label">
              <input type="checkbox" bind:checked={addDefaultRoute6} />
              <span class="toggle-track"><span class="toggle-thumb"></span></span>
              IPv6 Default Route
            </label>
          </div>
        </div>
      </div>

    {:else if currentStep === 4}
      <!-- STEP 4: Review and Connect -->
      <div class="step-content">
        <h2>Review & Connect</h2>
        <p class="step-desc">Verify your configuration, then connect.</p>

        <!-- Summary -->
        <div class="review-grid">
          <div class="review-item">
            <span class="review-label">Interface</span>
            <span class="review-value">{selectedInterface}</span>
          </div>
          <div class="review-item">
            <span class="review-label">Username</span>
            <span class="review-value">{username}</span>
          </div>
          <div class="review-item">
            <span class="review-label">Password</span>
            <span class="review-value">{password ? '•'.repeat(Math.min(password.length, 12)) : '(not set)'}</span>
          </div>
          <div class="review-item">
            <span class="review-label">MTU / MRU</span>
            <span class="review-value">{mtu} / {mru}</span>
          </div>
          <div class="review-item">
            <span class="review-label">DNS</span>
            <span class="review-value">
              {dnsMode === 'isp' ? 'ISP (auto)' : dnsMode === 'auto' ? 'System default' : customDns}
            </span>
          </div>
          <div class="review-item">
            <span class="review-label">Default Routes</span>
            <span class="review-value">
              {addDefaultRoute4 ? 'IPv4' : ''}
              {addDefaultRoute4 && addDefaultRoute6 ? ' + ' : ''}
              {addDefaultRoute6 ? 'IPv6' : ''}
              {!addDefaultRoute4 && !addDefaultRoute6 ? 'None' : ''}
            </span>
          </div>
          {#if acName || serviceName}
            <div class="review-item">
              <span class="review-label">AC Name</span>
              <span class="review-value">{acName || '(any)'}</span>
            </div>
            <div class="review-item">
              <span class="review-label">Service Name</span>
              <span class="review-value">{serviceName || '(any)'}</span>
            </div>
          {/if}
        </div>

        <!-- Connect Button -->
        <div class="connect-actions">
          {#if connectError}
            <div class="alert alert-error">{connectError}</div>
          {/if}
          {#if connectSuccess}
            <div class="alert alert-success">{connectSuccess}</div>
          {/if}

          <button
            class="btn-connect"
            on:click={connectPppoe}
            disabled={isConnecting}
          >
            {#if isConnecting}
              <span class="btn-spinner"></span> Connecting...
            {:else}
              Connect
            {/if}
          </button>
        </div>
      </div>
    {/if}

    <!-- ── Step Navigation ── -->
    <div class="wizard-nav">
      {#if currentStep > 1}
        <button class="btn-back" on:click={prevStep}>&larr; Back</button>
      {:else}
        <div></div>
      {/if}
      {#if currentStep < totalSteps}
        <button class="btn-next" on:click={nextStep} disabled={!canProceed}>
          Next &rarr;
        </button>
      {/if}
    </div>
  </div>

  <!-- ── Auto-Connect Section ── -->
  <div class="autoconnect-card">
    <h2>Auto-Connect</h2>
    {#if autoLoading}
      <p class="muted">Loading...</p>
    {:else}
      <div class="autoconnect-controls">
        <div class="autoconnect-toggle">
          <span class="status-indicator" style="background: {statusColor(autoStatus?.status || 'idle')}"></span>
          <span>Status: <strong style="color: {statusColor(autoStatus?.status || 'idle')}">{autoStatus?.status || 'idle'}</strong></span>
          {#if autoStatus?.running}
            <button class="btn-stop" on:click={stopAutoConnect}>Stop</button>
          {:else}
            <button class="btn-start" on:click={startAutoConnect}>Start</button>
          {/if}
        </div>

        <div class="autoconnect-stats">
          <div class="stat">
            <span class="stat-label">Reconnects</span>
            <span class="stat-value">{autoStatus?.total_reconnects || 0}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Failures</span>
            <span class="stat-value">{autoStatus?.consecutive_failures || 0}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Retry Interval</span>
            <span class="stat-value">{autoStatus?.current_retry_interval || 0}s</span>
          </div>
          <div class="stat">
            <span class="stat-label">Last Health Check</span>
            <span class="stat-value">{formatTimestamp(autoStatus?.last_health_check)}</span>
          </div>
        </div>

        <!-- Auto-connect config -->
        <details class="advanced-section">
          <summary>Auto-Connect Configuration</summary>
          <div class="autoconfig-grid">
            <div class="form-group">
              <label class="form-label">Enabled</label>
              <label class="toggle">
                <input type="checkbox" bind:checked={autoConfig.enabled} />
                <span class="toggle-slider"></span>
              </label>
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-max-retries">Max Retries (0=infinite)</label>
              <input type="number" id="auto-max-retries" bind:value={autoConfig.max_retries} min="0" />
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-retry-interval">Retry Interval (s)</label>
              <input type="number" id="auto-retry-interval" bind:value={autoConfig.retry_interval} min="1" max="60" />
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-backoff">Backoff Factor</label>
              <input type="number" id="auto-backoff" bind:value={autoConfig.backoff_factor} min="1" max="10" step="0.5" />
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-max-interval">Max Retry Interval (s)</label>
              <input type="number" id="auto-max-interval" bind:value={autoConfig.max_retry_interval} min="10" max="3600" />
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-check">Check Interval (s)</label>
              <input type="number" id="auto-check" bind:value={autoConfig.check_interval} min="5" max="120" />
            </div>
            <div class="form-group">
              <label class="form-label" for="auto-health">Health Check Interval (s)</label>
              <input type="number" id="auto-health" bind:value={autoConfig.health_check_interval} min="10" max="600" />
            </div>
          </div>
          <button class="btn-save" on:click={saveAutoConfig}>Save Auto-Connect Config</button>
        </details>

        <!-- Connection History -->
        {#if autoStatus?.history && autoStatus.history.length > 0}
          <details class="advanced-section">
            <summary>Connection History ({autoStatus.history.length})</summary>
            <div class="history-list">
              {#each autoStatus.history.slice().reverse() as entry}
                <div class="history-entry">
                  <span class="history-time">{formatTimestamp(entry.timestamp)}</span>
                  <span class="history-type" style="color: {eventTypeColor(entry.event_type)}">{eventTypeLabel(entry.event_type)}</span>
                  <span class="history-message">{entry.message}</span>
                </div>
              {/each}
            </div>
          </details>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .pppoe-page {
    max-width: 900px;
  }

  h1 {
    color: #00ff88;
    margin-bottom: 1.5rem;
  }

  h2 {
    color: #e0e0e0;
    font-size: 1.1rem;
    margin-bottom: 0.5rem;
  }

  /* ── Connected Banner ─────────────────────────────────── */
  .connected-banner {
    background: linear-gradient(135deg, #0d3320 0%, #1a1a2e 100%);
    border: 1px solid #00ff8844;
    border-radius: 0.75rem;
    padding: 1.25rem;
    margin-bottom: 1.5rem;
    animation: fadeIn 0.3s ease;
  }

  .connected-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .connected-header h2 {
    margin: 0;
    flex: 1;
    color: #00ff88;
    font-size: 1rem;
  }

  .connected-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #00ff88;
    box-shadow: 0 0 8px #00ff8866;
    animation: pulse 2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .btn-disconnect {
    background: #ff4444;
    color: white;
    border: none;
    padding: 0.4rem 1rem;
    border-radius: 0.4rem;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    flex-shrink: 0;
  }
  .btn-disconnect:hover { opacity: 0.85; }

  .connected-stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.6rem;
  }

  .connected-stat {
    background: #0f0f23;
    padding: 0.6rem 0.75rem;
    border-radius: 0.4rem;
  }

  .cs-label {
    display: block;
    font-size: 0.7rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 0.15rem;
  }

  .cs-value {
    display: block;
    font-size: 0.9rem;
    font-weight: 600;
    color: #e0e0e0;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  .cs-value.ip { color: #00ff88; }
  .cs-value.uptime { color: #4dabf7; }

  /* ── Stepper ──────────────────────────────────────────── */
  .stepper {
    display: flex;
    align-items: center;
    gap: 0;
    margin-bottom: 1.5rem;
    padding: 0.75rem 1rem;
    background: #1a1a2e;
    border-radius: 0.75rem;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background: none;
    border: none;
    color: #666;
    font-size: 0.85rem;
    cursor: pointer;
    padding: 0.4rem 0.75rem;
    border-radius: 0.4rem;
    transition: color 0.2s, background 0.2s;
    white-space: nowrap;
  }

  .step:hover { color: #aaa; }

  .step.active {
    color: #00ff88;
    background: #0d3320;
  }

  .step.completed {
    color: #00ff88;
  }

  .step-num {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 1.5px solid #444;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    font-weight: 700;
    flex-shrink: 0;
  }

  .step.active .step-num {
    border-color: #00ff88;
    background: #00ff88;
    color: #0f0f23;
  }

  .step.completed .step-num {
    border-color: #00ff88;
    background: #00ff8822;
    color: #00ff88;
  }

  .step-line {
    flex: 1;
    height: 2px;
    background: #333;
    margin: 0 0.25rem;
    border-radius: 1px;
    transition: background 0.3s;
  }

  .step-line.filled {
    background: #00ff88;
  }

  .step-label {
    font-weight: 500;
  }

  /* ── Wizard Card ──────────────────────────────────────── */
  .wizard-card {
    background: #1a1a2e;
    border-radius: 0.75rem;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .step-content {
    min-height: 300px;
  }

  .step-desc {
    color: #888;
    font-size: 0.9rem;
    margin-bottom: 1.25rem;
  }

  /* ── Wizard Navigation ────────────────────────────────── */
  .wizard-nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid #333;
  }

  .btn-back {
    background: #16213e;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.6rem 1.25rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
  }
  .btn-back:hover { border-color: #00ff88; color: #00ff88; }

  .btn-next {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.6rem 1.5rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .btn-next:hover { opacity: 0.9; }
  .btn-next:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ── Interface Grid (Step 1) ──────────────────────────── */
  .iface-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 0.75rem;
  }

  .iface-card {
    background: #0f0f23;
    border: 1.5px solid #333;
    border-radius: 0.6rem;
    padding: 1rem;
    cursor: pointer;
    text-align: left;
    color: #e0e0e0;
    transition: border-color 0.2s, background 0.2s;
  }
  .iface-card:hover { border-color: #555; background: #16213e44; }
  .iface-card.selected { border-color: #00ff88; background: #0d3320; }
  .iface-card.iface-up { border-left: 3px solid #00ff88; }

  .iface-card-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .iface-card-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .iface-card-dot.up { background: #00ff88; box-shadow: 0 0 6px #00ff8844; }
  .iface-card-dot.down { background: #ff4444; }

  .iface-card-name {
    font-weight: 600;
    font-size: 0.95rem;
    flex: 1;
  }

  .iface-card-check {
    color: #00ff88;
    font-weight: 700;
    font-size: 1rem;
  }

  .iface-card-details {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.75rem;
    margin-bottom: 0.6rem;
  }

  .iface-card-state {
    font-weight: 700;
    text-transform: uppercase;
    padding: 0.1rem 0.35rem;
    border-radius: 0.2rem;
    font-size: 0.65rem;
  }
  .iface-card-state.state-up { background: #0d3320; color: #00ff88; }
  .iface-card-state.state-down { background: #331010; color: #ff6666; }

  .iface-card-meta { color: #666; }
  .iface-card-mac { color: #555; font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.7rem; }

  .btn-toggle-iface {
    width: 100%;
    background: #16213e;
    color: #aaa;
    border: 1px solid #333;
    padding: 0.35rem;
    border-radius: 0.3rem;
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: border-color 0.15s, color 0.15s;
  }
  .btn-toggle-iface:hover { border-color: #00ff88; color: #00ff88; }

  .empty-hint {
    color: #666;
    font-size: 0.9rem;
    text-align: center;
    padding: 2rem 1rem;
  }
  .empty-hint a { color: #00ff88; }

  /* ── ISP Presets (Step 2) ─────────────────────────────── */
  .isp-section { margin-bottom: 1.25rem; }

  .isp-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.6rem;
  }

  .isp-btn {
    background: #0f0f23;
    border: 1.5px solid #333;
    border-radius: 0.5rem;
    padding: 0.6rem 0.5rem;
    cursor: pointer;
    text-align: center;
    color: #aaa;
    transition: border-color 0.2s, background 0.2s, color 0.2s;
  }
  .isp-btn:hover { border-color: #555; color: #e0e0e0; }
  .isp-btn.selected { border-color: #4dabf7; background: #0d1a2e; color: #4dabf7; }

  .isp-icon {
    display: block;
    font-size: 1.3rem;
    margin-bottom: 0.25rem;
  }

  .isp-name {
    font-size: 0.75rem;
    font-weight: 600;
  }

  /* ── Password toggle ──────────────────────────────────── */
  .password-field {
    position: relative;
    display: flex;
    align-items: center;
  }

  .password-field input {
    flex: 1;
    padding-right: 3rem;
  }

  .btn-toggle-pw {
    position: absolute;
    right: 0.5rem;
    background: none;
    border: none;
    color: #888;
    font-size: 1rem;
    cursor: pointer;
    padding: 0.3rem;
  }
  .btn-toggle-pw:hover { color: #e0e0e0; }

  /* ── Advanced sections (collapsible) ──────────────────── */
  .advanced-section {
    margin-top: 1rem;
    border: 1px solid #333;
    border-radius: 0.5rem;
    overflow: hidden;
  }

  .advanced-section summary {
    padding: 0.65rem 1rem;
    background: #16213e;
    color: #aaa;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
    user-select: none;
  }
  .advanced-section summary:hover { color: #e0e0e0; }

  .advanced-section[open] summary {
    border-bottom: 1px solid #333;
  }

  .advanced-section > :not(summary) {
    padding: 0.75rem 1rem;
  }

  /* ── DNS Options (Step 3) ─────────────────────────────── */
  .dns-options {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.6rem;
  }

  .dns-option {
    background: #0f0f23;
    border: 1.5px solid #333;
    border-radius: 0.5rem;
    padding: 0.75rem;
    cursor: pointer;
    text-align: left;
    color: #aaa;
    transition: border-color 0.2s, background 0.2s;
  }
  .dns-option:hover { border-color: #555; }
  .dns-option.selected { border-color: #4dabf7; background: #0d1a2e; }

  .dns-opt-title {
    display: block;
    font-size: 0.85rem;
    font-weight: 600;
    color: #e0e0e0;
    margin-bottom: 0.2rem;
  }

  .dns-option.selected .dns-opt-title { color: #4dabf7; }

  .dns-opt-desc {
    display: block;
    font-size: 0.72rem;
    color: #666;
  }

  .custom-dns-input {
    margin-top: 0.6rem;
    width: 100%;
  }

  /* ── Review Grid (Step 4) ─────────────────────────────── */
  .review-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
    margin-bottom: 1.25rem;
  }

  .review-item {
    background: #0f0f23;
    padding: 0.65rem 0.85rem;
    border-radius: 0.4rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .review-label {
    font-size: 0.8rem;
    color: #888;
  }

  .review-value {
    font-size: 0.9rem;
    font-weight: 600;
    color: #e0e0e0;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  .connect-actions {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
  }

  .btn-connect {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.85rem 3rem;
    border-radius: 0.5rem;
    font-size: 1.05rem;
    font-weight: 700;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    transition: opacity 0.2s;
  }
  .btn-connect:hover { opacity: 0.9; }
  .btn-connect:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-spinner {
    width: 16px;
    height: 16px;
    border: 2.5px solid #0f0f2333;
    border-top-color: #0f0f23;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    display: inline-block;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* ── Alerts ───────────────────────────────────────────── */
  .alert {
    width: 100%;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    text-align: center;
  }

  .alert-error {
    background: #331010;
    color: #ff6666;
    border: 1px solid #ff444444;
  }

  .alert-success {
    background: #0d3320;
    color: #00ff88;
    border: 1px solid #00ff8844;
  }

  /* ── Forms ────────────────────────────────────────────── */
  .form-group {
    margin-bottom: 1rem;
  }

  .form-label {
    display: block;
    font-size: 0.8rem;
    color: #888;
    margin-bottom: 0.35rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .form-row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  input, select {
    width: 100%;
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.65rem 0.85rem;
    border-radius: 0.4rem;
    font-size: 0.9rem;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  .input-error {
    border-color: #ff4444;
  }

  .field-error {
    display: block;
    font-size: 0.75rem;
    color: #ff6666;
    margin-top: 0.3rem;
  }

  .field-hint {
    display: block;
    font-size: 0.75rem;
    color: #555;
    margin-top: 0.3rem;
  }

  /* ── Toggle Switch ────────────────────────────────────── */
  .toggle-group {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .toggle-label {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    cursor: pointer;
    font-size: 0.9rem;
    color: #e0e0e0;
  }

  .toggle-label input { display: none; }

  .toggle-track {
    width: 38px;
    height: 20px;
    background: #333;
    border-radius: 10px;
    position: relative;
    transition: background 0.2s;
    flex-shrink: 0;
  }

  .toggle-thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: #888;
    border-radius: 50%;
    transition: transform 0.2s, background 0.2s;
  }

  .toggle-label input:checked + .toggle-track { background: #00ff8844; }
  .toggle-label input:checked + .toggle-track .toggle-thumb {
    transform: translateX(18px);
    background: #00ff88;
  }

  /* ── Loading ──────────────────────────────────────────── */
  .loading-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: #888;
    font-size: 0.9rem;
    padding: 1rem 0;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid #333;
    border-top-color: #00ff88;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .muted { color: #666; font-size: 0.9rem; }

  /* ── Auto-Connect Card ────────────────────────────────── */
  .autoconnect-card {
    background: #1a1a2e;
    border-radius: 0.75rem;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .autoconnect-card h2 {
    color: #e0e0e0;
    font-size: 1.1rem;
    margin-bottom: 1rem;
  }

  .autoconnect-controls {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .autoconnect-toggle {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem;
    background: #0f0f23;
    border-radius: 0.5rem;
  }

  .status-indicator {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }

  .autoconnect-toggle span {
    flex: 1;
  }

  .btn-start, .btn-stop, .btn-save {
    padding: 0.4rem 1.25rem;
    border: none;
    border-radius: 0.4rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-start { background: #00ff88; color: #0f0f23; }
  .btn-stop { background: #ff4444; color: white; }
  .btn-save { background: #00ff88; color: #0f0f23; margin-top: 0.75rem; }
  .btn-start:hover, .btn-stop:hover, .btn-save:hover { opacity: 0.9; }

  .autoconnect-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.6rem;
  }

  .stat {
    background: #0f0f23;
    padding: 0.65rem;
    border-radius: 0.4rem;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .stat-label {
    font-size: 0.7rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .stat-value {
    font-size: 1rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  /* Auto-config grid */
  .autoconfig-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.75rem;
    padding-top: 0.5rem;
  }

  .autoconfig-grid .form-group { margin-bottom: 0; }

  /* Toggle switch for auto-connect */
  .toggle {
    position: relative;
    display: inline-block;
    width: 48px;
    height: 24px;
    cursor: pointer;
  }

  .toggle input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .toggle-slider {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    background: #333;
    border-radius: 24px;
    transition: 0.3s;
  }

  .toggle-slider:before {
    content: '';
    position: absolute;
    width: 18px; height: 18px;
    left: 3px; bottom: 3px;
    background: white;
    border-radius: 50%;
    transition: 0.3s;
  }

  .toggle input:checked + .toggle-slider { background: #00ff88; }
  .toggle input:checked + .toggle-slider:before { transform: translateX(24px); }

  /* ── Connection History ───────────────────────────────── */
  .history-list {
    display: flex;
    flex-direction: column;
    gap: 0;
    max-height: 250px;
    overflow-y: auto;
  }

  .history-entry {
    display: grid;
    grid-template-columns: 150px 100px 1fr;
    gap: 0.75rem;
    padding: 0.45rem 0;
    border-bottom: 1px solid #1a1a2e;
    font-size: 0.8rem;
  }

  .history-time {
    color: #888;
    font-family: 'SF Mono', 'Fira Code', monospace;
  }

  .history-type { font-weight: 600; }
  .history-message { color: #aaa; }

  /* ── Animations ───────────────────────────────────────── */
  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 700px) {
    .stepper { flex-wrap: wrap; gap: 0.25rem; }
    .step-line { display: none; }
    .connected-stats { grid-template-columns: 1fr 1fr; }
    .isp-grid { grid-template-columns: repeat(2, 1fr); }
    .dns-options { grid-template-columns: 1fr; }
    .form-row-2 { grid-template-columns: 1fr; }
    .review-grid { grid-template-columns: 1fr; }
    .autoconnect-stats { grid-template-columns: repeat(2, 1fr); }
    .autoconfig-grid { grid-template-columns: 1fr; }
  }
</style>
