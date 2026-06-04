<script lang="ts">
  import { onMount } from 'svelte';

  let frrStatus: any = null;
  let routes: any[] = [];
  let loading = true;
  let error = '';
  let activeTab: 'status' | 'routes' | 'add-route' = 'status';

  // Add route form
  let newRoute = {
    prefix: '',
    nexthop: '',
    interface: '',
    distance: undefined as number | undefined,
  };

  // Delete route form
  let delRoute = {
    prefix: '',
    nexthop: '',
    interface: '',
    distance: undefined as number | undefined,
  };

  let operationResult = '';
  let operationError = '';

  onMount(async () => {
    await fetchStatus();
    await fetchRoutes();
  });

  async function fetchStatus() {
    try {
      loading = true;
      const res = await fetch('/api/frr/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        frrStatus = data;
      }
    } catch (e) {
      error = 'Failed to fetch FRR status';
    } finally {
      loading = false;
    }
  }

  async function fetchRoutes() {
    try {
      const res = await fetch('/api/frr/routes');
      const data = await res.json();
      if (!data.error) {
        routes = data.routes || [];
      }
    } catch (e) {
      // Ignore - routes are optional
    }
  }

  async function handleAddRoute() {
    operationResult = '';
    operationError = '';

    if (!newRoute.prefix) {
      operationError = 'Prefix is required';
      return;
    }

    const body: any = { prefix: newRoute.prefix };
    if (newRoute.nexthop) body.nexthop = newRoute.nexthop;
    if (newRoute.interface) body.interface = newRoute.interface;
    if (newRoute.distance !== undefined) body.distance = newRoute.distance;

    try {
      const res = await fetch('/api/frr/add-route', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (data.error) {
        operationError = data.error;
      } else {
        operationResult = data.message || 'Route added successfully';
        newRoute = { prefix: '', nexthop: '', interface: '', distance: undefined };
        await fetchRoutes();
      }
    } catch (e) {
      operationError = 'Failed to add route';
    }
  }

  async function handleDelRoute() {
    operationResult = '';
    operationError = '';

    if (!delRoute.prefix) {
      operationError = 'Prefix is required';
      return;
    }

    const body: any = { prefix: delRoute.prefix };
    if (delRoute.nexthop) body.nexthop = delRoute.nexthop;
    if (delRoute.interface) body.interface = delRoute.interface;
    if (delRoute.distance !== undefined) body.distance = delRoute.distance;

    try {
      const res = await fetch('/api/frr/del-route', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json();
      if (data.error) {
        operationError = data.error;
      } else {
        operationResult = data.message || 'Route deleted successfully';
        delRoute = { prefix: '', nexthop: '', interface: '', distance: undefined };
        await fetchRoutes();
      }
    } catch (e) {
      operationError = 'Failed to delete route';
    }
  }
</script>

<svelte:head>
  <title>VectorOS - FRRouting Management</title>
</svelte:head>

<div class="frr-page">
  <h1>FRRouting Management</h1>

  <!-- Tab navigation -->
  <div class="tabs">
    <button
      class:active={activeTab === 'status'}
      on:click={() => (activeTab = 'status')}
    >
      Status
    </button>
    <button
      class:active={activeTab === 'routes'}
      on:click={() => (activeTab = 'routes')}
    >
      Routes
    </button>
    <button
      class:active={activeTab === 'add-route'}
      on:click={() => (activeTab = 'add-route')}
    >
      Add Route
    </button>
  </div>

  <!-- Status tab -->
  {#if activeTab === 'status'}
    <div class="status-card">
      <h2>FRRouting Status</h2>
      {#if loading}
        <p>Loading...</p>
      {:else if error}
        <p class="error">{error}</p>
      {:else if frrStatus}
        <div class="status-info">
          <p>
            Running:
            <span class:status-ok={frrStatus.running} class:status-off={!frrStatus.running}>
              {frrStatus.running ? 'Yes' : 'No'}
            </span>
          </p>
          <p>Version: {frrStatus.version || 'N/A'}</p>
        </div>

        {#if frrStatus.daemons && Object.keys(frrStatus.daemons).length > 0}
          <div class="daemons">
            <h3>Daemons</h3>
            <div class="daemon-grid">
              {#each Object.entries(frrStatus.daemons) as [name, active]}
                <div class="daemon-item">
                  <span class="daemon-name">{name}</span>
                  <span class:status-ok={active} class:status-off={!active}>
                    {active ? 'active' : 'inactive'}
                  </span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        {#if frrStatus.bgp && frrStatus.bgp.summary}
          <div class="protocol-section">
            <h3>BGP Summary</h3>
            <pre>{frrStatus.bgp.summary}</pre>
          </div>
        {/if}

        {#if frrStatus.ospf && frrStatus.ospf.neighbors}
          <div class="protocol-section">
            <h3>OSPF Neighbors</h3>
            <pre>{frrStatus.ospf.neighbors}</pre>
          </div>
        {/if}
      {:else}
        <p>No status available</p>
      {/if}
    </div>
  {/if}

  <!-- Routes tab -->
  {#if activeTab === 'routes'}
    <div class="status-card">
      <h2>Routing Table</h2>

      <button class="refresh-btn" on:click={fetchRoutes}>Refresh</button>

      {#if routes.length > 0}
        <div class="routes-table">
          <table>
            <thead>
              <tr>
                <th>Protocol</th>
                <th>Prefix</th>
                <th>Nexthop</th>
                <th>Interface</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {#each routes as route}
                <tr>
                  <td>
                    <span class="protocol-badge protocol-{route.protocol}">{route.protocol}</span>
                  </td>
                  <td>{route.prefix}</td>
                  <td>{route.nexthop || '-'}</td>
                  <td>{route.interface || '-'}</td>
                  <td class="raw-route">{route.raw}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="no-routes">No routes found</p>
      {/if}
    </div>

    <!-- Delete route section -->
    <div class="status-card delete-card">
      <h2>Delete Route</h2>
      <form on:submit|preventDefault={handleDelRoute}>
        <div class="form-row">
          <div class="form-group">
            <label for="del-prefix">Prefix *</label>
            <input
              type="text"
              id="del-prefix"
              bind:value={delRoute.prefix}
              placeholder="e.g. 10.0.0.0/8"
            />
          </div>
          <div class="form-group">
            <label for="del-nexthop">Nexthop</label>
            <input
              type="text"
              id="del-nexthop"
              bind:value={delRoute.nexthop}
              placeholder="e.g. 192.168.1.1"
            />
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="del-interface">Interface</label>
            <input
              type="text"
              id="del-interface"
              bind:value={delRoute.interface}
              placeholder="e.g. eth0"
            />
          </div>
          <div class="form-group">
            <label for="del-distance">Distance</label>
            <input
              type="number"
              id="del-distance"
              bind:value={delRoute.distance}
              min="1"
              max="255"
              placeholder="e.g. 1"
            />
          </div>
        </div>
        <button type="submit" class="btn-danger">Delete Route</button>
      </form>
    </div>
  {/if}

  <!-- Add route tab -->
  {#if activeTab === 'add-route'}
    <div class="status-card">
      <h2>Add Static Route</h2>
      <form on:submit|preventDefault={handleAddRoute}>
        <div class="form-row">
          <div class="form-group">
            <label for="prefix">Prefix *</label>
            <input
              type="text"
              id="prefix"
              bind:value={newRoute.prefix}
              placeholder="e.g. 10.0.0.0/8"
            />
          </div>
          <div class="form-group">
            <label for="nexthop">Nexthop</label>
            <input
              type="text"
              id="nexthop"
              bind:value={newRoute.nexthop}
              placeholder="e.g. 192.168.1.1"
            />
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="interface">Interface</label>
            <input
              type="text"
              id="interface"
              bind:value={newRoute.interface}
              placeholder="e.g. eth0"
            />
          </div>
          <div class="form-group">
            <label for="distance">Distance</label>
            <input
              type="number"
              id="distance"
              bind:value={newRoute.distance}
              min="1"
              max="255"
              placeholder="e.g. 1"
            />
          </div>
        </div>
        <button type="submit">Add Route</button>
      </form>
    </div>
  {/if}

  <!-- Operation result/error messages -->
  {#if operationResult}
    <div class="message success-message">{operationResult}</div>
  {/if}
  {#if operationError}
    <div class="message error-message">{operationError}</div>
  {/if}
</div>

<style>
  .frr-page {
    max-width: 1000px;
  }

  h1 {
    margin-bottom: 2rem;
    color: #00ff88;
  }

  .tabs {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  .tabs button {
    background: #1a1a2e;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    cursor: pointer;
    font-size: 0.9rem;
    transition: all 0.2s;
  }

  .tabs button:hover {
    background: #16213e;
  }

  .tabs button.active {
    background: #00ff88;
    color: #0f0f23;
    border-color: #00ff88;
  }

  .status-card {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    margin-bottom: 1.5rem;
  }

  .delete-card {
    border-left: 3px solid #ff4444;
  }

  h2 {
    margin-bottom: 1rem;
    color: #e0e0e0;
  }

  h3 {
    margin: 1rem 0 0.5rem;
    color: #e0e0e0;
  }

  .status-info p {
    margin: 0.5rem 0;
  }

  .status-ok {
    color: #00ff88;
    font-weight: bold;
  }

  .status-off {
    color: #ff4444;
    font-weight: bold;
  }

  .error {
    color: #ff4444;
  }

  .daemon-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.75rem;
    margin-top: 0.5rem;
  }

  .daemon-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: #0f0f23;
    padding: 0.5rem 0.75rem;
    border-radius: 0.5rem;
  }

  .daemon-name {
    font-weight: 500;
  }

  .protocol-section pre {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
    overflow-x: auto;
    font-size: 0.85rem;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .refresh-btn {
    margin-bottom: 1rem;
    padding: 0.5rem 1rem;
    font-size: 0.85rem;
  }

  .routes-table {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th, td {
    padding: 0.75rem;
    text-align: left;
    border-bottom: 1px solid #333;
  }

  th {
    color: #888;
    font-size: 0.85rem;
    text-transform: uppercase;
  }

  .protocol-badge {
    padding: 0.2rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .protocol-connected {
    background: #00ff8833;
    color: #00ff88;
  }

  .protocol-static {
    background: #4488ff33;
    color: #4488ff;
  }

  .protocol-ospf {
    background: #ff880033;
    color: #ff8800;
  }

  .protocol-bgp {
    background: #ff44ff33;
    color: #ff44ff;
  }

  .protocol-kernel {
    background: #ffff0033;
    color: #ffff00;
  }

  .protocol-other {
    background: #88888833;
    color: #888;
  }

  .raw-route {
    font-size: 0.8rem;
    color: #888;
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .no-routes {
    color: #888;
    text-align: center;
    padding: 2rem;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
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

  button {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 1rem;
    border-radius: 0.5rem;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    margin-top: 0.5rem;
  }

  button:hover {
    opacity: 0.9;
  }

  .btn-danger {
    background: #ff4444;
  }

  .btn-danger:hover {
    background: #cc3333;
  }

  .message {
    padding: 1rem;
    border-radius: 0.5rem;
    margin-top: 1rem;
    font-weight: 500;
  }

  .success-message {
    background: #00ff8833;
    color: #00ff88;
    border: 1px solid #00ff88;
  }

  .error-message {
    background: #ff444433;
    color: #ff4444;
    border: 1px solid #ff4444;
  }
</style>
