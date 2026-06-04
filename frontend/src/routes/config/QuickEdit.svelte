<script lang="ts">
  import { onMount } from 'svelte';

  // ── Types ─────────────────────────────────────────────────────────
  interface ConfigSection {
    key: string;
    label: string;
    icon: string;
    fields: ConfigField[];
  }

  interface ConfigField {
    key: string;
    label: string;
    type: 'text' | 'password' | 'number' | 'select' | 'toggle' | 'textarea';
    placeholder?: string;
    options?: { value: string; label: string }[];
    default?: any;
    min?: number;
    max?: number;
    step?: number;
  }

  // ── State ─────────────────────────────────────────────────────────
  let loading = true;
  let error = '';
  let success = '';
  let configData: any = {};
  let editingSection: string | null = null;
  let editBuffer: Record<string, any> = {};
  let saving = false;
  let expandedSections = new Set<string>(['system', 'network', 'pppoe', 'dhcp', 'dns', 'nat']);

  // History state
  let showHistory = false;
  let history: any[] = [];
  let historyLoading = false;

  // Import/Export state
  let showImportExport = false;
  let importJson = '';
  let importValidation: any = null;
  let importResult: any = null;
  let importLoading = false;

  // ── Section Definitions ───────────────────────────────────────────
  const sections: ConfigSection[] = [
    {
      key: 'system',
      label: 'System Configuration',
      icon: '[SYS]',
      fields: [
        { key: 'hostname', label: 'Hostname', type: 'text', placeholder: 'vectoros-router', default: 'vectoros' },
        { key: 'timezone', label: 'Timezone', type: 'select', default: 'UTC',
          options: [
            { value: 'UTC', label: 'UTC' },
            { value: 'America/New_York', label: 'America/New_York (EST)' },
            { value: 'America/Chicago', label: 'America/Chicago (CST)' },
            { value: 'America/Denver', label: 'America/Denver (MST)' },
            { value: 'America/Los_Angeles', label: 'America/Los_Angeles (PST)' },
            { value: 'Europe/London', label: 'Europe/London (GMT/BST)' },
            { value: 'Europe/Berlin', label: 'Europe/Berlin (CET)' },
            { value: 'Europe/Moscow', label: 'Europe/Moscow (MSK)' },
            { value: 'Asia/Shanghai', label: 'Asia/Shanghai (CST)' },
            { value: 'Asia/Tokyo', label: 'Asia/Tokyo (JST)' },
            { value: 'Asia/Kolkata', label: 'Asia/Kolkata (IST)' },
            { value: 'Australia/Sydney', label: 'Australia/Sydney (AEST)' },
          ]
        },
        { key: 'ntp_server', label: 'NTP Server', type: 'text', placeholder: 'pool.ntp.org', default: 'pool.ntp.org' },
      ]
    },
    {
      key: 'network',
      label: 'Network Configuration',
      icon: '[NET]',
      fields: [
        { key: 'wan_interface', label: 'WAN Interface', type: 'select', default: 'eth0',
          options: [
            { value: 'eth0', label: 'eth0' },
            { value: 'eth1', label: 'eth1' },
            { value: 'enp1s0', label: 'enp1s0' },
            { value: 'enp2s0', label: 'enp2s0' },
            { value: 'ens160', label: 'ens160' },
          ]
        },
        { key: 'lan_interface', label: 'LAN Interface', type: 'select', default: 'eth1',
          options: [
            { value: 'eth0', label: 'eth0' },
            { value: 'eth1', label: 'eth1' },
            { value: 'enp1s0', label: 'enp1s0' },
            { value: 'enp2s0', label: 'enp2s0' },
            { value: 'ens160', label: 'ens160' },
          ]
        },
        { key: 'lan_ip', label: 'LAN IP Address', type: 'text', placeholder: '192.168.1.1', default: '192.168.1.1' },
        { key: 'lan_subnet', label: 'LAN Subnet Mask', type: 'text', placeholder: '255.255.255.0', default: '255.255.255.0' },
      ]
    },
    {
      key: 'pppoe',
      label: 'PPPoE Configuration',
      icon: '[PPP]',
      fields: [
        { key: 'pppoe_username', label: 'Username', type: 'text', placeholder: 'user@isp.com', default: '' },
        { key: 'pppoe_password', label: 'Password', type: 'password', placeholder: 'password', default: '' },
        { key: 'pppoe_interface', label: 'Interface', type: 'select', default: 'eth0',
          options: [
            { value: 'eth0', label: 'eth0 (WAN)' },
            { value: 'eth1', label: 'eth1 (LAN)' },
            { value: 'enp1s0', label: 'enp1s0' },
            { value: 'enp2s0', label: 'enp2s0' },
          ]
        },
        { key: 'pppoe_ac_name', label: 'AC Name (optional)', type: 'text', placeholder: 'Leave empty for any', default: '' },
        { key: 'pppoe_service_name', label: 'Service Name (optional)', type: 'text', placeholder: 'Leave empty for any', default: '' },
      ]
    },
    {
      key: 'dhcp',
      label: 'DHCP Configuration',
      icon: '[DHCP]',
      fields: [
        { key: 'dhcp_enabled', label: 'Enable DHCP Server', type: 'toggle', default: true },
        { key: 'dhcp_range_start', label: 'Range Start', type: 'text', placeholder: '192.168.1.100', default: '192.168.1.100' },
        { key: 'dhcp_range_end', label: 'Range End', type: 'text', placeholder: '192.168.1.200', default: '192.168.1.200' },
        { key: 'dhcp_gateway', label: 'Gateway', type: 'text', placeholder: '192.168.1.1', default: '192.168.1.1' },
        { key: 'dhcp_lease_time', label: 'Lease Time (seconds)', type: 'number', default: 86400, min: 60, max: 2592000, step: 60 },
      ]
    },
    {
      key: 'dns',
      label: 'DNS Configuration',
      icon: '[DNS]',
      fields: [
        { key: 'dns_upstream', label: 'Upstream DNS Servers', type: 'textarea', placeholder: '8.8.8.8\n1.1.1.1', default: '8.8.8.8\n1.1.1.1' },
        { key: 'dns_cache_size', label: 'Cache Size', type: 'number', default: 1000, min: 0, max: 100000, step: 100 },
      ]
    },
    {
      key: 'nat',
      label: 'NAT Configuration',
      icon: '[NAT]',
      fields: [
        { key: 'nat_enabled', label: 'Enable NAT', type: 'toggle', default: true },
        { key: 'nat_inside_interface', label: 'Inside Interface', type: 'select', default: 'eth1',
          options: [
            { value: 'eth0', label: 'eth0' },
            { value: 'eth1', label: 'eth1' },
            { value: 'enp1s0', label: 'enp1s0' },
            { value: 'enp2s0', label: 'enp2s0' },
          ]
        },
        { key: 'nat_outside_interface', label: 'Outside Interface', type: 'select', default: 'eth0',
          options: [
            { value: 'eth0', label: 'eth0' },
            { value: 'eth1', label: 'eth1' },
            { value: 'enp1s0', label: 'enp1s0' },
            { value: 'enp2s0', label: 'enp2s0' },
          ]
        },
      ]
    },
  ];

  // ── Lifecycle ─────────────────────────────────────────────────────
  onMount(async () => {
    await loadConfig();
  });

  // ── Data Loading ──────────────────────────────────────────────────
  async function loadConfig() {
    loading = true;
    error = '';
    try {
      const res = await fetch('/api/config/status');
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        configData = data.config || data;
      }
    } catch (e) {
      error = 'Failed to load configuration';
    } finally {
      loading = false;
    }
  }

  // ── Section Helpers ───────────────────────────────────────────────
  function getFieldValue(field: ConfigField): any {
    const val = configData[field.key];
    if (val === undefined || val === null) return field.default ?? '';
    return val;
  }

  function startEditing(sectionKey: string) {
    editingSection = sectionKey;
    const section = sections.find(s => s.key === sectionKey)!;
    editBuffer = {};
    for (const field of section.fields) {
      editBuffer[field.key] = getFieldValue(field);
    }
  }

  function cancelEditing() {
    editingSection = null;
    editBuffer = {};
  }

  async function saveSection(sectionKey: string) {
    saving = true;
    error = '';
    success = '';
    try {
      const section = sections.find(s => s.key === sectionKey)!;
      const payload: Record<string, any> = {};
      for (const field of section.fields) {
        payload[field.key] = editBuffer[field.key];
      }

      const res = await fetch('/api/config/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ...configData, ...payload })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = `${section.label} saved successfully`;
        configData = { ...configData, ...payload };
        editingSection = null;
        editBuffer = {};
      }
    } catch (e) {
      error = `Failed to save ${sectionKey} configuration`;
    } finally {
      saving = false;
    }
  }

  function resetSection(sectionKey: string) {
    const section = sections.find(s => s.key === sectionKey)!;
    for (const field of section.fields) {
      editBuffer[field.key] = field.default ?? '';
    }
  }

  function resetAllDefaults() {
    for (const section of sections) {
      for (const field of section.fields) {
        configData[field.key] = field.default ?? '';
      }
    }
    editingSection = null;
    editBuffer = {};
  }

  async function saveAll() {
    saving = true;
    error = '';
    success = '';
    try {
      const res = await fetch('/api/config/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(configData)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = 'All configuration saved successfully';
      }
    } catch (e) {
      error = 'Failed to save configuration';
    } finally {
      saving = false;
    }
  }

  // ── History ───────────────────────────────────────────────────────
  async function loadHistory() {
    historyLoading = true;
    try {
      const res = await fetch('/api/config/history');
      const data = await res.json();
      history = data.history || [];
    } catch (e) {
      // Ignore
    } finally {
      historyLoading = false;
    }
  }

  async function rollback(version: string) {
    error = '';
    success = '';
    try {
      const res = await fetch(`/api/config/rollback/${version}`, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        success = `Rolled back to version ${version}`;
        await loadConfig();
        await loadHistory();
      }
    } catch (e) {
      error = 'Failed to rollback';
    }
  }

  // ── Import/Export ─────────────────────────────────────────────────
  async function handleExportJson() {
    error = '';
    try {
      const blob = new Blob([JSON.stringify(configData, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `vectoros-config-${new Date().toISOString().slice(0, 10)}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) {
      error = 'Failed to export configuration';
    }
  }

  function handleFileUpload(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (e) => {
      importJson = (e.target?.result as string) || '';
      importValidation = null;
      importResult = null;
    };
    reader.readAsText(file);
  }

  async function validateImport() {
    if (!importJson.trim()) {
      error = 'Please paste or upload a config file first';
      return;
    }
    error = '';
    importValidation = null;
    try {
      const parsed = JSON.parse(importJson);
      const requiredSections = ['system', 'network', 'pppoe', 'dhcp', 'dns', 'nat'];
      const foundSections = requiredSections.filter(s => parsed[s] || parsed[`${s}_enabled`] || parsed[`pppoe_${s}`]);
      importValidation = {
        valid: true,
        sections_found: foundSections,
        message: `Valid JSON with ${Object.keys(parsed).length} top-level keys`
      };
    } catch (e) {
      importValidation = { valid: false, message: 'Invalid JSON format' };
    }
  }

  async function handleImport() {
    if (!importJson.trim()) {
      error = 'Please paste or upload a config file first';
      return;
    }
    importLoading = true;
    error = '';
    importResult = null;
    try {
      const parsed = JSON.parse(importJson);
      const res = await fetch('/api/config/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(parsed)
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        importResult = { status: 'ok', message: 'Configuration imported successfully' };
        await loadConfig();
      }
    } catch (e) {
      error = 'Failed to import configuration';
    } finally {
      importLoading = false;
    }
  }

  // ── UI Helpers ────────────────────────────────────────────────────
  function toggleSection(sectionKey: string) {
    if (expandedSections.has(sectionKey)) {
      expandedSections.delete(sectionKey);
    } else {
      expandedSections.add(sectionKey);
    }
    expandedSections = expandedSections;
  }

  function clearMessages() {
    error = '';
    success = '';
  }

  function formatValue(val: any): string {
    if (val === null || val === undefined) return '(not set)';
    if (typeof val === 'boolean') return val ? 'Enabled' : 'Disabled';
    if (typeof val === 'string' && val.includes('\n')) return val.split('\n').join(', ');
    return String(val);
  }
</script>

<div class="quick-edit">
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

  <!-- Toolbar -->
  <div class="toolbar">
    <div class="toolbar-left">
      <button class="btn btn-primary" on:click={saveAll} disabled={saving || loading}>
        {saving ? 'Saving...' : 'Save All'}
      </button>
      <button class="btn btn-secondary" on:click={loadConfig} disabled={loading}>
        {loading ? 'Loading...' : 'Reload'}
      </button>
      <button class="btn btn-secondary btn-danger-outline" on:click={resetAllDefaults}>
        Reset Defaults
      </button>
    </div>
    <div class="toolbar-right">
      <button class="btn btn-secondary" on:click={() => { showHistory = !showHistory; if (showHistory) loadHistory(); }}>
        {showHistory ? 'Hide History' : 'History'}
      </button>
      <button class="btn btn-secondary" on:click={() => showImportExport = !showImportExport}>
        {showImportExport ? 'Hide Import/Export' : 'Import/Export'}
      </button>
    </div>
  </div>

  {#if loading}
    <div class="loading">Loading configuration...</div>
  {:else}
    <!-- Config Sections -->
    <div class="sections">
      {#each sections as section}
        <div class="section" class:expanded={expandedSections.has(section.key)} class:editing={editingSection === section.key}>
          <div class="section-header" on:click={() => toggleSection(section.key)}>
            <span class="section-icon">{section.icon}</span>
            <span class="section-label">{section.label}</span>
            <span class="section-chevron">{expandedSections.has(section.key) ? 'v' : '>'}</span>
          </div>

          {#if expandedSections.has(section.key)}
            <div class="section-body">
              <!-- Display current values -->
              <div class="current-values">
                <div class="values-grid">
                  {#each section.fields as field}
                    <div class="value-item">
                      <span class="value-label">{field.label}</span>
                      <span class="value-data">{formatValue(getFieldValue(field))}</span>
                    </div>
                  {/each}
                </div>
              </div>

              <!-- Edit Form -->
              {#if editingSection === section.key}
                <div class="edit-form">
                  <h4>Edit {section.label}</h4>
                  {#each section.fields as field}
                    <div class="form-group">
                      <label for="{section.key}-{field.key}">{field.label}</label>
                      {#if field.type === 'toggle'}
                        <label class="toggle-label">
                          <span class="toggle-switch" class:enabled={editBuffer[field.key]}>
                            <input
                              type="checkbox"
                              id="{section.key}-{field.key}"
                              bind:checked={editBuffer[field.key]}
                            />
                            <span class="toggle-slider"></span>
                          </span>
                          <span>{editBuffer[field.key] ? 'Enabled' : 'Disabled'}</span>
                        </label>
                      {:else if field.type === 'select'}
                        <select id="{section.key}-{field.key}" bind:value={editBuffer[field.key]}>
                          {#each field.options || [] as opt}
                            <option value={opt.value}>{opt.label}</option>
                          {/each}
                        </select>
                      {:else if field.type === 'textarea'}
                        <textarea
                          id="{section.key}-{field.key}"
                          bind:value={editBuffer[field.key]}
                          placeholder={field.placeholder || ''}
                          rows="3"
                          class="textarea-field"
                        ></textarea>
                      {:else}
                        <input
                          type={field.type}
                          id="{section.key}-{field.key}"
                          bind:value={editBuffer[field.key]}
                          placeholder={field.placeholder || ''}
                          min={field.min}
                          max={field.max}
                          step={field.step}
                        />
                      {/if}
                    </div>
                  {/each}
                  <div class="form-actions">
                    <button class="btn btn-primary" on:click={() => saveSection(section.key)} disabled={saving}>
                      {saving ? 'Saving...' : 'Save'}
                    </button>
                    <button class="btn btn-secondary" on:click={() => resetSection(section.key)}>
                      Reset
                    </button>
                    <button class="btn btn-secondary btn-cancel" on:click={cancelEditing}>
                      Cancel
                    </button>
                  </div>
                </div>
              {:else}
                <div class="section-actions">
                  <button class="btn btn-sm btn-primary" on:click={() => startEditing(section.key)}>
                    Edit
                  </button>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <!-- Configuration History Panel -->
    {#if showHistory}
      <div class="panel">
        <div class="panel-header">
          <h3>Configuration History</h3>
          <button class="btn btn-sm btn-secondary" on:click={loadHistory} disabled={historyLoading}>
            {historyLoading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
        {#if history.length === 0}
          <p class="no-data">No configuration history available.</p>
        {:else}
          <div class="history-list">
            {#each [...history].reverse() as entry}
              <div class="history-entry">
                <div class="history-meta">
                  <span class="history-version">{entry.version}</span>
                  <span class="history-time">{entry.timestamp}</span>
                </div>
                <div class="history-message">{entry.message || '(no message)'}</div>
                <button class="btn btn-sm btn-warning" on:click={() => rollback(entry.version)}>
                  Rollback
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Import/Export Panel -->
    {#if showImportExport}
      <div class="panel">
        <div class="panel-header">
          <h3>Import / Export Configuration</h3>
        </div>
        <div class="import-export-grid">
          <!-- Export -->
          <div class="ie-card">
            <h4>Export as JSON</h4>
            <p class="ie-desc">Download the current configuration as a JSON file.</p>
            <button class="btn btn-primary" on:click={handleExportJson}>Export & Download</button>
          </div>

          <!-- Import -->
          <div class="ie-card">
            <h4>Import from JSON</h4>
            <p class="ie-desc">Upload a JSON configuration file or paste JSON below.</p>
            <div class="form-group">
              <label>Select File</label>
              <input type="file" accept=".json" on:change={handleFileUpload} />
            </div>
            <div class="form-group">
              <label>Or Paste JSON</label>
              <textarea
                bind:value={importJson}
                rows="6"
                placeholder='{"hostname": "vectoros", ...}'
                class="textarea-field"
              ></textarea>
            </div>
            <div class="ie-actions">
              <button class="btn btn-secondary" on:click={validateImport} disabled={!importJson.trim()}>
                Validate
              </button>
              <button class="btn btn-primary" on:click={handleImport} disabled={!importJson.trim() || importLoading}>
                {importLoading ? 'Importing...' : 'Import'}
              </button>
            </div>

            {#if importValidation}
              <div class="validation-result" class:valid={importValidation.valid} class:invalid={!importValidation.valid}>
                <span class="status-badge" class:badge-ok={importValidation.valid} class:badge-error={!importValidation.valid}>
                  {importValidation.valid ? 'Valid' : 'Invalid'}
                </span>
                <span>{importValidation.message}</span>
                {#if importValidation.sections_found?.length > 0}
                  <span>Sections: {importValidation.sections_found.join(', ')}</span>
                {/if}
              </div>
            {/if}

            {#if importResult}
              <div class="import-result">
                <span class="status-badge badge-ok">{importResult.status}</span>
                <span>{importResult.message}</span>
              </div>
            {/if}
          </div>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .quick-edit {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  /* ── Banners ─────────────────────────────────────── */
  .error-banner {
    background: #ff444422;
    border: 1px solid #ff4444;
    color: #ff8888;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .success-banner {
    background: #00ff8822;
    border: 1px solid #00ff88;
    color: #00ff88;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
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

  /* ── Toolbar ─────────────────────────────────────── */
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .toolbar-left, .toolbar-right {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  /* ── Buttons ─────────────────────────────────────── */
  .btn {
    padding: 0.5rem 1rem;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    transition: all 0.2s;
    background: #333;
    color: #e0e0e0;
  }

  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn:hover:not(:disabled) { opacity: 0.9; }

  .btn-primary { background: #00ff88; color: #0f0f23; font-weight: 600; }
  .btn-secondary { background: #333; color: #e0e0e0; border: 1px solid #555; }
  .btn-secondary:hover:not(:disabled) { border-color: #00ff88; }
  .btn-warning { background: #ffaa00; color: #0f0f23; font-weight: 600; }
  .btn-danger-outline { border-color: #ff4444; color: #ff6666; }
  .btn-danger-outline:hover:not(:disabled) { background: #ff444422; border-color: #ff4444; }
  .btn-cancel { border-color: #555; }
  .btn-sm { padding: 0.3rem 0.6rem; font-size: 0.8rem; }

  .loading {
    color: #888;
    text-align: center;
    padding: 3rem;
  }

  /* ── Sections ────────────────────────────────────── */
  .sections {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section {
    background: #1a1a2e;
    border-radius: 0.75rem;
    overflow: hidden;
    border: 1px solid #333;
  }

  .section.editing {
    border-color: #00ff8840;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    cursor: pointer;
    transition: background 0.2s;
  }

  .section-header:hover {
    background: #16213e;
  }

  .section-icon {
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.8rem;
    color: #00ff88;
    font-weight: bold;
    min-width: 3rem;
  }

  .section-label {
    flex: 1;
    font-size: 1rem;
    font-weight: 600;
    color: #e0e0e0;
  }

  .section-chevron {
    color: #666;
    font-size: 0.8rem;
    transition: transform 0.2s;
  }

  .section-body {
    padding: 0 1.25rem 1.25rem;
  }

  /* ── Current Values ──────────────────────────────── */
  .current-values {
    background: #0f0f23;
    padding: 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
  }

  .values-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
  }

  .value-item {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .value-label {
    font-size: 0.75rem;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .value-data {
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.9rem;
    color: #e0e0e0;
    word-break: break-all;
  }

  /* ── Edit Form ───────────────────────────────────── */
  .edit-form {
    background: #16213e;
    padding: 1.25rem;
    border-radius: 0.5rem;
    border-left: 3px solid #00ff88;
  }

  .edit-form h4 {
    margin: 0 0 1rem 0;
    color: #00ff88;
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-bottom: 0.75rem;
  }

  label {
    font-size: 0.8rem;
    color: #888;
  }

  input, select {
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.5rem;
    border-radius: 0.4rem;
    font-size: 0.9rem;
    font-family: 'Monaco', 'Menlo', monospace;
  }

  input:focus, select:focus {
    outline: none;
    border-color: #00ff88;
  }

  .textarea-field {
    width: 100%;
    background: #0f0f23;
    color: #e0e0e0;
    border: 1px solid #333;
    padding: 0.5rem;
    border-radius: 0.4rem;
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.9rem;
    resize: vertical;
  }

  .textarea-field:focus {
    outline: none;
    border-color: #00ff88;
  }

  .form-actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 1rem;
  }

  .section-actions {
    display: flex;
    gap: 0.5rem;
  }

  /* ── Toggle Switch ───────────────────────────────── */
  .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    cursor: pointer;
    font-size: 0.9rem;
    color: #e0e0e0;
  }

  .toggle-switch {
    position: relative;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
  }

  .toggle-switch input {
    opacity: 0;
    width: 0;
    height: 0;
    padding: 0;
    border: none;
  }

  .toggle-slider {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: #444;
    border-radius: 12px;
    transition: background 0.3s;
    cursor: pointer;
  }

  .toggle-slider::before {
    content: '';
    position: absolute;
    width: 18px;
    height: 18px;
    left: 3px;
    top: 3px;
    background: #e0e0e0;
    border-radius: 50%;
    transition: transform 0.3s;
  }

  .toggle-switch.enabled .toggle-slider { background: #00ff88; }
  .toggle-switch.enabled .toggle-slider::before { transform: translateX(20px); }

  /* ── History Panel ───────────────────────────────── */
  .panel {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    border: 1px solid #333;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .panel-header h3 {
    margin: 0;
    color: #e0e0e0;
    font-size: 1.05rem;
  }

  .no-data {
    color: #666;
    text-align: center;
    padding: 2rem;
  }

  .history-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .history-entry {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background: #0f0f23;
    border-radius: 0.5rem;
    border: 1px solid #333;
  }

  .history-meta {
    display: flex;
    flex-direction: column;
    min-width: 140px;
  }

  .history-version {
    font-family: monospace;
    color: #00ff88;
    font-size: 0.9rem;
  }

  .history-time {
    color: #666;
    font-size: 0.8rem;
  }

  .history-message {
    flex: 1;
    color: #ccc;
    font-size: 0.9rem;
  }

  /* ── Import/Export ───────────────────────────────── */
  .import-export-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.5rem;
  }

  .ie-card {
    background: #16213e;
    padding: 1.25rem;
    border-radius: 0.5rem;
  }

  .ie-card h4 {
    margin: 0 0 0.5rem 0;
    color: #e0e0e0;
    font-size: 1rem;
  }

  .ie-desc {
    color: #888;
    font-size: 0.85rem;
    margin: 0 0 1rem 0;
  }

  .ie-actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  .validation-result, .import-result {
    margin-top: 0.75rem;
    padding: 0.6rem 0.8rem;
    border-radius: 0.4rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
    flex-wrap: wrap;
  }

  .validation-result.valid { background: #00ff8811; border: 1px solid #00ff8840; color: #00ff88; }
  .validation-result.invalid { background: #ff444411; border: 1px solid #ff444440; color: #ff6666; }
  .import-result { background: #00ff8811; border: 1px solid #00ff8840; color: #00ff88; }

  .status-badge {
    padding: 0.15rem 0.5rem;
    border-radius: 0.3rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .badge-ok { background: #003322; color: #00ff88; }
  .badge-error { background: #331111; color: #ff4444; }
</style>
