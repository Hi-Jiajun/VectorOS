<script lang="ts">
  import { onMount } from 'svelte';

  // ── State ───────────────────────────────────────────────────────
  let activeTab: 'wireguard' | 'ipsec' | 'openvpn' = 'wireguard';
  let vpnStatus: any = null;
  let connections: any[] = [];
  let loading = true;
  let error = '';
  let success = '';

  // Per-VPN-type stats
  let wireguardStats: any = null;
  let ipsecStats: any = null;
  let openvpnStats: any = null;

  // ── WireGuard form ──────────────────────────────────────────────
  let wgForm = {
    name: 'wg0',
    private_key: '',
    listen_port: 51820,
    address: '',
    peer_endpoint: '',
    peer_public_key: '',
    peer_allowed_ips: '0.0.0.0/0',
    dns: '',
    mtu: 1420
  };

  // ── IPsec form ──────────────────────────────────────────────────
  let ipsecForm = {
    name: 'ipsec0',
    mode: 'tunnel',
    proto: 'esp',
    local_ip: '',
    remote_ip: '',
    local_subnet: '',
    remote_subnet: '',
    local_id: '',
    remote_id: '',
    encryption: 'aes-256-gcm',
    integrity: 'sha256',
    dh_group: '2',
    pre_shared_key: ''
  };

  // ── OpenVPN form ────────────────────────────────────────────────
  let ovpnForm = {
    name: 'ovpn0',
    mode: 'client',
    remote: '',
    port: 1194,
    proto: 'udp',
    config_file: '',
    ca_cert: '',
    client_cert: '',
    client_key: '',
    cipher: 'AES-256-GCM',
    auth: 'SHA256',
    redirect_gateway: false,
    dns_push: ''
  };

  // ── Lifecycle ──────────────────────────────────────────────────
  onMount(async () => {
    await Promise.all([fetchVpnStatus(), fetchConnections()]);
    await fetchAllStats();
  });

  // ── Data fetching ──────────────────────────────────────────────
  async function fetchVpnStatus() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/vpn/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        vpnStatus = data;
      }
    } catch (e) {
      error = 'Failed to fetch VPN status';
    } finally {
      loading = false;
    }
  }

  async function fetchConnections() {
    try {
      const res = await fetch('/api/vpn/connections');
      const data = await res.json();
      if (!data.error) {
        connections = data.connections || [];
      }
    } catch (e) {
      // Ignore connection fetch errors
    }
  }

  async function fetchAllStats() {
    await Promise.all([fetchWireGuardStats(), fetchIpsecStats(), fetchOpenVpnStats()]);
  }

  async function fetchWireGuardStats() {
    try {
      const res = await fetch('/api/vpn/wireguard/stats');
      const data = await res.json();
      if (!data.error) wireguardStats = data;
    } catch (e) { /* ignore */ }
  }

  async function fetchIpsecStats() {
    try {
      const res = await fetch('/api/vpn/ipsec/stats');
      const data = await res.json();
      if (!data.error) ipsecStats = data;
    } catch (e) { /* ignore */ }
  }

  async function fetchOpenVpnStats() {
    try {
      const res = await fetch('/api/vpn/openvpn/stats');
      const data = await res.json();
      if (!data.error) openvpnStats = data;
    } catch (e) { /* ignore */ }
  }

  // ── Configure actions ──────────────────────────────────────────
  async function configureWireGuard() {
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/wireguard/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(wgForm)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'WireGuard configured successfully';
        await refreshAll();
      }
    } catch (e) {
      error = 'Failed to configure WireGuard';
    }
  }

  async function configureIpsec() {
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/ipsec/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(ipsecForm)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'IPsec configured successfully';
        await refreshAll();
      }
    } catch (e) {
      error = 'Failed to configure IPsec';
    }
  }

  async function configureOpenVpn() {
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/openvpn/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(ovpnForm)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'OpenVPN configured successfully';
        await refreshAll();
      }
    } catch (e) {
      error = 'Failed to configure OpenVPN';
    }
  }

  // ── Start/Stop actions ─────────────────────────────────────────
  async function startVpn(vpnType: string, name: string) {
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ vpn_type: vpnType, name })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || `${vpnType} tunnel '${name}' started`;
        await refreshAll();
      }
    } catch (e) {
      error = `Failed to start ${vpnType} tunnel`;
    }
  }

  async function stopVpn(vpnType: string, name: string) {
    if (!confirm(`Stop ${vpnType} tunnel '${name}'?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/stop', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ vpn_type: vpnType, name })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || `${vpnType} tunnel '${name}' stopped`;
        await refreshAll();
      }
    } catch (e) {
      error = `Failed to stop ${vpnType} tunnel`;
    }
  }

  async function bringDown(vpnType: string, name: string) {
    if (!confirm(`Bring down ${vpnType} tunnel '${name}'?`)) return;
    try {
      error = '';
      success = '';
      const res = await fetch('/api/vpn/down', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ vpn_type: vpnType, name })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = data.message || 'Tunnel brought down';
        await refreshAll();
      }
    } catch (e) {
      error = 'Failed to bring down tunnel';
    }
  }

  async function refreshAll() {
    await Promise.all([
      fetchVpnStatus(),
      fetchConnections(),
      fetchAllStats()
    ]);
  }

  // ── Helpers ────────────────────────────────────────────────────
  function formatBytes(bytes: string | number): string {
    const b = typeof bytes === 'string' ? parseInt(bytes) : bytes;
    if (isNaN(b)) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let size = b;
    while (size >= 1024 && i < units.length - 1) {
      size /= 1024;
      i++;
    }
    return `${size.toFixed(1)} ${units[i]}`;
  }

  function getConnectionForType(type: string): any | null {
    return connections.find(c => c.type === type) || null;
  }

  function isRunning(type: string): boolean {
    const conn = getConnectionForType(type);
    return conn && (conn.status === 'active' || conn.status === 'connected');
  }

  function clearMessages() {
    error = '';
    success = '';
  }
</script>

<svelte:head>
  <title>VectorOS - VPN</title>
</svelte:head>

<div class="vpn-page">
  <!-- Header -->
  <div class="header-row">
    <h1>VPN Management</h1>
    <div class="header-actions">
      <button class="btn-refresh" on:click={refreshAll} disabled={loading}>
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

  <!-- Status Card -->
  <div class="status-card">
    <h2>VPN Backend Status</h2>
    {#if loading}
      <p class="loading-text">Loading...</p>
    {:else if vpnStatus}
      <div class="backend-grid">
        <div class="backend-item">
          <span class="backend-label">WireGuard (Kernel):</span>
          <span class="badge" class:badge-active={vpnStatus.backends?.wireguard_kernel}>
            {vpnStatus.backends?.wireguard_kernel ? 'Available' : 'Not Available'}
          </span>
        </div>
        <div class="backend-item">
          <span class="backend-label">WireGuard (VPP):</span>
          <span class="badge" class:badge-active={vpnStatus.backends?.wireguard_vpp}>
            {vpnStatus.backends?.wireguard_vpp ? 'Available' : 'Not Available'}
          </span>
        </div>
        <div class="backend-item">
          <span class="backend-label">IPsec (Kernel):</span>
          <span class="badge" class:badge-active={vpnStatus.backends?.ipsec_kernel}>
            {vpnStatus.backends?.ipsec_kernel ? 'Available' : 'Not Available'}
          </span>
        </div>
        <div class="backend-item">
          <span class="backend-label">IPsec (VPP):</span>
          <span class="badge" class:badge-active={vpnStatus.backends?.ipsec_vpp}>
            {vpnStatus.backends?.ipsec_vpp ? 'Available' : 'Not Available'}
          </span>
        </div>
        <div class="backend-item">
          <span class="backend-label">OpenVPN:</span>
          <span class="badge" class:badge-active={vpnStatus.backends?.openvpn}>
            {vpnStatus.backends?.openvpn ? 'Available' : 'Not Available'}
          </span>
        </div>
      </div>
    {/if}
  </div>

  <!-- Active Connections -->
  {#if connections.length > 0}
    <div class="connections-card">
      <h2>Active Connections ({connections.length})</h2>
      <div class="table-wrapper">
        <table>
          <thead>
            <tr>
              <th>Type</th>
              <th>Name</th>
              <th>Status</th>
              <th>Remote</th>
              <th>Traffic</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each connections as conn}
              <tr>
                <td>
                  <span class="type-badge" class:type-wireguard={conn.type === 'wireguard'} class:type-ipsec={conn.type === 'ipsec'} class:type-openvpn={conn.type === 'openvpn'}>
                    {conn.type.toUpperCase()}
                  </span>
                </td>
                <td class="name-cell">{conn.name || conn.spi || '-'}</td>
                <td>
                  <span class="status-indicator" style="background: {(conn.status === 'active' || conn.status === 'connected') ? 'var(--color-success)' : 'var(--color-danger)'}"></span>
                  {conn.status}
                </td>
                <td class="mono-text">{conn.remote || '-'}</td>
                <td>
                  {#if conn.bytes_in}
                    <span class="traffic-in">IN: {formatBytes(conn.bytes_in)}</span>
                    <span class="traffic-out">OUT: {formatBytes(conn.bytes_out)}</span>
                  {:else}
                    -
                  {/if}
                </td>
                <td class="actions-cell">
                  <button class="btn-stop-sm" on:click={() => stopVpn(conn.type, conn.name || '')}>Stop</button>
                  <button class="btn-down-sm" on:click={() => bringDown(conn.type, conn.name || '')}>Down</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  <!-- Tabs -->
  <div class="tabs">
    <button class="tab" class:tab-active={activeTab === 'wireguard'} on:click={() => activeTab = 'wireguard'}>
      WireGuard
      {#if isRunning('wireguard')}
        <span class="tab-indicator"></span>
      {/if}
    </button>
    <button class="tab" class:tab-active={activeTab === 'ipsec'} on:click={() => activeTab = 'ipsec'}>
      IPsec
      {#if isRunning('ipsec')}
        <span class="tab-indicator"></span>
      {/if}
    </button>
    <button class="tab" class:tab-active={activeTab === 'openvpn'} on:click={() => activeTab = 'openvpn'}>
      OpenVPN
      {#if isRunning('openvpn')}
        <span class="tab-indicator"></span>
      {/if}
    </button>
  </div>

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: WireGuard -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'wireguard'}
    <!-- Status & Controls -->
    <div class="vpn-type-card">
      <div class="vpn-type-header">
        <h2>WireGuard Status</h2>
        <div class="vpn-controls">
          {#if isRunning('wireguard')}
            <span class="status-badge status-active">Connected</span>
            <button class="btn-stop" on:click={() => stopVpn('wireguard', wgForm.name)}>Stop</button>
          {:else}
            <span class="status-badge status-inactive">Disconnected</span>
            <button class="btn-start" on:click={() => startVpn('wireguard', wgForm.name)}>Start</button>
          {/if}
        </div>
      </div>

      {#if wireguardStats}
        <div class="stats-grid">
          <div class="stat-card">
            <span class="stat-label">Interface</span>
            <span class="stat-value">{wireguardStats.interface || wgForm.name}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Listen Port</span>
            <span class="stat-value">{wireguardStats.listen_port || wgForm.listen_port}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Peers</span>
            <span class="stat-value">{wireguardStats.peers || 0}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Transfer In</span>
            <span class="stat-value text-info">{formatBytes(wireguardStats.bytes_in || 0)}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Transfer Out</span>
            <span class="stat-value text-warning">{formatBytes(wireguardStats.bytes_out || 0)}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Last Handshake</span>
            <span class="stat-value text-sm">{wireguardStats.last_handshake || 'Never'}</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- Configuration Form -->
    <div class="vpn-type-card">
      <h2>WireGuard Configuration</h2>
      <form on:submit|preventDefault={configureWireGuard}>
        <div class="form-row">
          <div class="form-group">
            <label for="wg-name">Interface Name</label>
            <input type="text" id="wg-name" bind:value={wgForm.name} placeholder="wg0" />
          </div>
          <div class="form-group">
            <label for="wg-port">Listen Port</label>
            <input type="number" id="wg-port" bind:value={wgForm.listen_port} min="1" max="65535" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="wg-privkey">Private Key</label>
            <input type="password" id="wg-privkey" bind:value={wgForm.private_key} placeholder="Base64 private key" />
          </div>
          <div class="form-group">
            <label for="wg-mtu">MTU</label>
            <input type="number" id="wg-mtu" bind:value={wgForm.mtu} min="1280" max="9000" />
          </div>
        </div>

        <div class="form-group">
          <label for="wg-address">Interface Address (CIDR)</label>
          <input type="text" id="wg-address" bind:value={wgForm.address} placeholder="10.0.0.1/24" required />
        </div>

        <h3>Peer Configuration</h3>

        <div class="form-row">
          <div class="form-group">
            <label for="wg-peer-pk">Peer Public Key</label>
            <input type="text" id="wg-peer-pk" bind:value={wgForm.peer_public_key} placeholder="Base64 public key" />
          </div>
          <div class="form-group">
            <label for="wg-peer-ep">Peer Endpoint</label>
            <input type="text" id="wg-peer-ep" bind:value={wgForm.peer_endpoint} placeholder="1.2.3.4:51820" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="wg-allowed">Allowed IPs</label>
            <input type="text" id="wg-allowed" bind:value={wgForm.peer_allowed_ips} placeholder="0.0.0.0/0" />
          </div>
          <div class="form-group">
            <label for="wg-dns">DNS</label>
            <input type="text" id="wg-dns" bind:value={wgForm.dns} placeholder="1.1.1.1, 8.8.8.8" />
          </div>
        </div>

        <button type="submit" class="btn-primary">Configure WireGuard</button>
      </form>
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: IPsec -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'ipsec'}
    <!-- Status & Controls -->
    <div class="vpn-type-card">
      <div class="vpn-type-header">
        <h2>IPsec Status</h2>
        <div class="vpn-controls">
          {#if isRunning('ipsec')}
            <span class="status-badge status-active">Connected</span>
            <button class="btn-stop" on:click={() => stopVpn('ipsec', ipsecForm.name)}>Stop</button>
          {:else}
            <span class="status-badge status-inactive">Disconnected</span>
            <button class="btn-start" on:click={() => startVpn('ipsec', ipsecForm.name)}>Start</button>
          {/if}
        </div>
      </div>

      {#if ipsecStats}
        <div class="stats-grid">
          <div class="stat-card">
            <span class="stat-label">Tunnel</span>
            <span class="stat-value">{ipsecStats.name || ipsecForm.name}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">SPI</span>
            <span class="stat-value font-mono">{ipsecStats.spi || '-'}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Protocol</span>
            <span class="stat-value">{ipsecStats.proto || ipsecForm.proto}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Encryption</span>
            <span class="stat-value text-sm">{ipsecStats.encryption || ipsecForm.encryption}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Packets In</span>
            <span class="stat-value text-info">{ipsecStats.packets_in || 0}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Packets Out</span>
            <span class="stat-value text-warning">{ipsecStats.packets_out || 0}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Bytes In</span>
            <span class="stat-value text-info">{formatBytes(ipsecStats.bytes_in || 0)}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Bytes Out</span>
            <span class="stat-value text-warning">{formatBytes(ipsecStats.bytes_out || 0)}</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- Configuration Form -->
    <div class="vpn-type-card">
      <h2>IPsec Configuration</h2>
      <form on:submit|preventDefault={configureIpsec}>
        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-name">Tunnel Name</label>
            <input type="text" id="ipsec-name" bind:value={ipsecForm.name} placeholder="ipsec0" />
          </div>
          <div class="form-group">
            <label for="ipsec-mode">Mode</label>
            <select id="ipsec-mode" bind:value={ipsecForm.mode}>
              <option value="tunnel">Tunnel Mode</option>
              <option value="transport">Transport Mode</option>
            </select>
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-proto">Protocol</label>
            <select id="ipsec-proto" bind:value={ipsecForm.proto}>
              <option value="esp">ESP (Encapsulating Security Payload)</option>
              <option value="ah">AH (Authentication Header)</option>
            </select>
          </div>
          <div class="form-group">
            <label for="ipsec-enc">Encryption Algorithm</label>
            <select id="ipsec-enc" bind:value={ipsecForm.encryption}>
              <option value="aes-256-gcm">AES-256-GCM</option>
              <option value="aes-128-gcm">AES-128-GCM</option>
              <option value="aes-256-cbc">AES-256-CBC</option>
              <option value="aes-128-cbc">AES-128-CBC</option>
              <option value="chacha20-poly1305">ChaCha20-Poly1305</option>
            </select>
          </div>
        </div>

        <h3>Endpoints</h3>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-local-ip">Local Endpoint</label>
            <input type="text" id="ipsec-local-ip" bind:value={ipsecForm.local_ip} placeholder="192.168.1.1" required />
          </div>
          <div class="form-group">
            <label for="ipsec-remote-ip">Remote Endpoint</label>
            <input type="text" id="ipsec-remote-ip" bind:value={ipsecForm.remote_ip} placeholder="10.0.0.1" required />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-local-subnet">Local Subnet</label>
            <input type="text" id="ipsec-local-subnet" bind:value={ipsecForm.local_subnet} placeholder="192.168.1.0/24" />
          </div>
          <div class="form-group">
            <label for="ipsec-remote-subnet">Remote Subnet</label>
            <input type="text" id="ipsec-remote-subnet" bind:value={ipsecForm.remote_subnet} placeholder="10.0.0.0/24" />
          </div>
        </div>

        <h3>Authentication</h3>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-psk">Pre-Shared Key</label>
            <input type="password" id="ipsec-psk" bind:value={ipsecForm.pre_shared_key} placeholder="Pre-shared key" />
          </div>
          <div class="form-group">
            <label for="ipsec-integrity">Integrity Algorithm</label>
            <select id="ipsec-integrity" bind:value={ipsecForm.integrity}>
              <option value="sha256">SHA-256</option>
              <option value="sha384">SHA-384</option>
              <option value="sha512">SHA-512</option>
              <option value="aes-128gmac">AES-128-GMAC</option>
              <option value="aes-192gmac">AES-192-GMAC</option>
              <option value="aes-256gmac">AES-256-GMAC</option>
            </select>
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-local-id">Local ID</label>
            <input type="text" id="ipsec-local-id" bind:value={ipsecForm.local_id} placeholder="Local identity" />
          </div>
          <div class="form-group">
            <label for="ipsec-remote-id">Remote ID</label>
            <input type="text" id="ipsec-remote-id" bind:value={ipsecForm.remote_id} placeholder="Remote identity" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ipsec-dh">DH Group</label>
            <select id="ipsec-dh" bind:value={ipsecForm.dh_group}>
              <option value="2">DH Group 2 (1024-bit)</option>
              <option value="14">DH Group 14 (2048-bit)</option>
              <option value="19">DH Group 19 (256-bit ECP)</option>
              <option value="20">DH Group 20 (384-bit ECP)</option>
            </select>
          </div>
          <div class="form-group"></div>
        </div>

        <button type="submit" class="btn-primary">Configure IPsec</button>
      </form>
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: OpenVPN -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'openvpn'}
    <!-- Status & Controls -->
    <div class="vpn-type-card">
      <div class="vpn-type-header">
        <h2>OpenVPN Status</h2>
        <div class="vpn-controls">
          {#if isRunning('openvpn')}
            <span class="status-badge status-active">Connected</span>
            <button class="btn-stop" on:click={() => stopVpn('openvpn', ovpnForm.name)}>Stop</button>
          {:else}
            <span class="status-badge status-inactive">Disconnected</span>
            <button class="btn-start" on:click={() => startVpn('openvpn', ovpnForm.name)}>Start</button>
          {/if}
        </div>
      </div>

      {#if openvpnStats}
        <div class="stats-grid">
          <div class="stat-card">
            <span class="stat-label">Tunnel</span>
            <span class="stat-value">{openvpnStats.name || ovpnForm.name}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Remote</span>
            <span class="stat-value font-mono">{openvpnStats.remote || ovpnForm.remote}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Connected Since</span>
            <span class="stat-value text-sm">{openvpnStats.connected_since || 'N/A'}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Bytes In</span>
            <span class="stat-value text-info">{formatBytes(openvpnStats.bytes_in || 0)}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Bytes Out</span>
            <span class="stat-value text-warning">{formatBytes(openvpnStats.bytes_out || 0)}</span>
          </div>
          <div class="stat-card">
            <span class="stat-label">Virtual IP</span>
            <span class="stat-value font-mono">{openvpnStats.virtual_ip || '-'}</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- Configuration Form -->
    <div class="vpn-type-card">
      <h2>OpenVPN Configuration</h2>
      <form on:submit|preventDefault={configureOpenVpn}>
        <div class="form-row">
          <div class="form-group">
            <label for="ovpn-name">Tunnel Name</label>
            <input type="text" id="ovpn-name" bind:value={ovpnForm.name} placeholder="ovpn0" />
          </div>
          <div class="form-group">
            <label for="ovpn-mode">Mode</label>
            <select id="ovpn-mode" bind:value={ovpnForm.mode}>
              <option value="client">Client</option>
              <option value="server">Server</option>
            </select>
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ovpn-remote">Server Address</label>
            <input type="text" id="ovpn-remote" bind:value={ovpnForm.remote} placeholder="vpn.example.com" required />
          </div>
          <div class="form-group">
            <label for="ovpn-port">Port</label>
            <input type="number" id="ovpn-port" bind:value={ovpnForm.port} min="1" max="65535" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ovpn-proto">Protocol</label>
            <select id="ovpn-proto" bind:value={ovpnForm.proto}>
              <option value="udp">UDP</option>
              <option value="tcp">TCP</option>
            </select>
          </div>
          <div class="form-group">
            <label for="ovpn-cipher">Cipher</label>
            <select id="ovpn-cipher" bind:value={ovpnForm.cipher}>
              <option value="AES-256-GCM">AES-256-GCM</option>
              <option value="AES-128-GCM">AES-128-GCM</option>
              <option value="AES-256-CBC">AES-256-CBC</option>
              <option value="CHACHA20-POLY1305">ChaCha20-Poly1305</option>
            </select>
          </div>
        </div>

        <h3>Configuration File</h3>

        <div class="form-group">
          <label for="ovpn-config">Upload Configuration File (.ovpn)</label>
          <input type="file" id="ovpn-config" accept=".ovpn,.conf" on:change={(e) => {
            const file = (e.target as HTMLInputElement).files?.[0];
            if (file) {
              const reader = new FileReader();
              reader.onload = (ev) => {
                ovpnForm.config_file = ev.target?.result as string;
              };
              reader.readAsText(file);
            }
          }} />
          <span class="form-hint">Or paste the configuration content below</span>
        </div>

        <div class="form-group">
          <label for="ovpn-config-content">Configuration Content</label>
          <textarea id="ovpn-config-content" bind:value={ovpnForm.config_file} rows="6" placeholder="client
dev tun
proto udp
remote vpn.example.com 1194
resolv-retry infinite
nobind
persist-key
persist-tun
..."></textarea>
        </div>

        <h3>Certificates</h3>

        <div class="form-group">
          <label for="ovpn-ca">CA Certificate Path</label>
          <input type="text" id="ovpn-ca" bind:value={ovpnForm.ca_cert} placeholder="/etc/openvpn/ca.crt" />
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="ovpn-cert">Client Certificate Path</label>
            <input type="text" id="ovpn-cert" bind:value={ovpnForm.client_cert} placeholder="/etc/openvpn/client.crt" />
          </div>
          <div class="form-group">
            <label for="ovpn-key">Client Key Path</label>
            <input type="text" id="ovpn-key" bind:value={ovpnForm.client_key} placeholder="/etc/openvpn/client.key" />
          </div>
        </div>

        <h3>Advanced</h3>

        <div class="form-row">
          <div class="form-group">
            <label for="ovpn-auth">Authentication Algorithm</label>
            <select id="ovpn-auth" bind:value={ovpnForm.auth}>
              <option value="SHA256">SHA-256</option>
              <option value="SHA384">SHA-384</option>
              <option value="SHA512">SHA-512</option>
              <option value="SHA1">SHA-1 (legacy)</option>
            </select>
          </div>
          <div class="form-group">
            <label for="ovpn-dns">DNS Servers</label>
            <input type="text" id="ovpn-dns" bind:value={ovpnForm.dns_push} placeholder="1.1.1.1, 8.8.8.8" />
          </div>
        </div>

        <div class="form-check-row">
          <label class="check-label">
            <input type="checkbox" bind:checked={ovpnForm.redirect_gateway} />
            Redirect Gateway (Route all traffic through VPN)
          </label>
        </div>

        <button type="submit" class="btn-primary">Configure OpenVPN</button>
      </form>
    </div>
  {/if}
</div>

<style>
  .vpn-page {
    max-width: 1200px;
  }

  /* Header */
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1.5rem;
  }

  h1 {
    color: var(--color-primary);
    margin: 0;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  /* Banners */
  .error-banner {
    background: var(--color-danger-bg);
    border: 1px solid var(--color-danger-border);
    color: var(--color-danger);
    padding: 0.75rem 1rem;
    border-radius: var(--radius-md);
    margin-bottom: 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .success-banner {
    background: var(--color-success-bg);
    border: 1px solid var(--color-success-border);
    color: var(--color-success);
    padding: 0.75rem 1rem;
    border-radius: var(--radius-md);
    margin-bottom: 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
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

  .btn-close:hover { opacity: 1; }

  /* Status Card */
  .status-card {
    background: var(--color-bg-card);
    padding: 1.5rem;
    border-radius: var(--radius-lg);
    margin-bottom: 1.5rem;
    border: 1px solid var(--color-border);
  }

  .backend-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
  }

  .backend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
  }

  .backend-label {
    color: var(--color-text-muted);
  }

  .badge {
    padding: 0.2rem 0.6rem;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    font-weight: 600;
    background: var(--color-danger-bg);
    color: var(--color-danger);
    border: 1px solid var(--color-danger-border);
  }

  .badge-active {
    background: var(--color-success-bg);
    color: var(--color-success);
    border-color: var(--color-success-border);
  }

  /* Connections Card */
  .connections-card {
    background: var(--color-bg-card);
    padding: 1.5rem;
    border-radius: var(--radius-lg);
    margin-bottom: 1.5rem;
    border: 1px solid var(--color-border);
  }

  .table-wrapper {
    overflow-x: auto;
  }

  .name-cell {
    font-weight: 600;
    color: var(--color-text);
  }

  .mono-text {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.85rem;
  }

  .status-indicator {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-right: 0.4rem;
    vertical-align: middle;
  }

  .traffic-in {
    display: block;
    color: var(--color-info);
    font-size: 0.85rem;
  }

  .traffic-out {
    display: block;
    color: var(--color-warning);
    font-size: 0.85rem;
  }

  .actions-cell {
    display: flex;
    gap: 0.3rem;
  }

  .btn-stop-sm {
    background: none;
    border: 1px solid var(--color-danger);
    color: var(--color-danger);
    padding: 0.25rem 0.6rem;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    cursor: pointer;
    font-weight: 500;
    transition: all var(--transition-fast);
  }

  .btn-stop-sm:hover {
    background: var(--color-danger);
    color: white;
  }

  .btn-down-sm {
    background: none;
    border: 1px solid var(--color-border-light);
    color: var(--color-text-muted);
    padding: 0.25rem 0.6rem;
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    cursor: pointer;
    font-weight: 500;
    transition: all var(--transition-fast);
  }

  .btn-down-sm:hover {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }

  .type-badge {
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-sm);
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
  }

  .type-wireguard {
    background: rgba(59, 130, 246, 0.15);
    color: #60a5fa;
    border: 1px solid rgba(59, 130, 246, 0.3);
  }

  .type-ipsec {
    background: rgba(245, 158, 11, 0.15);
    color: #fbbf24;
    border: 1px solid rgba(245, 158, 11, 0.3);
  }

  .type-openvpn {
    background: rgba(139, 92, 246, 0.15);
    color: #a78bfa;
    border: 1px solid rgba(139, 92, 246, 0.3);
  }

  /* Tabs */
  .tabs {
    display: flex;
    gap: 0;
    margin-bottom: 1.5rem;
    border-bottom: 2px solid var(--color-border);
  }

  .tab {
    background: none;
    color: var(--color-text-muted);
    border: none;
    padding: 0.75rem 1.5rem;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    cursor: pointer;
    font-weight: 500;
    font-size: 0.9rem;
    transition: all var(--transition-fast);
    border-radius: 0;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .tab:hover {
    color: var(--color-text);
    background: rgba(51, 65, 85, 0.2);
  }

  .tab-active {
    color: var(--color-primary);
    border-bottom-color: var(--color-primary);
    font-weight: 600;
  }

  .tab-indicator {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-success);
    display: inline-block;
  }

  /* VPN Type Cards */
  .vpn-type-card {
    background: var(--color-bg-card);
    padding: 1.5rem;
    border-radius: var(--radius-lg);
    margin-bottom: 1.5rem;
    border: 1px solid var(--color-border);
  }

  .vpn-type-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .vpn-type-header h2 {
    margin-bottom: 0;
  }

  .vpn-controls {
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

  .status-active {
    background: var(--color-success-bg);
    color: var(--color-success);
    border: 1px solid var(--color-success-border);
  }

  .status-inactive {
    background: rgba(100, 116, 139, 0.1);
    color: var(--color-text-muted);
    border: 1px solid rgba(100, 116, 139, 0.3);
  }

  .btn-start {
    background: var(--color-success);
    color: white;
    border: none;
    padding: 0.5rem 1.2rem;
    border-radius: var(--radius-md);
    font-weight: 600;
    cursor: pointer;
    font-size: 0.85rem;
    transition: opacity var(--transition-fast);
  }

  .btn-start:hover { opacity: 0.9; }

  .btn-stop {
    background: var(--color-danger);
    color: white;
    border: none;
    padding: 0.5rem 1.2rem;
    border-radius: var(--radius-md);
    font-weight: 600;
    cursor: pointer;
    font-size: 0.85rem;
    transition: opacity var(--transition-fast);
  }

  .btn-stop:hover { opacity: 0.9; }

  /* Stats Grid */
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 1rem;
  }

  .stat-card {
    background: var(--color-bg);
    padding: 1rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-label {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 500;
  }

  .stat-value {
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--color-text);
  }

  .stat-value.text-sm {
    font-size: 0.85rem;
  }

  .text-info { color: var(--color-info); }
  .text-warning { color: var(--color-warning); }

  .font-mono {
    font-family: 'JetBrains Mono', monospace;
    font-size: 0.9rem;
  }

  /* Forms */
  h3 {
    margin: 1.5rem 0 0.75rem;
    color: var(--color-text-secondary);
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-bottom: 0.75rem;
  }

  .form-group label {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    font-weight: 500;
  }

  .form-hint {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    margin-top: 0.25rem;
  }

  .form-check-row {
    margin: 0.75rem 0;
  }

  .check-label {
    color: var(--color-text);
    font-size: 0.9rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .check-label input {
    width: auto;
    accent-color: var(--color-primary);
  }

  textarea {
    background: var(--color-bg-input);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    padding: 0.6rem 0.75rem;
    border-radius: var(--radius-md);
    font-size: 0.85rem;
    font-family: 'JetBrains Mono', monospace;
    resize: vertical;
    line-height: 1.5;
  }

  textarea:focus {
    outline: none;
    border-color: var(--color-primary);
  }

  input[type="file"] {
    background: var(--color-bg-input);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-md);
    font-size: 0.85rem;
    width: 100%;
    cursor: pointer;
  }

  input[type="file"]::file-selector-button {
    background: var(--color-bg-hover);
    color: var(--color-text);
    border: 1px solid var(--color-border-light);
    padding: 0.4rem 0.8rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-weight: 500;
    margin-right: 0.75rem;
  }

  input[type="file"]::file-selector-button:hover {
    background: var(--color-border);
  }

  /* Buttons */
  .btn-refresh {
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
    padding: 0.5rem 1rem;
    border-radius: var(--radius-md);
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    transition: all var(--transition-fast);
  }

  .btn-refresh:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-text);
  }

  .btn-refresh:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: var(--color-primary);
    color: white;
    border: none;
    padding: 0.65rem 1.5rem;
    border-radius: var(--radius-md);
    font-weight: 600;
    cursor: pointer;
    font-size: 0.9rem;
    transition: opacity var(--transition-fast);
    margin-top: 0.5rem;
  }

  .btn-primary:hover { opacity: 0.9; }

  .loading-text {
    color: var(--color-text-muted);
    font-style: italic;
  }

  @media (max-width: 800px) {
    .form-row { grid-template-columns: 1fr; }
    .header-row { flex-direction: column; align-items: flex-start; gap: 0.75rem; }
    .tabs { flex-wrap: wrap; }
    .vpn-type-header { flex-direction: column; align-items: flex-start; gap: 0.75rem; }
    .stats-grid { grid-template-columns: 1fr 1fr; }
  }
</style>
