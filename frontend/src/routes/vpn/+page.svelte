<script lang="ts">
  import { onMount } from 'svelte';

  let vpnStatus: any = null;
  let connections: any[] = [];
  let loading = true;
  let error = '';
  let success = '';

  // Active tab
  let activeTab: 'wireguard' | 'ipsec' | 'openvpn' = 'wireguard';

  // WireGuard form
  let wgForm = {
    name: 'wg0',
    listen_port: 51820,
    address: '',
    peer_endpoint: '',
    peer_public_key: '',
    peer_allowed_ips: '0.0.0.0/0',
    dns: '',
    mtu: 1420
  };

  // IPsec form
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

  // OpenVPN form
  let ovpnForm = {
    name: 'ovpn0',
    mode: 'client',
    remote: '',
    port: 1194,
    proto: 'udp',
    ca_cert: '',
    client_cert: '',
    client_key: '',
    cipher: 'AES-256-GCM',
    auth: 'SHA256',
    redirect_gateway: false,
    dns_push: ''
  };

  onMount(async () => {
    await fetchVpnStatus();
    await fetchConnections();
  });

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
        await fetchVpnStatus();
        await fetchConnections();
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
        await fetchVpnStatus();
        await fetchConnections();
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
        await fetchVpnStatus();
        await fetchConnections();
      }
    } catch (e) {
      error = 'Failed to configure OpenVPN';
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
        await fetchVpnStatus();
        await fetchConnections();
      }
    } catch (e) {
      error = 'Failed to bring down tunnel';
    }
  }

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
</script>

<svelte:head>
  <title>VectorOS - VPN</title>
</svelte:head>

