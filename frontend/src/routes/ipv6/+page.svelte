<script lang="ts">
  import { onMount } from 'svelte';

  let ipv6Status: any = null;
  let dhcpv6Status: any = null;
  let loading = true;
  let error = '';
  let message = '';

  // IPv6 configuration form
  let ipv6Config = {
    enabled: false,
    lanInterface: 'lan0',
    lanAddress: '2001:db8:1::1/64',
    wanInterface: 'pppoe-wan0',
    wanAddress: '',
    enableNdp: true
  };

  // DHCPv6 configuration form
  let dhcpv6Config = {
    enabled: false,
    interface: 'veth-lan0',
    rangeStart: '2001:db8:1::100',
    rangeEnd: '2001:db8:1::200',
    gateway: '2001:db8:1::1',
    leaseTime: 86400,
    enableIaNa: true,
    enableIaPd: false,
    pdPrefix: '2001:db8:2::/48'
  };

  onMount(async () => {
    await fetchAllStatus();
  });

  async function fetchAllStatus() {
    try {
      loading = true;
      error = '';
      const [ipv6Res, dhcpv6Res] = await Promise.all([
        fetch('/api/ipv6/status').then(r => r.json()),
        fetch('/api/dhcpv6/status').then(r => r.json())
      ]);
      ipv6Status = ipv6Res;
      dhcpv6Status = dhcpv6Res;
    } catch (e) {
      error = 'Failed to fetch IPv6 status';
    } finally {
      loading = false;
    }
  }

  async function saveIPv6Config() {
    try {
      message = '';
      const res = await fetch('/api/ipv6/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(ipv6Config)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        message = 'IPv6 configuration saved';
        await fetchAllStatus();
      }
    } catch (e) {
      error = 'Failed to save IPv6 configuration';
    }
  }

  async function saveDHCPv6Config() {
    try {
      message = '';
      const res = await fetch('/api/dhcpv6/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(dhcpv6Config)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        message = 'DHCPv6 configuration saved';
        await fetchAllStatus();
      }
    } catch (e) {
      error = 'Failed to save DHCPv6 configuration';
    }
  }
</script>

<svelte:head>
  <title>VectorOS - IPv6 Configuration</title>
</svelte:head>

