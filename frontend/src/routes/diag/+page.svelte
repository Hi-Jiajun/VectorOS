<script lang="ts">
  import { onMount } from 'svelte';

  // Tab state
  let activeTab: 'ping' | 'traceroute' | 'dns' | 'portscan' = 'ping';

  // Tool availability
  let toolStatus: any = null;

  // Ping state
  let pingHost = '';
  let pingCount = 4;
  let pingResult: any = null;
  let pingLoading = false;

  // Traceroute state
  let traceHost = '';
  let traceMaxHops = 30;
  let traceResult: any = null;
  let traceLoading = false;

  // DNS state
  let dnsDomain = '';
  let dnsServer = '';
  let dnsResult: any = null;
  let dnsLoading = false;

  // Port scan state
  let scanHost = '';
  let scanPorts = '22,80,443';
  let scanResult: any = null;
  let scanLoading = false;

  let error = '';

  onMount(async () => {
    try {
      const res = await fetch('/api/diag/status');
      toolStatus = await res.json();
    } catch (e) {
      // Ignore
    }
  });

  async function runPing() {
    if (!pingHost.trim()) return;
    try {
      pingLoading = true;
      error = '';
      pingResult = null;
      const res = await fetch('/api/diag/ping', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ host: pingHost.trim(), count: pingCount })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        pingResult = data;
      }
    } catch (e) {
      error = 'Failed to run ping';
    } finally {
      pingLoading = false;
    }
  }

  async function runTraceroute() {
    if (!traceHost.trim()) return;
    try {
      traceLoading = true;
      error = '';
      traceResult = null;
      const res = await fetch('/api/diag/traceroute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ host: traceHost.trim(), max_hops: traceMaxHops })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        traceResult = data;
      }
    } catch (e) {
      error = 'Failed to run traceroute';
    } finally {
      traceLoading = false;
    }
  }

  async function runDns() {
    if (!dnsDomain.trim()) return;
    try {
      dnsLoading = true;
      error = '';
      dnsResult = null;
      const body: any = { domain: dnsDomain.trim() };
      if (dnsServer.trim()) body.server = dnsServer.trim();
      const res = await fetch('/api/diag/dns', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        dnsResult = data;
      }
    } catch (e) {
      error = 'Failed to run DNS lookup';
    } finally {
      dnsLoading = false;
    }
  }

  async function runPortScan() {
    if (!scanHost.trim() || !scanPorts.trim()) return;
    try {
      scanLoading = true;
      error = '';
      scanResult = null;
      const res = await fetch('/api/diag/portscan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ host: scanHost.trim(), ports: scanPorts.trim() })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        scanResult = data;
      }
    } catch (e) {
      error = 'Failed to run port scan';
    } finally {
      scanLoading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent, fn: () => void) {
    if (e.key === 'Enter') fn();
  }

  function lossColor(loss: string) {
    const pct = parseFloat(loss);
    if (pct === 0) return '#00ff88';
    if (pct < 50) return '#ffaa00';
    return '#ff4444';
  }
</script>

<svelte:head>
  <title>VectorOS - Network Diagnostics</title>
</svelte:head>

<div class="diag-page">
  <h1>Network Diagnostics</h1>

  <!-- Tool status -->
  {#if toolStatus && toolStatus.tools}
    <div class="status-bar">
      <span class="status-label">Available Tools:</span>
      {#if toolStatus.tools.ping}<span class="tag tag-ok">Ping</span>{/if}
      {#if toolStatus.tools.traceroute}<span class="tag tag-ok">Traceroute</span>{/if}
      {#if toolStatus.tools.dns_lookup}<span class="tag tag-ok">DNS Lookup</span>{/if}
      {#if toolStatus.tools.port_scan}<span class="tag tag-ok">Port Scan</span>{/if}
    </div>
  {/if}

  {#if error}
    <div class="error-card">{error}</div>
  {/if}

  <!-- Tab bar -->
  <div class="tabs">
    <button class:active={activeTab === 'ping'} on:click={() => activeTab = 'ping'}>Ping</button>
    <button class:active={activeTab === 'traceroute'} on:click={() => activeTab = 'traceroute'}>Traceroute</button>
    <button class:active={activeTab === 'dns'} on:click={() => activeTab = 'dns'}>DNS Lookup</button>
    <button class:active={activeTab === 'portscan'} on:click={() => activeTab = 'portscan'}>Port Scan</button>
  </div>

  <!-- Ping Tab -->
  {#if activeTab === 'ping'}
    <div class="tool-card">
      <h2>Ping</h2>
      <p class="tool-desc">Send ICMP echo requests to test connectivity and measure latency.</p>
      <div class="input-row">
        <div class="form-group grow">
          <label for="ping-host">Host</label>
          <input
            type="text"
            id="ping-host"
            bind:value={pingHost}
            placeholder="e.g. 8.8.8.8 or google.com"
            on:keydown={(e) => handleKeydown(e, runPing)}
          />
        </div>
        <div class="form-group" style="width: 100px;">
          <label for="ping-count">Count</label>
          <input type="number" id="ping-count" bind:value={pingCount} min="1" max="100" />
        </div>
        <div class="form-group" style="align-self: flex-end;">
          <button class="btn-primary" on:click={runPing} disabled={pingLoading}>
            {pingLoading ? 'Running...' : 'Run Ping'}
          </button>
        </div>
      </div>

      {#if pingResult}
        <div class="result-card">
          <h3>Results for {pingResult.host}</h3>
          <div class="stats-grid">
            <div class="stat">
              <span class="stat-label">Sent</span>
              <span class="stat-value">{pingResult.packets_sent}</span>
            </div>
            <div class="stat">
              <span class="stat-label">Received</span>
              <span class="stat-value">{pingResult.packets_received}</span>
            </div>
            <div class="stat">
              <span class="stat-label">Loss</span>
              <span class="stat-value" style="color: {lossColor(pingResult.packet_loss)}">{pingResult.packet_loss}</span>
            </div>
          </div>

          {#if pingResult.rtt}
            <div class="stats-grid">
              <div class="stat">
                <span class="stat-label">Min</span>
                <span class="stat-value">{pingResult.rtt.min.toFixed(2)} ms</span>
              </div>
              <div class="stat">
                <span class="stat-label">Avg</span>
                <span class="stat-value">{pingResult.rtt.avg.toFixed(2)} ms</span>
              </div>
              <div class="stat">
                <span class="stat-label">Max</span>
                <span class="stat-value">{pingResult.rtt.max.toFixed(2)} ms</span>
              </div>
              <div class="stat">
                <span class="stat-label">Mdev</span>
                <span class="stat-value">{pingResult.rtt.mdev.toFixed(2)} ms</span>
              </div>
            </div>
          {/if}

          {#if pingResult.replies && pingResult.replies.length > 0}
            <h4>Replies</h4>
            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Seq</th>
                    <th>Source</th>
                    <th>Bytes</th>
                    <th>TTL</th>
                    <th>Time</th>
                  </tr>
                </thead>
                <tbody>
                  {#each pingResult.replies as reply}
                    <tr>
                      <td>{reply.icmp_seq}</td>
                      <td>{reply.source}</td>
                      <td>{reply.bytes}</td>
                      <td>{reply.ttl}</td>
                      <td>{reply.time_ms.toFixed(2)} ms</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Traceroute Tab -->
  {#if activeTab === 'traceroute'}
    <div class="tool-card">
      <h2>Traceroute</h2>
      <p class="tool-desc">Trace the network path to a destination host.</p>
      <div class="input-row">
        <div class="form-group grow">
          <label for="trace-host">Host</label>
          <input
            type="text"
            id="trace-host"
            bind:value={traceHost}
            placeholder="e.g. 8.8.8.8 or google.com"
            on:keydown={(e) => handleKeydown(e, runTraceroute)}
          />
        </div>
        <div class="form-group" style="width: 120px;">
          <label for="trace-max-hops">Max Hops</label>
          <input type="number" id="trace-max-hops" bind:value={traceMaxHops} min="1" max="64" />
        </div>
        <div class="form-group" style="align-self: flex-end;">
          <button class="btn-primary" on:click={runTraceroute} disabled={traceLoading}>
            {traceLoading ? 'Running...' : 'Run Traceroute'}
          </button>
        </div>
      </div>

      {#if traceResult}
        <div class="result-card">
          <h3>Results for {traceResult.host} ({traceResult.hop_count} hops)</h3>
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Hop</th>
                  <th>Address</th>
                  <th>Times</th>
                </tr>
              </thead>
              <tbody>
                {#each traceResult.hops as hop}
                  <tr class:timeout={hop.timeout}>
                    <td>{hop.hop}</td>
                    <td>
                      {#if hop.timeout}
                        <span class="timeout-text">* * *</span>
                      {:else}
                        {hop.addresses.join(' / ')}
                      {/if}
                    </td>
                    <td>
                      {#if hop.times_ms.length > 0}
                        {hop.times_ms.map(t => t.toFixed(2) + ' ms').join('  ')}
                      {:else}
                        -
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- DNS Tab -->
  {#if activeTab === 'dns'}
    <div class="tool-card">
      <h2>DNS Lookup</h2>
      <p class="tool-desc">Resolve domain names and query DNS records.</p>
      <div class="input-row">
        <div class="form-group grow">
          <label for="dns-domain">Domain</label>
          <input
            type="text"
            id="dns-domain"
            bind:value={dnsDomain}
            placeholder="e.g. google.com"
            on:keydown={(e) => handleKeydown(e, runDns)}
          />
        </div>
        <div class="form-group grow">
          <label for="dns-server">DNS Server (optional)</label>
          <input
            type="text"
            id="dns-server"
            bind:value={dnsServer}
            placeholder="e.g. 8.8.8.8"
          />
        </div>
        <div class="form-group" style="align-self: flex-end;">
          <button class="btn-primary" on:click={runDns} disabled={dnsLoading}>
            {dnsLoading ? 'Looking up...' : 'Run Lookup'}
          </button>
        </div>
      </div>

      {#if dnsResult}
        <div class="result-card">
          <h3>DNS Records for {dnsResult.domain}</h3>
          <p class="result-meta">Server: {dnsResult.server}</p>

          {#if dnsResult.a_records && dnsResult.a_records.length > 0}
            <div class="dns-section">
              <h4>A Records (IPv4)</h4>
              {#each dnsResult.a_records as record}
                <div class="dns-record">{record}</div>
              {/each}
            </div>
          {/if}

          {#if dnsResult.aaaa_records && dnsResult.aaaa_records.length > 0}
            <div class="dns-section">
              <h4>AAAA Records (IPv6)</h4>
              {#each dnsResult.aaaa_records as record}
                <div class="dns-record">{record}</div>
              {/each}
            </div>
          {/if}

          {#if dnsResult.mx_records && dnsResult.mx_records.length > 0}
            <div class="dns-section">
              <h4>MX Records (Mail)</h4>
              {#each dnsResult.mx_records as record}
                <div class="dns-record">{record}</div>
              {/each}
            </div>
          {/if}

          {#if dnsResult.ns_records && dnsResult.ns_records.length > 0}
            <div class="dns-section">
              <h4>NS Records (Nameservers)</h4>
              {#each dnsResult.ns_records as record}
                <div class="dns-record">{record}</div>
              {/each}
            </div>
          {/if}

          {#if dnsResult.soa_record}
            <div class="dns-section">
              <h4>SOA Record</h4>
              <div class="dns-record">{dnsResult.soa_record}</div>
            </div>
          {/if}

          {#if (!dnsResult.a_records || dnsResult.a_records.length === 0) &&
                (!dnsResult.aaaa_records || dnsResult.aaaa_records.length === 0) &&
                !dnsResult.soa_record}
            <p class="no-data">No DNS records found for this domain.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Port Scan Tab -->
  {#if activeTab === 'portscan'}
    <div class="tool-card">
      <h2>Port Scan</h2>
      <p class="tool-desc">Scan TCP ports on a target host to discover open services.</p>
      <div class="input-row">
        <div class="form-group grow">
          <label for="scan-host">Host</label>
          <input
            type="text"
            id="scan-host"
            bind:value={scanHost}
            placeholder="e.g. 192.168.1.1 or example.com"
            on:keydown={(e) => handleKeydown(e, runPortScan)}
          />
        </div>
        <div class="form-group grow">
          <label for="scan-ports">Ports</label>
          <input
            type="text"
            id="scan-ports"
            bind:value={scanPorts}
            placeholder="e.g. 80,443 or 1-1024"
            on:keydown={(e) => handleKeydown(e, runPortScan)}
          />
        </div>
        <div class="form-group" style="align-self: flex-end;">
          <button class="btn-primary" on:click={runPortScan} disabled={scanLoading}>
            {scanLoading ? 'Scanning...' : 'Run Scan'}
          </button>
        </div>
      </div>
      <p class="hint">Max 1024 ports per scan. Use comma-separated values or ranges (e.g. 22,80,443 or 1-1024).</p>

      {#if scanResult}
        <div class="result-card">
          <h3>Scan Results for {scanResult.host} ({scanResult.target_ip})</h3>
          <div class="stats-grid">
            <div class="stat">
              <span class="stat-label">Scanned</span>
              <span class="stat-value">{scanResult.ports_scanned}</span>
            </div>
            <div class="stat">
              <span class="stat-label">Open</span>
              <span class="stat-value open">{scanResult.open_count}</span>
            </div>
            <div class="stat">
              <span class="stat-label">Closed</span>
              <span class="stat-value closed">{scanResult.closed_count}</span>
            </div>
          </div>

          {#if scanResult.open_ports && scanResult.open_ports.length > 0}
            <h4>Open Ports</h4>
            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Port</th>
                    <th>State</th>
                    <th>Service</th>
                  </tr>
                </thead>
                <tbody>
                  {#each scanResult.open_ports as port}
                    <tr>
                      <td class="port-num">{port.port}</td>
                      <td><span class="state-open">{port.state}</span></td>
                      <td>{port.service}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <p class="no-data">No open ports found.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .diag-page {
    max-width: 1200px;
  }

  h1 {
    margin-bottom: 1.5rem;
    color: #00ff88;
  }

  h2 {
    margin-bottom: 0.5rem;
    color: #e0e0e0;
  }

  h3 {
    margin-bottom: 1rem;
    color: #e0e0e0;
  }

  h4 {
    margin: 1rem 0 0.5rem;
    color: #888;
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-bar {
    background: #1a1a2e;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .status-label {
    color: #888;
    font-size: 0.85rem;
  }

  .tag {
    padding: 0.25rem 0.75rem;
    border-radius: 1rem;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .tag-ok {
    background: #00ff8822;
    color: #00ff88;
    border: 1px solid #00ff8844;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 1.5rem;
    background: #1a1a2e;
    padding: 0.25rem;
    border-radius: 0.75rem;
  }

  .tabs button {
    flex: 1;
    padding: 0.75rem 1rem;
    background: transparent;
    color: #888;
    border: none;
    border-radius: 0.5rem;
    cursor: pointer;
    font-size: 0.95rem;
    font-weight: 500;
    transition: all 0.2s;
  }

  .tabs button:hover {
    color: #e0e0e0;
    background: #16213e;
  }

  .tabs button.active {
    background: #00ff88;
    color: #0f0f23;
    font-weight: 600;
  }

  .tool-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .tool-desc {
    color: #888;
    margin-bottom: 1.5rem;
    font-size: 0.9rem;
  }

  .input-row {
    display: flex;
    gap: 1rem;
    align-items: flex-end;
    flex-wrap: wrap;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .grow {
    flex: 1;
    min-width: 200px;
  }

  label {
    font-size: 0.85rem;
    color: #888;
  }

  input {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.6rem;
    border-radius: 0.5rem;
    font-size: 0.95rem;
  }

  input:focus {
    outline: none;
    border-color: #00ff88;
  }

  .hint {
    color: #666;
    font-size: 0.8rem;
    margin-top: 0.5rem;
  }

  .btn-primary {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.6rem 1.5rem;
    border-radius: 0.5rem;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 1rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
    color: #ff4444;
  }

  .result-card {
    background: #0f0f23;
    border: 1px solid #333;
    padding: 1.5rem;
    border-radius: 0.5rem;
    margin-top: 1.5rem;
  }

  .result-meta {
    color: #888;
    font-size: 0.85rem;
    margin-bottom: 1rem;
  }

  .stats-grid {
    display: flex;
    gap: 2rem;
    margin-bottom: 1rem;
    flex-wrap: wrap;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .stat-label {
    color: #888;
    font-size: 0.8rem;
    text-transform: uppercase;
  }

  .stat-value {
    color: #e0e0e0;
    font-size: 1.5rem;
    font-weight: bold;
  }

  .stat-value.open {
    color: #00ff88;
  }

  .stat-value.closed {
    color: #ff4444;
  }

  .table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th {
    text-align: left;
    color: #888;
    font-weight: 600;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid #444;
    font-size: 0.8rem;
    text-transform: uppercase;
  }

  td {
    padding: 0.5rem 1rem;
    border-bottom: 1px solid #222;
    color: #e0e0e0;
  }

  tr:hover {
    background: #16213e;
  }

  tr.timeout {
    opacity: 0.5;
  }

  .timeout-text {
    color: #666;
    font-style: italic;
  }

  .port-num {
    font-family: 'Courier New', monospace;
    font-weight: bold;
    color: #00aaff;
  }

  .state-open {
    color: #00ff88;
    font-weight: bold;
  }

  .dns-section {
    margin-bottom: 1rem;
  }

  .dns-record {
    background: #16213e;
    padding: 0.5rem 0.75rem;
    border-radius: 0.3rem;
    font-family: 'Courier New', monospace;
    font-size: 0.9rem;
    margin-bottom: 0.25rem;
    color: #e0e0e0;
  }

  .no-data {
    color: #888;
    text-align: center;
    padding: 2rem;
  }
</style>