<div class="vpn-page">
  <h1>VPN Management</h1>

  <!-- Status Card -->
  <div class="status-card">
    <h2>VPN Status</h2>
    {#if loading}
      <p>Loading...</p>
    {:else if vpnStatus}
      <div class="status-info">
        <div class="backend-grid">
          <div class="backend-item">
            <span class="label">WireGuard (Kernel):</span>
            <span class="badge" class:active={vpnStatus.backends?.wireguard_kernel}>
              {vpnStatus.backends?.wireguard_kernel ? 'Available' : 'Not Available'}
            </span>
          </div>
          <div class="backend-item">
            <span class="label">WireGuard (VPP):</span>
            <span class="badge" class:active={vpnStatus.backends?.wireguard_vpp}>
              {vpnStatus.backends?.wireguard_vpp ? 'Available' : 'Not Available'}
            </span>
          </div>
          <div class="backend-item">
            <span class="label">IPsec (Kernel):</span>
            <span class="badge" class:active={vpnStatus.backends?.ipsec_kernel}>
              {vpnStatus.backends?.ipsec_kernel ? 'Available' : 'Not Available'}
            </span>
          </div>
          <div class="backend-item">
            <span class="label">IPsec (VPP):</span>
            <span class="badge" class:active={vpnStatus.backends?.ipsec_vpp}>
              {vpnStatus.backends?.ipsec_vpp ? 'Available' : 'Not Available'}
            </span>
          </div>
          <div class="backend-item">
            <span class="label">OpenVPN:</span>
            <span class="badge" class:active={vpnStatus.backends?.openvpn}>
              {vpnStatus.backends?.openvpn ? 'Available' : 'Not Available'}
            </span>
          </div>
        </div>
      </div>
      <button class="btn-secondary" on:click={() => { fetchVpnStatus(); fetchConnections(); }}>Refresh</button>
    {/if}
  </div>

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  {#if success}
    <div class="success-card">{success}</div>
  {/if}

  <!-- Active Connections -->
  {#if connections.length > 0}
    <div class="connections-card">
      <h2>Active Connections ({connections.length})</h2>
      <div class="connections-table">
        <div class="conn-header">
          <span class="col-type">Type</span>
          <span class="col-name">Name</span>
          <span class="col-status">Status</span>
          <span class="col-remote">Remote</span>
          <span class="col-traffic">Traffic</span>
          <span class="col-actions">Actions</span>
        </div>
        {#each connections as conn}
          <div class="conn-row">
            <span class="col-type">
              <span class="type-badge" class:wireguard={conn.type === 'wireguard'} class:ipsec={conn.type === 'ipsec'} class:openvpn={conn.type === 'openvpn'}>
                {conn.type.toUpperCase()}
              </span>
            </span>
            <span class="col-name">{conn.name || conn.spi || '-'}</span>
            <span class="col-status">
              <span class="status-dot" class:active={conn.status === 'active' || conn.status === 'connected'}>
              </span>
              {conn.status}
            </span>
            <span class="col-remote">{conn.remote || '-'}</span>
            <span class="col-traffic">
              {#if conn.bytes_in}
                IN: {formatBytes(conn.bytes_in)} / OUT: {formatBytes(conn.bytes_out)}
              {:else}
                -
              {/if}
            </span>
            <span class="col-actions">
              <button class="btn-down" on:click={() => bringDown(conn.type, conn.name || '')}>Down</button>
            </span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- VPN Type Tabs -->
  <div class="tabs">
    <button class="tab" class:active={activeTab === 'wireguard'} on:click={() => activeTab = 'wireguard'}>
      WireGuard
    </button>
    <button class="tab" class:active={activeTab === 'ipsec'} on:click={() => activeTab = 'ipsec'}>
      IPsec
    </button>
    <button class="tab" class:active={activeTab === 'openvpn'} on:click={() => activeTab = 'openvpn'}>
      OpenVPN
    </button>
  </div>

  <!-- WireGuard Config -->
  {#if activeTab === 'wireguard'}
    <div class="config-card">
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
            <label for="wg-address">Interface Address (CIDR)</label>
            <input type="text" id="wg-address" bind:value={wgForm.address} placeholder="10.0.0.1/24" required />
          </div>
          <div class="form-group">
            <label for="wg-mtu">MTU</label>
            <input type="number" id="wg-mtu" bind:value={wgForm.mtu} min="1280" max="9000" />
          </div>
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

  <!-- IPsec Config -->
  {#if activeTab === 'ipsec'}
    <div class="config-card">
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
            <label for="ipsec-local-ip">Local IP</label>
            <input type="text" id="ipsec-local-ip" bind:value={ipsecForm.local_ip} placeholder="192.168.1.1" required />
          </div>
          <div class="form-group">
            <label for="ipsec-remote-ip">Remote IP</label>
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
            <label for="ipsec-psk">Pre-Shared Key</label>
            <input type="password" id="ipsec-psk" bind:value={ipsecForm.pre_shared_key} placeholder="Pre-shared key" />
          </div>
          <div class="form-group">
            <label for="ipsec-dh">DH Group</label>
            <select id="ipsec-dh" bind:value={ipsecForm.dh_group}>
              <option value="2">DH Group 2 (1024-bit)</option>
              <option value="14">DH Group 14 (2048-bit)</option>
              <option value="19">DH Group 19 (256-bit ECP)</option>
              <option value="20">DH Group 20 (384-bit ECP)</option>
            </select>
          </div>
        </div>

        <button type="submit" class="btn-primary">Configure IPsec</button>
      </form>
    </div>
  {/if}

  <!-- OpenVPN Config -->
  {#if activeTab === 'openvpn'}
    <div class="config-card">
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
            <label for="ovpn-remote">Remote Server</label>
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
            <label for="ovpn-dns">DNS Servers</label>
            <input type="text" id="ovpn-dns" bind:value={ovpnForm.dns_push} placeholder="1.1.1.1, 8.8.8.8" />
          </div>
          <div class="form-group checkbox-group">
            <label>
              <input type="checkbox" bind:checked={ovpnForm.redirect_gateway} />
              Redirect Gateway (Route all traffic through VPN)
            </label>
          </div>
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

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
  }

  h3 {
    margin: 1.5rem 0 0.75rem;
    color: #aaa;
    font-size: 0.95rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .backend-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .backend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
  }

  .backend-item .label {
    color: #888;
  }

  .badge {
    padding: 0.2rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    font-weight: 600;
    background: #331111;
    color: #ff4444;
  }

  .badge.active {
    background: #003322;
    color: #00ff88;
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
    margin-top: 1rem;
  }

  .btn-secondary:hover { opacity: 0.9; }

  .btn-down {
    background: none;
    border: 1px solid #ff8844;
    color: #ff8844;
    padding: 0.3rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .btn-down:hover {
    background: #ff8844;
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

  .connections-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .connections-table {
    font-size: 0.9rem;
    overflow-x: auto;
  }

  .conn-header, .conn-row {
    display: grid;
    grid-template-columns: 90px 100px 100px 1fr 150px 70px;
    gap: 0.5rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid #333;
    align-items: center;
  }

  .conn-header {
    font-weight: bold;
    color: #888;
    border-bottom: 1px solid #555;
  }

  .conn-row {
    color: #e0e0e0;
  }

  .type-badge {
    padding: 0.15rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.7rem;
    font-weight: bold;
  }

  .type-badge.wireguard {
    background: #1a2e3e;
    color: #44aaff;
  }

  .type-badge.ipsec {
    background: #2e2e1a;
    color: #ffaa44;
  }

  .type-badge.openvpn {
    background: #2e1a2e;
    color: #ff44aa;
  }

  .status-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #ff4444;
    margin-right: 0.3rem;
  }

  .status-dot.active {
    background: #00ff88;
  }

  .tabs {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  .tab {
    background: #1a1a2e;
    color: #888;
    border: 1px solid #333;
    padding: 0.6rem 1.5rem;
    border-radius: 0.5rem;
    cursor: pointer;
    font-weight: 600;
    transition: all 0.2s;
  }

  .tab.active {
    background: #16213e;
    color: #00ff88;
    border-color: #00ff88;
  }

  .tab:hover {
    color: #e0e0e0;
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

  .checkbox-group {
    justify-content: flex-end;
  }

  .checkbox-group label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    color: #e0e0e0;
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
</style>