<div class="ipv6-page">
  <h1>IPv6 Configuration</h1>

  {#if loading}
    <p>Loading...</p>
  {:else}
    {#if error}
      <div class="error-banner">{error}</div>
    {/if}
    {#if message}
      <div class="success-banner">{message}</div>
    {/if}

    <!-- IPv6 Address Configuration -->
    <div class="config-card">
      <h2>IPv6 Addresses</h2>
      <form on:submit|preventDefault={saveIPv6Config}>
        <div class="form-group">
          <label>
            <input type="checkbox" bind:checked={ipv6Config.enabled} />
            Enable IPv6
          </label>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="lan-iface">LAN Interface</label>
            <select id="lan-iface" bind:value={ipv6Config.lanInterface}>
              <option value="lan0">lan0</option>
              <option value="lan1">lan1</option>
              <option value="lan2">lan2</option>
            </select>
          </div>
          <div class="form-group">
            <label for="lan-addr">LAN IPv6 Address</label>
            <input type="text" id="lan-addr" bind:value={ipv6Config.lanAddress}
                   placeholder="2001:db8:1::1/64" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="wan-iface">WAN Interface</label>
            <select id="wan-iface" bind:value={ipv6Config.wanInterface}>
              <option value="pppoe-wan0">pppoe-wan0</option>
              <option value="wan0">wan0</option>
            </select>
          </div>
          <div class="form-group">
            <label for="wan-addr">WAN IPv6 Address (optional)</label>
            <input type="text" id="wan-addr" bind:value={ipv6Config.wanAddress}
                   placeholder="Auto from PPPoE/SLAAC" />
          </div>
        </div>

        <div class="form-group">
          <label>
            <input type="checkbox" bind:checked={ipv6Config.enableNdp} />
            Enable Neighbor Discovery (NDP)
          </label>
        </div>

        <button type="submit">Save IPv6 Settings</button>
      </form>
    </div>

    <!-- IPv6 Neighbor Table -->
    <div class="status-card">
      <h2>IPv6 Neighbor Table (NDP)</h2>
      {#if ipv6Status?.neighbors?.length > 0}
        <table>
          <thead>
            <tr>
              <th>IPv6 Address</th>
              <th>MAC Address</th>
              <th>Interface</th>
              <th>Flags</th>
            </tr>
          </thead>
          <tbody>
            {#each ipv6Status.neighbors as neighbor}
              <tr>
                <td>{neighbor.ipv6}</td>
                <td>{neighbor.mac}</td>
                <td>{neighbor.interface}</td>
                <td>{neighbor.flags}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty">No IPv6 neighbors found</p>
      {/if}
    </div>

    <!-- IPv6 Routing Table -->
    <div class="status-card">
      <h2>IPv6 Routing Table</h2>
      {#if ipv6Status?.routes?.length > 0}
        <table>
          <thead>
            <tr>
              <th>Destination</th>
              <th>Next Hop</th>
              <th>Details</th>
            </tr>
          </thead>
          <tbody>
            {#each ipv6Status.routes as route}
              <tr>
                <td>{route.destination}</td>
                <td>{route.next_hop}</td>
                <td>{route.details}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty">No IPv6 routes found</p>
      {/if}
    </div>

    <!-- DHCPv6 Configuration -->
    <div class="config-card">
      <h2>DHCPv6 Server</h2>
      <form on:submit|preventDefault={saveDHCPv6Config}>
        <div class="form-group">
          <label>
            <input type="checkbox" bind:checked={dhcpv6Config.enabled} />
            Enable DHCPv6 Server
          </label>
        </div>

        <div class="form-group">
          <label for="dhcpv6-iface">Interface</label>
          <select id="dhcpv6-iface" bind:value={dhcpv6Config.interface}>
            <option value="veth-lan0">veth-lan0</option>
            <option value="veth-lan1">veth-lan1</option>
          </select>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="range-start">Range Start (IA_NA)</label>
            <input type="text" id="range-start" bind:value={dhcpv6Config.rangeStart}
                   placeholder="2001:db8:1::100" />
          </div>
          <div class="form-group">
            <label for="range-end">Range End (IA_NA)</label>
            <input type="text" id="range-end" bind:value={dhcpv6Config.rangeEnd}
                   placeholder="2001:db8:1::200" />
          </div>
        </div>

        <div class="form-row">
          <div class="form-group">
            <label for="gateway">Gateway</label>
            <input type="text" id="gateway" bind:value={dhcpv6Config.gateway}
                   placeholder="2001:db8:1::1" />
          </div>
          <div class="form-group">
            <label for="lease-time">Lease Time (seconds)</label>
            <input type="number" id="lease-time" bind:value={dhcpv6Config.leaseTime}
                   min="60" max="604800" />
          </div>
        </div>

        <div class="form-group">
          <label>
            <input type="checkbox" bind:checked={dhcpv6Config.enableIaNa} />
            Enable IA_NA (Address Assignment)
          </label>
        </div>

        <div class="form-group">
          <label>
            <input type="checkbox" bind:checked={dhcpv6Config.enableIaPd} />
            Enable IA_PD (Prefix Delegation)
          </label>
        </div>

        {#if dhcpv6Config.enableIaPd}
          <div class="form-group">
            <label for="pd-prefix">Prefix Delegation Pool</label>
            <input type="text" id="pd-prefix" bind:value={dhcpv6Config.pdPrefix}
                   placeholder="2001:db8:2::/48" />
          </div>
        {/if}

        <button type="submit">Save DHCPv6 Settings</button>
      </form>
    </div>

    <!-- DHCPv6 Leases -->
    <div class="status-card">
      <h2>DHCPv6 Leases</h2>
      {#if dhcpv6Status?.ia_na?.leases?.length > 0}
        <h3>IA_NA Leases (Address Assignment)</h3>
        <table>
          <thead>
            <tr>
              <th>Address</th>
              <th>MAC Address</th>
              <th>Hostname</th>
              <th>Expires</th>
            </tr>
          </thead>
          <tbody>
            {#each dhcpv6Status.ia_na.leases as lease}
              <tr>
                <td>{lease.address}</td>
                <td>{lease.mac}</td>
                <td>{lease.hostname}</td>
                <td>{lease.expires}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty">No DHCPv6 IA_NA leases</p>
      {/if}

      {#if dhcpv6Status?.ia_pd?.enabled}
        <h3>IA_PD Status</h3>
        <p>Prefix Delegation: <span class="status-active">Enabled</span></p>
        <p>Delegation Pool: {dhcpv6Status.ia_pd.prefix || 'N/A'}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .ipv6-page {
    max-width: 1000px;
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
    margin: 1rem 0 0.5rem;
    color: #aaa;
  }

  .config-card, .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 2rem;
  }

  .error-banner {
    background: #4a1a1a;
    color: #ff6666;
    padding: 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    border: 1px solid #ff4444;
  }

  .success-banner {
    background: #1a4a1a;
    color: #00ff88;
    padding: 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    border: 1px solid #00ff88;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .form-group label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    font-size: 0.95rem;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  label {
    font-size: 0.9rem;
    color: #888;
  }

  input, select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.75rem;
    border-radius: 0.5rem;
    font-size: 1rem;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  input[type="checkbox"] {
    width: 1.2rem;
    height: 1.2rem;
    cursor: pointer;
  }

  button {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 1rem;
    border-radius: 0.5rem;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    margin-top: 1rem;
  }

  button:hover {
    opacity: 0.9;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
  }

  th, td {
    padding: 0.75rem;
    text-align: left;
    border-bottom: 1px solid #333;
  }

  th {
    color: #888;
    font-weight: 600;
    font-size: 0.85rem;
    text-transform: uppercase;
  }

  td {
    font-family: monospace;
    font-size: 0.9rem;
  }

  tr:hover td {
    background: rgba(0, 255, 136, 0.05);
  }

  .empty {
    color: #888;
    font-style: italic;
  }

  .status-active {
    color: #00ff88;
    font-weight: bold;
  }
</style>
