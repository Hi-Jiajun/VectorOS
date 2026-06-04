<script lang="ts">
  import { onMount } from 'svelte';

  // ── State ───────────────────────────────────────────────────────
  let activeTab = 'rules';
  let firewallStatus: any = null;
  let rules: any[] = [];
  let groups: any[] = [];
  let aliases: any[] = [];
  let schedules: any[] = [];
  let geoip: any = { enabled: false, blocked_countries: [], allowed_countries: [] };
  let shaper: any = { enabled: false, interfaces: {}, queues: [] };
  let ids: any = { enabled: false, interfaces: [], rule_categories: {}, stats: {}, recent_alerts: [] };
  let loading = true;
  let error = '';
  let success = '';

  // Drag-and-drop state
  let dragIndex: number | null = null;
  let dragOverIndex: number | null = null;

  // Rule form
  let ruleForm: any = resetRuleForm();
  let editingRuleId: number | null = null;

  // Group form
  let groupForm = { name: '', description: '' };

  // Alias form
  let aliasForm: any = { name: '', type: 'host', entries: '', description: '', refresh_interval: 0 };

  // Schedule form
  let scheduleForm: any = { name: '', description: '', time_ranges: [{ day: 1, start: '08:00', end: '17:00' }] };

  // GeoIP form
  let geoipForm: any = { enabled: false, default_action: 'block', blocked_countries: '', allowed_countries: '' };

  // Shaper form
  let shaperIfaceForm = { interface: '', bandwidth: '', download: '', upload: '' };
  let shaperQueueForm = { name: '', weight: '50', priority: '5', dscp: '', interface: '', description: '' };

  // IDS form
  let idsForm: any = { enabled: false, interfaces: '' };

  const dayNames = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const protocols = ['ip', 'tcp', 'udp', 'icmp', 'esp', 'gre'];
  const directions = ['both', 'in', 'out'];
  const dscpOptions = ['', 'ef', 'af11', 'af12', 'af13', 'af21', 'af22', 'af23', 'af31', 'af32', 'af33', 'cs0', 'cs1', 'cs2', 'cs3', 'cs4', 'cs5', 'cs6', 'cs7'];

  function resetRuleForm() {
    return {
      action: 'pass', direction: 'both', protocol: 'ip',
      src_ip: '', dst_ip: '', src_port: '', dst_port: '',
      src_alias: '', dst_alias: '', src_port_alias: '', dst_port_alias: '',
      group: '', schedule: '', log: false, description: '', dscp: '',
      log_prefix: '', geoip_countries: []
    };
  }

  onMount(() => fetchAll());

  // ── Data fetching ───────────────────────────────────────────────

  async function fetchAll() {
    try {
      loading = true;
      error = '';
      const res = await fetch('/api/firewall/status');
      const data = await res.json();
      if (data.error) { error = data.error; return; }
      firewallStatus = data;
      rules = data.rules || [];
      groups = data.groups || [];
      aliases = data.aliases || [];
      schedules = data.schedules || [];
      geoip = data.geoip || {};
      shaper = data.shaper || {};
      ids = data.ids || {};
    } catch (e) {
      error = 'Failed to fetch firewall status';
    } finally {
      loading = false;
    }
  }

  // ── Firewall enable/disable ─────────────────────────────────────

  async function toggleFirewall(enable: boolean) {
    const endpoint = enable ? '/api/firewall/enable' : '/api/firewall/disable';
    const res = await fetch(endpoint, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = data.message || `Firewall ${enable ? 'enabled' : 'disabled'}`; await fetchAll(); }
  }

  // ── Rules CRUD ──────────────────────────────────────────────────

  async function saveRule() {
    error = ''; success = '';
    const payload: any = { ...ruleForm };
    if (ruleForm.src_port) payload.src_port = ruleForm.src_port;
    if (ruleForm.dst_port) payload.dst_port = ruleForm.dst_port;
    if (!ruleForm.src_alias) delete payload.src_alias;
    if (!ruleForm.dst_alias) delete payload.dst_alias;
    if (!ruleForm.src_port_alias) delete payload.src_port_alias;
    if (!ruleForm.dst_port_alias) delete payload.dst_port_alias;
    if (!ruleForm.group) delete payload.group;
    if (!ruleForm.schedule) delete payload.schedule;
    if (!ruleForm.dscp) delete payload.dscp;
    if (!ruleForm.log_prefix) delete payload.log_prefix;

    if (editingRuleId !== null) {
      payload.id = editingRuleId;
      const res = await fetch('/api/firewall/update-rule', {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload)
      });
      const data = await res.json();
      if (data.error) { error = data.error; return; }
      success = `Rule #${editingRuleId} updated`;
    } else {
      const res = await fetch('/api/firewall/add-rule', {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload)
      });
      const data = await res.json();
      if (data.error) { error = data.error; return; }
      success = 'Rule added';
    }

    ruleForm = resetRuleForm();
    editingRuleId = null;
    await fetchAll();
  }

  function editRule(rule: any) {
    editingRuleId = rule.id;
    ruleForm = {
      action: rule.action,
      direction: rule.direction || 'both',
      protocol: rule.protocol || 'ip',
      src_ip: rule.src_ip || '',
      dst_ip: rule.dst_ip || '',
      src_port: rule.src_port || '',
      dst_port: rule.dst_port || '',
      src_alias: rule.src_alias || '',
      dst_alias: rule.dst_alias || '',
      src_port_alias: rule.src_port_alias || '',
      dst_port_alias: rule.dst_port_alias || '',
      group: rule.group || '',
      schedule: rule.schedule || '',
      log: rule.log || false,
      description: rule.description || '',
      dscp: rule.dscp || '',
      log_prefix: rule.log_prefix || '',
      geoip_countries: rule.geoip_countries || [],
    };
    activeTab = 'rules';
  }

  async function deleteRule(id: number) {
    if (!confirm(`Delete rule #${id}?`)) return;
    const res = await fetch('/api/firewall/del-rule', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Rule #${id} deleted`; await fetchAll(); }
  }

  async function toggleRule(rule: any) {
    const res = await fetch('/api/firewall/update-rule', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: rule.id, enabled: !rule.enabled })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else await fetchAll();
  }

  // ── Drag-and-drop reorder ───────────────────────────────────────

  function onDragStart(e: DragEvent, index: number) {
    dragIndex = index;
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }

  function onDragOver(e: DragEvent, index: number) {
    e.preventDefault();
    dragOverIndex = index;
  }

  function onDragEnd() {
    dragIndex = null;
    dragOverIndex = null;
  }

  async function onDrop(e: DragEvent, index: number) {
    e.preventDefault();
    if (dragIndex === null || dragIndex === index) { onDragEnd(); return; }

    const reordered = [...rules];
    const [moved] = reordered.splice(dragIndex, 1);
    reordered.splice(index, 0, moved);

    // Update order values
    const ruleIds = reordered.map((r: any) => r.id);
    const res = await fetch('/api/firewall/reorder', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ rule_ids: ruleIds })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    onDragEnd();
    await fetchAll();
  }

  // ── Groups ──────────────────────────────────────────────────────

  async function addGroup() {
    if (!groupForm.name) { error = 'Group name required'; return; }
    const res = await fetch('/api/firewall/groups/add', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(groupForm)
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Group '${groupForm.name}' created`; groupForm = { name: '', description: '' }; await fetchAll(); }
  }

  async function deleteGroup(name: string) {
    if (!confirm(`Delete group '${name}'?`)) return;
    const res = await fetch(`/api/firewall/groups/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Group '${name}' deleted`; await fetchAll(); }
  }

  // ── Aliases ─────────────────────────────────────────────────────

  async function addAlias() {
    if (!aliasForm.name || !aliasForm.type) { error = 'Name and type required'; return; }
    const entries = aliasForm.entries.split(/[,\n]/).map((e: string) => e.trim()).filter((e: string) => e);
    const res = await fetch('/api/firewall/aliases/add', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ...aliasForm, entries })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Alias '${aliasForm.name}' created`; aliasForm = { name: '', type: 'host', entries: '', description: '', refresh_interval: 0 }; await fetchAll(); }
  }

  async function deleteAlias(name: string) {
    if (!confirm(`Delete alias '${name}'?`)) return;
    const res = await fetch(`/api/firewall/aliases/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Alias '${name}' deleted`; await fetchAll(); }
  }

  async function refreshAlias(name: string) {
    const res = await fetch(`/api/firewall/aliases/${encodeURIComponent(name)}/refresh`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Alias '${name}' refreshed (${data.alias?.cached_entries?.length || 0} entries)`; await fetchAll(); }
  }

  // ── Schedules ───────────────────────────────────────────────────

  function addTimeRange() {
    scheduleForm.time_ranges = [...scheduleForm.time_ranges, { day: 1, start: '08:00', end: '17:00' }];
  }

  function removeTimeRange(i: number) {
    scheduleForm.time_ranges = scheduleForm.time_ranges.filter((_: any, idx: number) => idx !== i);
  }

  async function addSchedule() {
    if (!scheduleForm.name) { error = 'Schedule name required'; return; }
    const res = await fetch('/api/firewall/schedules/add', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(scheduleForm)
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Schedule '${scheduleForm.name}' created`; scheduleForm = { name: '', description: '', time_ranges: [{ day: 1, start: '08:00', end: '17:00' }] }; await fetchAll(); }
  }

  async function deleteSchedule(name: string) {
    if (!confirm(`Delete schedule '${name}'?`)) return;
    const res = await fetch(`/api/firewall/schedules/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `Schedule '${name}' deleted`; await fetchAll(); }
  }

  // ── GeoIP ───────────────────────────────────────────────────────

  async function saveGeoip() {
    const payload = {
      enabled: geoipForm.enabled,
      default_action: geoipForm.default_action,
      blocked_countries: geoipForm.blocked_countries.split(/[,\s]+/).filter((c: string) => c),
      allowed_countries: geoipForm.allowed_countries.split(/[,\s]+/).filter((c: string) => c),
    };
    const res = await fetch('/api/firewall/geoip', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload)
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'GeoIP configuration saved'; geoip = data.geoip; await fetchAll(); }
  }

  // ── Shaper ──────────────────────────────────────────────────────

  async function setShaperIface() {
    if (!shaperIfaceForm.interface || !shaperIfaceForm.bandwidth) { error = 'Interface and bandwidth required'; return; }
    const res = await fetch('/api/firewall/shaper/interface', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        interface: shaperIfaceForm.interface,
        bandwidth: parseInt(shaperIfaceForm.bandwidth),
        download: shaperIfaceForm.download ? parseInt(shaperIfaceForm.download) : undefined,
        upload: shaperIfaceForm.upload ? parseInt(shaperIfaceForm.upload) : undefined,
      })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'Shaper interface configured'; shaperIfaceForm = { interface: '', bandwidth: '', download: '', upload: '' }; await fetchAll(); }
  }

  async function removeShaperIface(name: string) {
    if (!confirm(`Remove shaper for ${name}?`)) return;
    const res = await fetch(`/api/firewall/shaper/interface/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'Shaper interface removed'; await fetchAll(); }
  }

  async function addShaperQueue() {
    const res = await fetch('/api/firewall/shaper/queue', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name: shaperQueueForm.name,
        weight: parseInt(shaperQueueForm.weight),
        priority: parseInt(shaperQueueForm.priority),
        dscp: shaperQueueForm.dscp || undefined,
        interface: shaperQueueForm.interface || undefined,
        description: shaperQueueForm.description || undefined,
      })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'Shaper queue added'; shaperQueueForm = { name: '', weight: '50', priority: '5', dscp: '', interface: '', description: '' }; await fetchAll(); }
  }

  async function removeShaperQueue(name: string) {
    if (!confirm(`Remove shaper queue '${name}'?`)) return;
    const res = await fetch(`/api/firewall/shaper/queue/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'Shaper queue removed'; await fetchAll(); }
  }

  // ── IDS ─────────────────────────────────────────────────────────

  async function toggleIds(enable: boolean) {
    const res = await fetch('/api/firewall/ids/config', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled: enable, interfaces: idsForm.interfaces.split(/[,\s]+/).filter((i: string) => i) })
    });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = `IDS ${enable ? 'enabled' : 'disabled'}`; await fetchAll(); }
  }

  async function clearIdsAlerts() {
    const res = await fetch('/api/firewall/ids/alerts/clear', { method: 'POST' });
    const data = await res.json();
    if (data.error) error = data.error;
    else { success = 'Alerts cleared'; await fetchAll(); }
  }

  function formatBits(bits: number): string {
    if (bits >= 1_000_000_000) return `${(bits / 1_000_000_000).toFixed(1)} Gbps`;
    if (bits >= 1_000_000) return `${(bits / 1_000_000).toFixed(1)} Mbps`;
    if (bits >= 1_000) return `${(bits / 1_000).toFixed(1)} Kbps`;
    return `${bits} bps`;
  }

  function severityColor(s: string): string {
    if (s === 'critical' || s === 'high') return '#ff4444';
    if (s === 'medium') return '#ffaa00';
    if (s === 'low') return '#88aaff';
    return '#888';
  }

  function clearMsg() { error = ''; success = ''; }
</script>

<svelte:head>
  <title>VectorOS - Firewall</title>
</svelte:head>

<div class="fw-page">
  <!-- Header -->
  <div class="fw-header">
    <div class="fw-title-row">
      <h1>Firewall</h1>
      <div class="fw-status-right">
        {#if firewallStatus}
          <span class="fw-badge" class:fw-badge-on={firewallStatus.enabled} class:fw-badge-off={!firewallStatus.enabled}>
            {firewallStatus.enabled ? 'ENABLED' : 'DISABLED'}
          </span>
          <span class="fw-policy">Default: {firewallStatus.default_policy || 'block'}</span>
          <span class="fw-count">{firewallStatus.total_rules || 0} rules</span>
        {/if}
      </div>
    </div>
    <div class="fw-controls">
      {#if firewallStatus?.enabled}
        <button class="btn-off" on:click={() => toggleFirewall(false)}>Disable Firewall</button>
      {:else}
        <button class="btn-on" on:click={() => toggleFirewall(true)}>Enable Firewall</button>
      {/if}
      <button class="btn-refresh" on:click={fetchAll}>Refresh</button>
    </div>
  </div>

  {#if error}
    <div class="msg-error">{error} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}
  {#if success}
    <div class="msg-success">{success} <button class="msg-close" on:click={clearMsg}>x</button></div>
  {/if}

  <!-- Tab Bar -->
  <div class="fw-tabs">
    {#each [
      { id: 'rules', label: 'Rules', icon: '\u{1F6E1}' },
      { id: 'groups', label: 'Groups', icon: '\u{1F4E6}' },
      { id: 'aliases', label: 'Aliases', icon: '\u{1F3F7}' },
      { id: 'schedules', label: 'Schedules', icon: '\u{1F4C5}' },
      { id: 'geoip', label: 'GeoIP', icon: '\u{1F30D}' },
      { id: 'shaper', label: 'Traffic Shaper', icon: '\u{26A1}' },
      { id: 'ids', label: 'IDS / Alerts', icon: '\u{1F6A8}' },
    ] as tab}
      <button class="fw-tab" class:fw-tab-active={activeTab === tab.id} on:click={() => activeTab = tab.id}>
        <span class="tab-icon">{tab.icon}</span> {tab.label}
      </button>
    {/each}
  </div>

  <!-- Loading -->
  {#if loading}
    <div class="fw-loading">Loading firewall data...</div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: Rules -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'rules' && !loading}
    <!-- Add/Edit Rule Form -->
    <div class="fw-card">
      <h2>{editingRuleId !== null ? `Edit Rule #${editingRuleId}` : 'Add Firewall Rule'}</h2>
      <form on:submit|preventDefault={saveRule}>
        <div class="form-grid-4">
          <div class="form-group">
            <label>Action</label>
            <select bind:value={ruleForm.action}>
              <option value="pass">Pass (Allow)</option>
              <option value="block">Block (Deny)</option>
              <option value="reject">Reject</option>
            </select>
          </div>
          <div class="form-group">
            <label>Direction</label>
            <select bind:value={ruleForm.direction}>
              <option value="both">Both</option>
              <option value="in">In</option>
              <option value="out">Out</option>
            </select>
          </div>
          <div class="form-group">
            <label>Protocol</label>
            <select bind:value={ruleForm.protocol}>
              {#each protocols as p}
                <option value={p}>{p.toUpperCase()}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>DSCP</label>
            <select bind:value={ruleForm.dscp}>
              <option value="">None</option>
              {#each dscpOptions.filter(d => d) as d}
                <option value={d}>{d.toUpperCase()}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="form-grid-4">
          <div class="form-group">
            <label>Source IP</label>
            <input type="text" bind:value={ruleForm.src_ip} placeholder="e.g. 192.168.1.0/24" />
          </div>
          <div class="form-group">
            <label>Destination IP</label>
            <input type="text" bind:value={ruleForm.dst_ip} placeholder="e.g. 10.0.0.0/8" />
          </div>
          <div class="form-group">
            <label>Source Port / Alias</label>
            <input type="text" bind:value={ruleForm.src_port} placeholder="Port or alias" />
          </div>
          <div class="form-group">
            <label>Destination Port / Alias</label>
            <input type="text" bind:value={ruleForm.dst_port} placeholder="Port or alias" />
          </div>
        </div>

        <div class="form-grid-4">
          <div class="form-group">
            <label>Source Alias</label>
            <select bind:value={ruleForm.src_alias}>
              <option value="">None</option>
              {#each aliases.filter(a => a.type === 'host' || a.type === 'network') as a}
                <option value={a.name}>{a.name} ({a.type})</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Destination Alias</label>
            <select bind:value={ruleForm.dst_alias}>
              <option value="">None</option>
              {#each aliases.filter(a => a.type === 'host' || a.type === 'network') as a}
                <option value={a.name}>{a.name} ({a.type})</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Src Port Alias</label>
            <select bind:value={ruleForm.src_port_alias}>
              <option value="">None</option>
              {#each aliases.filter(a => a.type === 'port') as a}
                <option value={a.name}>{a.name}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Dst Port Alias</label>
            <select bind:value={ruleForm.dst_port_alias}>
              <option value="">None</option>
              {#each aliases.filter(a => a.type === 'port') as a}
                <option value={a.name}>{a.name}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="form-grid-4">
          <div class="form-group">
            <label>Group</label>
            <select bind:value={ruleForm.group}>
              <option value="">None</option>
              {#each groups as g}
                <option value={g.name}>{g.name}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Schedule</label>
            <select bind:value={ruleForm.schedule}>
              <option value="">Always Active</option>
              {#each schedules as s}
                <option value={s.name}>{s.name}</option>
              {/each}
            </select>
          </div>
          <div class="form-group">
            <label>Description</label>
            <input type="text" bind:value={ruleForm.description} placeholder="Rule description" />
          </div>
          <div class="form-group">
            <label>Log Prefix</label>
            <input type="text" bind:value={ruleForm.log_prefix} placeholder="Optional prefix" />
          </div>
        </div>

        <div class="form-check-row">
          <label class="check-label">
            <input type="checkbox" bind:checked={ruleForm.log} />
            Enable logging
          </label>
        </div>

        <div class="form-actions">
          <button type="submit" class="btn-primary">{editingRuleId !== null ? 'Update Rule' : 'Add Rule'}</button>
          {#if editingRuleId !== null}
            <button type="button" class="btn-cancel" on:click={() => { editingRuleId = null; ruleForm = resetRuleForm(); }}>Cancel</button>
          {/if}
        </div>
      </form>
    </div>

    <!-- Rules Table -->
    <div class="fw-card">
      <div class="card-header-row">
        <h2>Firewall Rules ({rules.length})</h2>
        <span class="hint-text">Drag rows to reorder. Rules are evaluated top to bottom.</span>
      </div>

      {#if rules.length === 0}
        <div class="no-data">No firewall rules configured</div>
      {:else}
        <div class="rules-table">
          <div class="rule-header">
            <span class="col-drag"></span>
            <span class="col-enable"></span>
            <span class="col-order">#</span>
            <span class="col-action">Action</span>
            <span class="col-dir">Dir</span>
            <span class="col-proto">Proto</span>
            <span class="col-src">Source</span>
            <span class="col-dst">Destination</span>
            <span class="col-sched">Schedule</span>
            <span class="col-group">Group</span>
            <span class="col-desc">Description</span>
            <span class="col-ops">Actions</span>
          </div>

          {#each rules as rule, i}
            <div
              class="rule-row"
              class:rule-disabled={!rule.enabled}
              class:rule-schedule-active={rule.schedule && rule.schedule_active}
              class:rule-schedule-inactive={rule.schedule && !rule.schedule_active}
              class:drag-over={dragOverIndex === i}
              draggable="true"
              on:dragstart={(e) => onDragStart(e, i)}
              on:dragover={(e) => onDragOver(e, i)}
              on:dragend={onDragEnd}
              on:drop={(e) => onDrop(e, i)}
            >
              <span class="col-drag"><span class="drag-handle">☰</span></span>
              <span class="col-enable">
                <button
                  class="toggle-btn"
                  class:toggle-on={rule.enabled}
                  class:toggle-off={!rule.enabled}
                  on:click={() => toggleRule(rule)}
                  title={rule.enabled ? 'Disable rule' : 'Enable rule'}
                >
                  {rule.enabled ? 'ON' : 'OFF'}
                </button>
              </span>
              <span class="col-order">{i + 1}</span>
              <span class="col-action">
                <span class="action-badge" class:act-pass={rule.action === 'pass'} class:act-block={rule.action === 'block'} class:act-reject={rule.action === 'reject'}>
                  {rule.action === 'pass' ? 'PASS' : rule.action === 'block' ? 'BLOCK' : 'REJECT'}
                </span>
              </span>
              <span class="col-dir">{rule.direction || 'both'}</span>
              <span class="col-proto">{rule.protocol || 'ip'}</span>
              <span class="col-src">
                {#if rule.src_alias}<span class="alias-ref">{rule.src_alias}</span>{/if}
                {rule.src_ip || '*'}
                {#if rule.src_port}: {rule.src_port}{/if}
              </span>
              <span class="col-dst">
                {#if rule.dst_alias}<span class="alias-ref">{rule.dst_alias}</span>{/if}
                {rule.dst_ip || '*'}
                {#if rule.dst_port}: {rule.dst_port}{/if}
              </span>
              <span class="col-sched">
                {#if rule.schedule}
                  <span class="sched-badge" class:sched-active={rule.schedule_active} class:sched-inactive={!rule.schedule_active}>
                    {rule.schedule} {rule.schedule_active ? '✓' : '✗'}
                  </span>
                {:else}
                  <span class="sched-always">Always</span>
                {/if}
              </span>
              <span class="col-group">{rule.group || '-'}</span>
              <span class="col-desc">{rule.description || '-'}</span>
              <span class="col-ops">
                <button class="btn-icon btn-edit" on:click={() => editRule(rule)} title="Edit">✎</button>
                <button class="btn-icon btn-delete-icon" on:click={() => deleteRule(rule.id)} title="Delete">✖</button>
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: Groups -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'groups' && !loading}
    <div class="fw-card">
      <h2>Add Rule Group</h2>
      <form on:submit|preventDefault={addGroup}>
        <div class="form-grid-3">
          <div class="form-group">
            <label>Group Name</label>
            <input type="text" bind:value={groupForm.name} placeholder="e.g. lan-allowed" />
          </div>
          <div class="form-group">
            <label>Description</label>
            <input type="text" bind:value={groupForm.description} placeholder="Optional" />
          </div>
          <div class="form-group form-group-end">
            <button type="submit" class="btn-primary">Create Group</button>
          </div>
        </div>
      </form>
    </div>

    <div class="fw-card">
      <h2>Rule Groups ({groups.length})</h2>
      {#if groups.length === 0}
        <div class="no-data">No groups configured</div>
      {:else}
        <div class="table-wrapper">
          <table>
            <thead>
              <tr><th>Name</th><th>Description</th><th>Rules</th><th>Interfaces</th><th>Actions</th></tr>
            </thead>
            <tbody>
              {#each groups as g}
                <tr>
                  <td class="name-cell">{g.name}</td>
                  <td>{g.description || '-'}</td>
                  <td>{(g.rules || []).length}</td>
                  <td>{(g.interfaces || []).join(', ') || 'all'}</td>
                  <td>
                    <button class="btn-delete-small" on:click={() => deleteGroup(g.name)}>Delete</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: Aliases -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'aliases' && !loading}
    <div class="fw-card">
      <h2>Add Alias</h2>
      <form on:submit|preventDefault={addAlias}>
        <div class="form-grid-4">
          <div class="form-group">
            <label>Name</label>
            <input type="text" bind:value={aliasForm.name} placeholder="e.g. blocked-ips" />
          </div>
          <div class="form-group">
            <label>Type</label>
            <select bind:value={aliasForm.type}>
              <option value="host">Host (IP addresses)</option>
              <option value="network">Network (CIDR)</option>
              <option value="port">Port</option>
              <option value="url">URL (blocklist)</option>
            </select>
          </div>
          <div class="form-group">
            <label>Description</label>
            <input type="text" bind:value={aliasForm.description} placeholder="Optional" />
          </div>
          <div class="form-group">
            <label>Refresh Interval (sec, URL only)</label>
            <input type="number" bind:value={aliasForm.refresh_interval} min="0" />
          </div>
        </div>
        <div class="form-group">
          <label>Entries (comma or newline separated{aliasForm.type === 'url' ? ' -- one URL per line' : ''})</label>
          <textarea bind:value={aliasForm.entries} rows="4" placeholder={aliasForm.type === 'url' ? 'https://example.com/blocklist.txt' : aliasForm.type === 'port' ? '80, 443, 8080-8090' : '192.168.1.0/24, 10.0.0.0/8'}></textarea>
        </div>
        <button type="submit" class="btn-primary">Add Alias</button>
      </form>
    </div>

    <div class="fw-card">
      <h2>Aliases ({aliases.length})</h2>
      {#if aliases.length === 0}
        <div class="no-data">No aliases configured</div>
      {:else}
        <div class="table-wrapper">
          <table>
            <thead>
              <tr><th>Name</th><th>Type</th><th>Entries</th><th>Description</th><th>Status</th><th>Actions</th></tr>
            </thead>
            <tbody>
              {#each aliases as a}
                <tr>
                  <td class="name-cell">{a.name}</td>
                  <td><span class="type-badge" class:type-host={a.type==='host'} class:type-net={a.type==='network'} class:type-port={a.type==='port'} class:type-url={a.type==='url'}>{a.type}</span></td>
                  <td>
                    {#if a.type === 'url' && a.cached_entries?.length}
                      <span class="entry-count">{a.entries.length} URLs, {a.cached_entries.length} cached</span>
                    {:else}
                      <span class="entry-count">{a.entries?.length || 0} entries</span>
                    {/if}
                  </td>
                  <td>{a.description || '-'}</td>
                  <td>
                    <span class="fw-badge-sm" class:fw-badge-on={a.enabled} class:fw-badge-off={!a.enabled}>
                      {a.enabled ? 'ON' : 'OFF'}
                    </span>
                  </td>
                  <td class="ops-cell">
                    {#if a.type === 'url'}
                      <button class="btn-small" on:click={() => refreshAlias(a.name)}>Refresh</button>
                    {/if}
                    <button class="btn-delete-small" on:click={() => deleteAlias(a.name)}>Delete</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: Schedules -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'schedules' && !loading}
    <div class="fw-card">
      <h2>Add Schedule</h2>
      <form on:submit|preventDefault={addSchedule}>
        <div class="form-grid-3">
          <div class="form-group">
            <label>Name</label>
            <input type="text" bind:value={scheduleForm.name} placeholder="e.g. business-hours" />
          </div>
          <div class="form-group">
            <label>Description</label>
            <input type="text" bind:value={scheduleForm.description} placeholder="Optional" />
          </div>
        </div>

        <h3 class="sub-heading">Time Ranges</h3>
        {#each scheduleForm.time_ranges as tr, i}
          <div class="time-range-row">
            <div class="form-group">
              <label>Day</label>
              <select bind:value={tr.day}>
                {#each dayNames as dayName, idx}
                  <option value={idx}>{dayName}</option>
                {/each}
              </select>
            </div>
            <div class="form-group">
              <label>Start</label>
              <input type="time" bind:value={tr.start} />
            </div>
            <div class="form-group">
              <label>End</label>
              <input type="time" bind:value={tr.end} />
            </div>
            <button type="button" class="btn-icon btn-delete-icon" on:click={() => removeTimeRange(i)}>✖</button>
          </div>
        {/each}
        <button type="button" class="btn-small" on:click={addTimeRange}>+ Add Time Range</button>

        <div class="form-actions" style="margin-top: 1rem;">
          <button type="submit" class="btn-primary">Create Schedule</button>
        </div>
      </form>
    </div>

    <div class="fw-card">
      <h2>Schedules ({schedules.length})</h2>
      {#if schedules.length === 0}
        <div class="no-data">No schedules configured</div>
      {:else}
        <div class="table-wrapper">
          <table>
            <thead>
              <tr><th>Name</th><th>Description</th><th>Time Ranges</th><th>Status</th><th>Actions</th></tr>
            </thead>
            <tbody>
              {#each schedules as s}
                <tr>
                  <td class="name-cell">{s.name}</td>
                  <td>{s.description || '-'}</td>
                  <td>
                    {#each s.time_ranges || [] as tr, idx}
                      <span class="time-range-badge">{dayNames[tr.day]?.substring(0, 3)} {tr.start}-{tr.end}</span>
                    {/each}
                  </td>
                  <td>
                    <span class="fw-badge-sm" class:fw-badge-on={s.enabled} class:fw-badge-off={!s.enabled}>
                      {s.enabled ? 'ON' : 'OFF'}
                    </span>
                  </td>
                  <td>
                    <button class="btn-delete-small" on:click={() => deleteSchedule(s.name)}>Delete</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: GeoIP -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'geoip' && !loading}
    <div class="fw-card">
      <h2>GeoIP Blocking</h2>
      <form on:submit|preventDefault={saveGeoip}>
        <div class="form-check-row">
          <label class="check-label">
            <input type="checkbox" bind:checked={geoipForm.enabled} />
            Enable GeoIP blocking
          </label>
        </div>
        <div class="form-grid-3">
          <div class="form-group">
            <label>Default Action</label>
            <select bind:value={geoipForm.default_action}>
              <option value="block">Block</option>
              <option value="pass">Pass</option>
            </select>
          </div>
          <div class="form-group">
            <label>Blocked Countries (ISO codes, comma-separated)</label>
            <input type="text" bind:value={geoipForm.blocked_countries} placeholder="e.g. CN, RU, IR" />
          </div>
          <div class="form-group">
            <label>Allowed Countries (ISO codes, comma-separated)</label>
            <input type="text" bind:value={geoipForm.allowed_countries} placeholder="e.g. US, GB, DE" />
          </div>
        </div>
        <button type="submit" class="btn-primary">Save GeoIP Config</button>
      </form>
    </div>

    {#if geoip.enabled}
      <div class="fw-card">
        <h2>Active GeoIP Configuration</h2>
        <div class="geoip-status">
          <p><strong>Status:</strong> <span class="fw-badge-sm fw-badge-on">ENABLED</span></p>
          <p><strong>Default Action:</strong> {geoip.default_action || 'block'}</p>
          <p><strong>Blocked Countries:</strong>
            {#each (geoip.blocked_countries || []) as cc}
              <span class="country-badge country-blocked">{cc}</span>
            {:else}
              <span class="no-data-inline">None</span>
            {/each}
          </p>
          <p><strong>Allowed Countries:</strong>
            {#each (geoip.allowed_countries || []) as cc}
              <span class="country-badge country-allowed">{cc}</span>
            {:else}
              <span class="no-data-inline">None</span>
            {/each}
          </p>
        </div>
      </div>
    {/if}
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: Traffic Shaper -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'shaper' && !loading}
    <div class="fw-card">
      <h2>Interface Bandwidth</h2>
      <form on:submit|preventDefault={setShaperIface}>
        <div class="form-grid-4">
          <div class="form-group">
            <label>Interface</label>
            <input type="text" bind:value={shaperIfaceForm.interface} placeholder="e.g. GigabitEthernet0/0/0" />
          </div>
          <div class="form-group">
            <label>Total Bandwidth (bits/sec)</label>
            <input type="number" bind:value={shaperIfaceForm.bandwidth} placeholder="e.g. 1000000000" min="1" />
          </div>
          <div class="form-group">
            <label>Download Limit (bits/sec, optional)</label>
            <input type="number" bind:value={shaperIfaceForm.download} placeholder="Optional" />
          </div>
          <div class="form-group">
            <label>Upload Limit (bits/sec, optional)</label>
            <input type="number" bind:value={shaperIfaceForm.upload} placeholder="Optional" />
          </div>
        </div>
        <button type="submit" class="btn-primary">Set Interface Limit</button>
      </form>
    </div>

    <div class="fw-card">
      <h2>Shaper Queues</h2>
      <form on:submit|preventDefault={addShaperQueue}>
        <div class="form-grid-4">
          <div class="form-group">
            <label>Queue Name</label>
            <input type="text" bind:value={shaperQueueForm.name} placeholder="e.g. voip" />
          </div>
          <div class="form-group">
            <label>Weight (1-100)</label>
            <input type="number" bind:value={shaperQueueForm.weight} min="1" max="100" />
          </div>
          <div class="form-group">
            <label>Priority (1=high)</label>
            <input type="number" bind:value={shaperQueueForm.priority} min="1" max="10" />
          </div>
          <div class="form-group">
            <label>DSCP Marking</label>
            <select bind:value={shaperQueueForm.dscp}>
              <option value="">None</option>
              {#each dscpOptions.filter(d => d) as d}
                <option value={d}>{d.toUpperCase()}</option>
              {/each}
            </select>
          </div>
        </div>
        <div class="form-grid-3">
          <div class="form-group">
            <label>Interface (optional)</label>
            <input type="text" bind:value={shaperQueueForm.interface} placeholder="All interfaces" />
          </div>
          <div class="form-group">
            <label>Description</label>
            <input type="text" bind:value={shaperQueueForm.description} placeholder="Optional" />
          </div>
          <div class="form-group form-group-end">
            <button type="submit" class="btn-primary">Add Queue</button>
          </div>
        </div>
      </form>
    </div>

    <!-- Active Shaper State -->
    {#if shaper.interfaces && Object.keys(shaper.interfaces).length > 0}
      <div class="fw-card">
        <h2>Interface Bandwidth Limits</h2>
        <div class="table-wrapper">
          <table>
            <thead><tr><th>Interface</th><th>Bandwidth</th><th>Download</th><th>Upload</th><th>Actions</th></tr></thead>
            <tbody>
              {#each Object.entries(shaper.interfaces) as [iface, info]}
                <tr>
                  <td class="name-cell">{iface}</td>
                  <td>{formatBits((info as any).bandwidth)}</td>
                  <td>{(info as any).download ? formatBits((info as any).download) : '-'}</td>
                  <td>{(info as any).upload ? formatBits((info as any).upload) : '-'}</td>
                  <td><button class="btn-delete-small" on:click={() => removeShaperIface(iface)}>Remove</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}

    {#if shaper.queues && shaper.queues.length > 0}
      <div class="fw-card">
        <h2>Active Queues</h2>
        <div class="table-wrapper">
          <table>
            <thead><tr><th>Queue</th><th>Weight</th><th>Priority</th><th>DSCP</th><th>Interface</th><th>Actions</th></tr></thead>
            <tbody>
              {#each shaper.queues as q}
                <tr>
                  <td class="name-cell">{q.name}</td>
                  <td>{q.weight}%</td>
                  <td>{q.priority}</td>
                  <td>{q.dscp || '-'}</td>
                  <td>{q.interface || 'all'}</td>
                  <td><button class="btn-delete-small" on:click={() => removeShaperQueue(q.name)}>Remove</button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
  {/if}

  <!-- ═══════════════════════════════════════════════════════════════ -->
  <!-- TAB: IDS / Alerts -->
  <!-- ═══════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'ids' && !loading}
    <div class="fw-card">
      <h2>Suricata IDS Configuration</h2>
      <div class="form-check-row">
        <label class="check-label">
          <input type="checkbox" bind:checked={ids.enabled} />
          Enable IDS/IPS
        </label>
      </div>
      <div class="form-grid-2">
        <div class="form-group">
          <label>Monitored Interfaces</label>
          <input type="text" bind:value={idsForm.interfaces} placeholder="e.g. eth0, eth1" />
        </div>
        <div class="form-group form-group-end">
          <button class="btn-primary" on:click={() => toggleIds(!ids.enabled)}>
            {ids.enabled ? 'Disable IDS' : 'Enable IDS'}
          </button>
        </div>
      </div>
    </div>

    <!-- IDS Stats -->
    <div class="fw-card">
      <h2>IDS Statistics</h2>
      <div class="stat-grid">
        <div class="stat-item">
          <span class="stat-label">Status</span>
          <span class="fw-badge-sm" class:fw-badge-on={ids.enabled} class:fw-badge-off={!ids.enabled}>
            {ids.enabled ? 'RUNNING' : 'STOPPED'}
          </span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Rules Loaded</span>
          <span class="stat-value">{ids.stats?.rules_loaded || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Total Alerts</span>
          <span class="stat-value">{ids.stats?.alerts_total || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Blocked</span>
          <span class="stat-value">{ids.stats?.alerts_blocked || 0}</span>
        </div>
        <div class="stat-item">
          <span class="stat-label">Packets Inspected</span>
          <span class="stat-value">{ids.stats?.packets_inspected || 0}</span>
        </div>
      </div>
    </div>

    <!-- Rule Categories -->
    {#if ids.rule_categories && Object.keys(ids.rule_categories).length > 0}
      <div class="fw-card">
        <h2>Rule Categories</h2>
        <div class="category-grid">
          {#each Object.entries(ids.rule_categories) as [cat, enabled]}
            <div class="category-item">
              <span class="cat-name">{cat}</span>
              <span class="fw-badge-sm" class:fw-badge-on={enabled} class:fw-badge-off={!enabled}>
                {enabled ? 'ON' : 'OFF'}
              </span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Alerts -->
    <div class="fw-card">
      <div class="card-header-row">
        <h2>Recent Alerts ({(ids.recent_alerts || []).length})</h2>
        <button class="btn-delete-small" on:click={clearIdsAlerts}>Clear All</button>
      </div>

      {#if !ids.recent_alerts || ids.recent_alerts.length === 0}
        <div class="no-data">No alerts recorded</div>
      {:else}
        <div class="table-wrapper">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Severity</th>
                <th>Category</th>
                <th>Source</th>
                <th>Destination</th>
                <th>Signature</th>
                <th>Blocked</th>
              </tr>
            </thead>
            <tbody>
              {#each ids.recent_alerts as alert}
                <tr>
                  <td class="ts-cell">{alert.timestamp}</td>
                  <td>
                    <span class="severity-badge" style="color: {severityColor(alert.severity)}">
                      {alert.severity?.toUpperCase()}
                    </span>
                  </td>
                  <td>{alert.category}</td>
                  <td class="ip-cell">{alert.src_ip}{alert.src_port ? `:${alert.src_port}` : ''}</td>
                  <td class="ip-cell">{alert.dst_ip}{alert.dst_port ? `:${alert.dst_port}` : ''}</td>
                  <td class="sig-cell">{alert.signature}</td>
                  <td>
                    {#if alert.blocked}
                      <span class="fw-badge-sm fw-badge-on">BLOCKED</span>
                    {:else}
                      <span class="fw-badge-sm fw-badge-alert">LOGGED</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .fw-page { max-width: 1600px; }

  /* Header */
  .fw-header { margin-bottom: 1.5rem; }
  .fw-title-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.75rem; }
  h1 { color: #00ff88; font-size: 1.6rem; margin: 0; }
  .fw-status-right { display: flex; align-items: center; gap: 1rem; font-size: 0.9rem; }
  .fw-badge { padding: 0.2rem 0.7rem; border-radius: 0.3rem; font-weight: 700; font-size: 0.8rem; }
  .fw-badge-on { background: #003322; color: #00ff88; }
  .fw-badge-off { background: #331111; color: #ff4444; }
  .fw-badge-alert { background: #332200; color: #ffaa00; }
  .fw-badge-sm { padding: 0.1rem 0.5rem; border-radius: 0.25rem; font-size: 0.75rem; font-weight: 600; }
  .fw-badge-sm.fw-badge-on { background: #003322; color: #00ff88; }
  .fw-badge-sm.fw-badge-off { background: #331111; color: #ff4444; }
  .fw-badge-sm.fw-badge-alert { background: #332200; color: #ffaa00; }
  .fw-policy { color: #888; }
  .fw-count { color: #aaa; }
  .fw-controls { display: flex; gap: 0.5rem; }

  /* Tabs */
  .fw-tabs { display: flex; gap: 0; margin-bottom: 1.5rem; border-bottom: 2px solid #333; }
  .fw-tab {
    background: none; border: none; color: #888; padding: 0.7rem 1.2rem;
    font-size: 0.9rem; cursor: pointer; border-bottom: 2px solid transparent;
    margin-bottom: -2px; transition: all 0.15s; border-radius: 0;
  }
  .fw-tab:hover { color: #e0e0e0; background: #16213e; }
  .fw-tab-active { color: #00ff88; border-bottom-color: #00ff88; font-weight: 600; }
  .tab-icon { margin-right: 0.3rem; }

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
  .fw-card {
    background: #1a1a2e; padding: 1.5rem; border-radius: 0.75rem; margin-bottom: 1.5rem;
  }
  .fw-card h2 { color: #e0e0e0; font-size: 1.1rem; margin-bottom: 1rem; }
  .card-header-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }
  .card-header-row h2 { margin-bottom: 0; }
  .hint-text { color: #666; font-size: 0.8rem; }
  .sub-heading { color: #aaa; font-size: 0.9rem; margin: 1rem 0 0.5rem; }

  /* Forms */
  .form-grid-4 { display: grid; grid-template-columns: repeat(4, 1fr); gap: 1rem; }
  .form-grid-3 { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; }
  .form-grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
  .form-group { display: flex; flex-direction: column; gap: 0.3rem; margin-bottom: 0.75rem; }
  .form-group-end { display: flex; align-items: flex-end; }
  .form-group label { font-size: 0.8rem; color: #888; }
  .form-check-row { margin: 0.75rem 0; }
  .check-label { color: #e0e0e0; font-size: 0.9rem; display: flex; align-items: center; gap: 0.5rem; cursor: pointer; }
  .check-label input { width: auto; }
  .form-actions { display: flex; gap: 0.75rem; margin-top: 0.5rem; }
  textarea {
    background: #0f0f23; color: #e0e0e0; border: 1px solid #333; padding: 0.6rem;
    border-radius: 0.5rem; font-size: 0.9rem; font-family: monospace; resize: vertical;
  }
  textarea:focus { outline: none; border-color: #00ff88; }

  /* Buttons */
  .btn-on { background: #00ff88; color: #0f0f23; border: none; padding: 0.5rem 1rem; border-radius: 0.4rem; font-weight: 600; cursor: pointer; font-size: 0.85rem; }
  .btn-off { background: #ff4444; color: #fff; border: none; padding: 0.5rem 1rem; border-radius: 0.4rem; font-weight: 600; cursor: pointer; font-size: 0.85rem; }
  .btn-refresh { background: #333; color: #e0e0e0; border: none; padding: 0.5rem 1rem; border-radius: 0.4rem; cursor: pointer; font-size: 0.85rem; }
  .btn-primary { background: #00ff88; color: #0f0f23; border: none; padding: 0.55rem 1.2rem; border-radius: 0.4rem; font-weight: 600; cursor: pointer; font-size: 0.85rem; }
  .btn-primary:hover { opacity: 0.9; }
  .btn-cancel { background: #444; color: #e0e0e0; border: none; padding: 0.55rem 1.2rem; border-radius: 0.4rem; cursor: pointer; font-size: 0.85rem; }
  .btn-small { background: #16213e; color: #88aaff; border: 1px solid #334; padding: 0.3rem 0.7rem; border-radius: 0.3rem; cursor: pointer; font-size: 0.8rem; }
  .btn-delete-small { background: none; border: 1px solid #ff4444; color: #ff4444; padding: 0.3rem 0.6rem; border-radius: 0.3rem; font-size: 0.8rem; cursor: pointer; }
  .btn-delete-small:hover { background: #ff4444; color: #fff; }
  .btn-icon { background: none; border: none; cursor: pointer; font-size: 0.95rem; padding: 0.2rem 0.4rem; border-radius: 0.2rem; }
  .btn-edit { color: #88aaff; }
  .btn-edit:hover { background: #16213e; }
  .btn-delete-icon { color: #ff4444; }
  .btn-delete-icon:hover { background: #331111; }

  /* Loading */
  .fw-loading { color: #888; text-align: center; padding: 3rem; }
  .no-data { color: #666; text-align: center; padding: 2rem; font-size: 0.95rem; }
  .no-data-inline { color: #666; font-style: italic; }

  /* Rules table */
  .rules-table { font-size: 0.85rem; overflow-x: auto; }
  .rule-header, .rule-row {
    display: grid;
    grid-template-columns: 30px 45px 35px 70px 45px 50px 1fr 1fr 120px 90px 1fr 70px;
    gap: 0.3rem; padding: 0.5rem 0.3rem; border-bottom: 1px solid #2a2a3e;
    align-items: center;
  }
  .rule-header { font-weight: 700; color: #666; font-size: 0.75rem; text-transform: uppercase; border-bottom: 2px solid #444; }
  .rule-row { color: #e0e0e0; transition: background 0.15s; }
  .rule-row:hover { background: #16213e; }
  .rule-row.drag-over { background: #1a2a1a; border-top: 2px solid #00ff88; }
  .rule-disabled { opacity: 0.45; }
  .rule-schedule-inactive { border-left: 3px solid #ffaa00; }

  .drag-handle { cursor: grab; color: #555; font-size: 1rem; user-select: none; }
  .drag-handle:hover { color: #888; }

  .toggle-btn {
    background: none; border: 1px solid #555; padding: 0.15rem 0.3rem;
    border-radius: 0.2rem; font-size: 0.65rem; font-weight: 700; cursor: pointer;
    width: 32px; text-align: center;
  }
  .toggle-on { border-color: #00ff88; color: #00ff88; }
  .toggle-off { border-color: #555; color: #666; }

  .col-order { color: #888; font-size: 0.8rem; }

  .action-badge { padding: 0.1rem 0.4rem; border-radius: 0.2rem; font-size: 0.7rem; font-weight: 700; }
  .act-pass { background: #003322; color: #00ff88; }
  .act-block { background: #331111; color: #ff4444; }
  .act-reject { background: #332200; color: #ffaa00; }

  .alias-ref { color: #88aaff; font-weight: 600; margin-right: 0.2rem; }
  .sched-badge { font-size: 0.75rem; padding: 0.1rem 0.4rem; border-radius: 0.2rem; }
  .sched-active { background: #003322; color: #00ff88; }
  .sched-inactive { background: #332200; color: #ffaa00; }
  .sched-always { color: #666; font-size: 0.75rem; }

  /* Generic tables */
  .table-wrapper { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; padding: 0.6rem 0.5rem; border-bottom: 2px solid #444; color: #888; font-size: 0.75rem; text-transform: uppercase; }
  td { padding: 0.6rem 0.5rem; border-bottom: 1px solid #2a2a3e; color: #e0e0e0; }
  .name-cell { font-weight: 600; color: #00ff88; }
  .ops-cell { display: flex; gap: 0.3rem; }
  .type-badge { padding: 0.1rem 0.4rem; border-radius: 0.2rem; font-size: 0.7rem; font-weight: 600; }
  .type-host { background: #16213e; color: #88aaff; }
  .type-net { background: #1a2e1a; color: #88ff88; }
  .type-port { background: #2e2a1a; color: #ffaa88; }
  .type-url { background: #2a1a2e; color: #cc88ff; }
  .entry-count { color: #888; font-size: 0.8rem; }
  .ts-cell { font-family: monospace; font-size: 0.8rem; color: #888; }
  .ip-cell { font-family: monospace; font-size: 0.8rem; }
  .sig-cell { font-size: 0.8rem; max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .severity-badge { font-weight: 700; font-size: 0.8rem; }

  /* Schedule time ranges */
  .time-range-row { display: flex; gap: 1rem; align-items: flex-end; margin-bottom: 0.5rem; }
  .time-range-row .form-group { flex: 0 0 auto; min-width: 120px; margin-bottom: 0; }
  .time-range-badge { background: #16213e; color: #88aaff; padding: 0.1rem 0.4rem; border-radius: 0.2rem; font-size: 0.75rem; margin-right: 0.3rem; display: inline-block; margin-bottom: 0.2rem; }

  /* GeoIP */
  .geoip-status p { margin: 0.4rem 0; }
  .country-badge { padding: 0.15rem 0.4rem; border-radius: 0.2rem; font-size: 0.75rem; font-weight: 600; margin-right: 0.3rem; display: inline-block; }
  .country-blocked { background: #331111; color: #ff4444; }
  .country-allowed { background: #003322; color: #00ff88; }

  /* Stats grid */
  .stat-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 1rem; }
  .stat-item { display: flex; flex-direction: column; gap: 0.3rem; }
  .stat-label { font-size: 0.75rem; color: #888; text-transform: uppercase; }
  .stat-value { font-size: 1.3rem; font-weight: 600; color: #e0e0e0; }

  /* IDS categories */
  .category-grid { display: flex; flex-wrap: wrap; gap: 0.75rem; }
  .category-item { display: flex; align-items: center; gap: 0.5rem; background: #0f0f23; padding: 0.4rem 0.8rem; border-radius: 0.4rem; }
  .cat-name { color: #e0e0e0; font-size: 0.85rem; }

  /* Form overrides */
  :global(.fw-page input), :global(.fw-page select), :global(.fw-page textarea) {
    background: #0f0f23; color: #e0e0e0; border: 1px solid #333;
    padding: 0.5rem; border-radius: 0.4rem; font-size: 0.85rem;
  }
  :global(.fw-page input:focus), :global(.fw-page select:focus), :global(.fw-page textarea:focus) {
    outline: none; border-color: #00ff88;
  }

  @media (max-width: 1200px) {
    .form-grid-4 { grid-template-columns: 1fr 1fr; }
    .rule-header, .rule-row { grid-template-columns: 25px 40px 30px 60px 40px 40px 1fr 1fr 90px 60px; }
    .col-group, .col-desc { display: none; }
  }
  @media (max-width: 800px) {
    .form-grid-4, .form-grid-3, .form-grid-2 { grid-template-columns: 1fr; }
    .fw-tabs { flex-wrap: wrap; }
    .fw-title-row { flex-direction: column; align-items: flex-start; gap: 0.5rem; }
  }
</style>
