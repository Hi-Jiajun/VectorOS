<script lang="ts">
  import { onMount } from 'svelte';

  // State
  let configTree: any = null;
  let stagingTree: any = null;
  let history: any[] = [];
  let templates: any[] = [];
  let diff: any[] = [];
  let loading = true;
  let error = '';
  let activeTab: 'tree' | 'diff' | 'history' | 'templates' | 'cli' = 'tree';

  // Set form
  let setPath = '';
  let setValue = '';

  // Delete form
  let deletePath = '';

  // Commit form
  let commitMessage = '';

  // Rollback form
  let rollbackVersion = '';

  // Template form
  let templateName = '';
  let templateDesc = '';

  // CLI
  let cliSessionId = '';
  let cliCommand = '';
  let cliHistory: Array<{ cmd: string; output: string; status: string }> = [];

  // Expanded nodes in tree view
  let expandedNodes = new Set<string>();

  onMount(async () => {
    await loadAll();
  });

  async function loadAll() {
    loading = true;
    error = '';
    try {
      const [treeRes, stagingRes, histRes, tplRes, diffRes] = await Promise.all([
        fetch('/api/config/tree').then(r => r.json()),
        fetch('/api/config/staging').then(r => r.json()),
        fetch('/api/config/history').then(r => r.json()),
        fetch('/api/config/templates').then(r => r.json()),
        fetch('/api/config/diff').then(r => r.json()),
      ]);

      configTree = treeRes.tree || treeRes;
      stagingTree = stagingRes.staging;
      history = histRes.history || [];
      templates = tplRes.templates || [];
      diff = diffRes.diff || [];
    } catch (e) {
      error = 'Failed to load configuration data';
    } finally {
      loading = false;
    }
  }

  // ── Set value ──────────────────────────────────────────────────

  async function handleSet() {
    if (!setPath) return;
    error = '';
    try {
      // Parse value as JSON if possible, otherwise use raw string
      let parsedValue: any;
      try {
        parsedValue = JSON.parse(setValue);
      } catch {
        // Try common type conversions
        if (setValue === 'true') parsedValue = true;
        else if (setValue === 'false') parsedValue = false;
        else if (!isNaN(Number(setValue)) && setValue !== '') parsedValue = Number(setValue);
        else parsedValue = setValue;
      }

      const res = await fetch('/api/config/set', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: setPath, value: parsedValue })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        setPath = '';
        setValue = '';
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to set value';
    }
  }

  // ── Delete value ───────────────────────────────────────────────

  async function handleDelete() {
    if (!deletePath) return;
    error = '';
    try {
      const res = await fetch('/api/config/delete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: deletePath })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        deletePath = '';
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to delete value';
    }
  }

  // ── Commit ─────────────────────────────────────────────────────

  async function handleCommit() {
    error = '';
    try {
      const res = await fetch('/api/config/commit', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        commitMessage = '';
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to commit';
    }
  }

  // ── Discard ────────────────────────────────────────────────────

  async function handleDiscard() {
    error = '';
    try {
      const res = await fetch('/api/config/discard', { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to discard';
    }
  }

  // ── Rollback ───────────────────────────────────────────────────

  async function handleRollback() {
    if (!rollbackVersion) return;
    error = '';
    try {
      const res = await fetch(`/api/config/rollback/${rollbackVersion}`, { method: 'POST' });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        rollbackVersion = '';
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to rollback';
    }
  }

  // ── Templates ──────────────────────────────────────────────────

  async function handleSaveTemplate() {
    if (!templateName) return;
    error = '';
    try {
      const res = await fetch('/api/config/template/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: templateName, description: templateDesc })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        templateName = '';
        templateDesc = '';
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to save template';
    }
  }

  async function handleApplyTemplate(name: string) {
    error = '';
    try {
      const res = await fetch('/api/config/template/apply', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name })
      });
      const data = await res.json();
      if (data.error) {
        error = data.error;
      } else {
        await loadAll();
      }
    } catch (e) {
      error = 'Failed to apply template';
    }
  }

  // ── CLI ────────────────────────────────────────────────────────

  async function initCliSession() {
    try {
      const res = await fetch('/api/config/cli/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({})
      });
      const data = await res.json();
      cliSessionId = data.session?.id || '';
    } catch (e) {
      error = 'Failed to create CLI session';
    }
  }

  async function executeCliCommand() {
    if (!cliCommand.trim()) return;

    const cmd = cliCommand.trim();
    cliCommand = '';

    if (!cliSessionId) {
      await initCliSession();
    }

    try {
      const res = await fetch('/api/config/cli/execute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: cliSessionId, command: cmd })
      });
      const data = await res.json();

      cliHistory = [...cliHistory, {
        cmd,
        output: data.message || data.error || JSON.stringify(data),
        status: data.status || 'ok'
      }];

      // Scroll to bottom
      setTimeout(() => {
        const terminal = document.querySelector('.terminal-output');
        if (terminal) terminal.scrollTop = terminal.scrollHeight;
      }, 10);
    } catch (e) {
      cliHistory = [...cliHistory, { cmd, output: 'Connection error', status: 'error' }];
    }
  }

  function handleCliKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      executeCliCommand();
    }
  }

  // ── Tree helpers ───────────────────────────────────────────────

  function toggleNode(path: string) {
    if (expandedNodes.has(path)) {
      expandedNodes.delete(path);
    } else {
      expandedNodes.add(path);
    }
    expandedNodes = expandedNodes; // trigger reactivity
  }

  function isExpanded(path: string): boolean {
    return expandedNodes.has(path);
  }

  function setQuickPath(path: string) {
    setPath = path;
    activeTab = 'tree';
  }

  function deleteQuickPath(path: string) {
    deletePath = path;
    activeTab = 'tree';
  }

  function formatValue(val: any): string {
    if (val === null || val === undefined) return 'null';
    if (typeof val === 'boolean') return val ? 'true' : 'false';
    if (typeof val === 'string') return `"${val}"`;
    return String(val);
  }
</script>

<svelte:head>
  <title>VectorOS - Configuration Management</title>
</svelte:head>

<div class="config-page">
  <h1>Configuration Management</h1>

  {#if error}
    <div class="error-card">
      <span>{error}</span>
      <button class="error-dismiss" on:click={() => error = ''}>x</button>
    </div>
  {/if}

  <!-- Tab Navigation -->
  <div class="tab-bar">
    <button class="tab" class:active={activeTab === 'tree'} on:click={() => activeTab = 'tree'}>
      Config Tree
    </button>
    <button class="tab" class:active={activeTab === 'diff'} on:click={() => activeTab = 'diff'}>
      Diff {#if diff.length > 0}<span class="tab-badge">{diff.length}</span>{/if}
    </button>
    <button class="tab" class:active={activeTab === 'history'} on:click={() => activeTab = 'history'}>
      History
    </button>
    <button class="tab" class:active={activeTab === 'templates'} on:click={() => activeTab = 'templates'}>
      Templates
    </button>
    <button class="tab" class:active={activeTab === 'cli'} on:click={() => activeTab = 'cli'}>
      CLI Terminal
    </button>
  </div>

  {#if loading}
    <div class="loading">Loading configuration...</div>
  {:else}

    <!-- ═══════════════ Config Tree Tab ═══════════════ -->

    {#if activeTab === 'tree'}
      <div class="panel-grid">
        <!-- Left: Tree View -->
        <div class="tree-panel">
          <div class="panel-header">
            <h2>Configuration Tree</h2>
            <div class="panel-actions">
              {#if stagingTree}
                <span class="staging-badge">Staged changes pending</span>
              {/if}
              <button class="btn-sm" on:click={loadAll}>Refresh</button>
            </div>
          </div>

          <div class="tree-view">
            {#if configTree && typeof configTree === 'object'}
              {#each Object.entries(configTree).sort(([a], [b]) => a.localeCompare(b)) as [key, value]}
                {#if typeof value === 'object' && value !== null && !Array.isArray(value)}
                  <div class="tree-node">
                    <div class="tree-row clickable" on:click={() => toggleNode(key)}>
                      <span class="tree-icon">{isExpanded(key) ? 'v' : '>'}</span>
                      <span class="tree-key">{key}</span>
                      <span class="tree-branch-count">({Object.keys(value).length})</span>
                    </div>
                    {#if isExpanded(key)}
                      <div class="tree-children">
                        {#each Object.entries(value).sort(([a], [b]) => a.localeCompare(b)) as [childKey, childValue]}
                          {#if typeof childValue === 'object' && childValue !== null && !Array.isArray(childValue)}
                            <div class="tree-node">
                              <div class="tree-row clickable" on:click={() => toggleNode(`${key}.${childKey}`)}>
                                <span class="tree-icon">{isExpanded(`${key}.${childKey}`) ? 'v' : '>'}</span>
                                <span class="tree-key">{childKey}</span>
                                <span class="tree-branch-count">({Object.keys(childValue).length})</span>
                              </div>
                              {#if isExpanded(`${key}.${childKey}`)}
                                <div class="tree-children">
                                  {#each Object.entries(childValue).sort(([a], [b]) => a.localeCompare(b)) as [leafKey, leafValue]}
                                    <div class="tree-leaf">
                                      <span class="tree-connector"></span>
                                      <span class="tree-key">{leafKey}</span>
                                      <span class="tree-eq"> = </span>
                                      <span class="tree-value">{formatValue(leafValue)}</span>
                                      <span class="tree-actions">
                                        <button class="btn-tiny" on:click={() => setQuickPath(`${key}.${childKey}.${leafKey}`)} title="Edit">e</button>
                                        <button class="btn-tiny btn-tiny-danger" on:click={() => deleteQuickPath(`${key}.${childKey}.${leafKey}`)} title="Delete">x</button>
                                      </span>
                                    </div>
                                  {/each}
                                </div>
                              {/if}
                            </div>
                          {:else}
                            <div class="tree-leaf">
                              <span class="tree-connector"></span>
                              <span class="tree-key">{childKey}</span>
                              <span class="tree-eq"> = </span>
                              <span class="tree-value">{formatValue(childValue)}</span>
                              <span class="tree-actions">
                                <button class="btn-tiny" on:click={() => setQuickPath(`${key}.${childKey}`)} title="Edit">e</button>
                                <button class="btn-tiny btn-tiny-danger" on:click={() => deleteQuickPath(`${key}.${childKey}`)} title="Delete">x</button>
                              </span>
                            </div>
                          {/if}
                        {/each}
                      </div>
                    {/if}
                  </div>
                {:else}
                  <div class="tree-leaf tree-top-level">
                    <span class="tree-key">{key}</span>
                    <span class="tree-eq"> = </span>
                    <span class="tree-value">{formatValue(value)}</span>
                    <span class="tree-actions">
                      <button class="btn-tiny" on:click={() => setQuickPath(key)} title="Edit">e</button>
                      <button class="btn-tiny btn-tiny-danger" on:click={() => deleteQuickPath(key)} title="Delete">x</button>
                    </span>
                  </div>
                {/if}
              {/each}
            {:else}
              <p class="no-data">No configuration loaded</p>
            {/if}
          </div>
        </div>

        <!-- Right: Operations Panel -->
        <div class="ops-panel">
          <!-- Set Value -->
          <div class="ops-card">
            <h3>Set Value</h3>
            <form on:submit|preventDefault={handleSet}>
              <div class="form-group">
                <label>Path (dot-separated)</label>
                <input type="text" bind:value={setPath} placeholder="e.g. interfaces.eth0.mtu" />
              </div>
              <div class="form-group">
                <label>Value</label>
                <input type="text" bind:value={setValue} placeholder="e.g. 1500" />
              </div>
              <button type="submit" class="btn-primary" disabled={!setPath}>Set</button>
            </form>
          </div>

          <!-- Delete Value -->
          <div class="ops-card">
            <h3>Delete Value</h3>
            <form on:submit|preventDefault={handleDelete}>
              <div class="form-group">
                <label>Path (dot-separated)</label>
                <input type="text" bind:value={deletePath} placeholder="e.g. pppoe.username" />
              </div>
              <button type="submit" class="btn-danger" disabled={!deletePath}>Delete</button>
            </form>
          </div>

          <!-- Commit / Discard -->
          <div class="ops-card">
            <h3>Commit Changes</h3>
            <p class="ops-desc">Apply staged changes to the active configuration.</p>
            <div class="button-row">
              <button class="btn-primary" on:click={handleCommit}>Commit</button>
              <button class="btn-secondary" on:click={handleDiscard}>Discard</button>
            </div>
          </div>

          <!-- Rollback -->
          <div class="ops-card">
            <h3>Rollback</h3>
            <form on:submit|preventDefault={handleRollback}>
              <div class="form-group">
                <label>Version Hash</label>
                <input type="text" bind:value={rollbackVersion} placeholder="e.g. a1b2c3d4e5f6" />
              </div>
              <button type="submit" class="btn-warning" disabled={!rollbackVersion}>Rollback</button>
            </form>
          </div>
        </div>
      </div>
    {/if}

    <!-- ═══════════════ Diff Tab ═══════════════ -->

    {#if activeTab === 'diff'}
      <div class="diff-panel">
        <div class="panel-header">
          <h2>Configuration Diff</h2>
          <span class="diff-count">{diff.length} change(s)</span>
        </div>

        {#if diff.length === 0}
          <div class="no-data">No differences between committed and staged configuration.</div>
        {:else}
          <div class="diff-view">
            {#each diff as entry}
              <div class="diff-entry" class:diff-set={entry.op === 'set'} class:diff-delete={entry.op === 'delete'} class:diff-update={entry.op === 'update'}>
                <span class="diff-op">{entry.op}</span>
                <span class="diff-path">{entry.path}</span>
                {#if entry.op === 'set'}
                  <span class="diff-value">= {formatValue(entry.value)}</span>
                {:else if entry.op === 'update'}
                  <span class="diff-old">{formatValue(entry.old)}</span>
                  <span class="diff-arrow"> -> </span>
                  <span class="diff-new">{formatValue(entry.new)}</span>
                {:else if entry.op === 'delete'}
                  <span class="diff-old">(removed)</span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- ═══════════════ History Tab ═══════════════ -->

    {#if activeTab === 'history'}
      <div class="history-panel">
        <div class="panel-header">
          <h2>Configuration History</h2>
          <span class="history-count">{history.length} version(s)</span>
        </div>

        {#if history.length === 0}
          <div class="no-data">No configuration history yet.</div>
        {:else}
          <div class="history-list">
            {#each [...history].reverse() as entry}
              <div class="history-entry">
                <div class="history-meta">
                  <span class="history-version">{entry.version}</span>
                  <span class="history-time">{entry.timestamp}</span>
                </div>
                <div class="history-message">{entry.message || '(no message)'}</div>
                <div class="history-actions">
                  <button class="btn-sm" on:click={() => { rollbackVersion = entry.version; handleRollback(); }}>
                    Rollback to this
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- ═══════════════ Templates Tab ═══════════════ -->

    {#if activeTab === 'templates'}
      <div class="templates-panel">
        <div class="panel-header">
          <h2>Configuration Templates</h2>
        </div>

        <!-- Save Template -->
        <div class="ops-card">
          <h3>Save Current Config as Template</h3>
          <form on:submit|preventDefault={handleSaveTemplate}>
            <div class="form-row">
              <div class="form-group">
                <label>Template Name</label>
                <input type="text" bind:value={templateName} placeholder="e.g. basic-router" />
              </div>
              <div class="form-group">
                <label>Description</label>
                <input type="text" bind:value={templateDesc} placeholder="Basic router config" />
              </div>
            </div>
            <button type="submit" class="btn-primary" disabled={!templateName}>Save Template</button>
          </form>
        </div>

        <!-- Template List -->
        {#if templates.length === 0}
          <div class="no-data">No templates saved yet.</div>
        {:else}
          <div class="template-list">
            {#each templates as tpl}
              <div class="template-entry">
                <div class="template-info">
                  <span class="template-name">{tpl.name}</span>
                  <span class="template-desc">{tpl.description || 'No description'}</span>
                  <span class="template-date">{tpl.created}</span>
                </div>
                <div class="template-actions">
                  <button class="btn-sm" on:click={() => handleApplyTemplate(tpl.name)}>
                    Apply to Staging
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- ═══════════════ CLI Terminal Tab ═══════════════ -->

    {#if activeTab === 'cli'}
      <div class="cli-panel">
        <div class="panel-header">
          <h2>CLI Terminal</h2>
          <span class="cli-hint">VyOS-style commands: set, delete, commit, show, rollback</span>
        </div>

        <div class="terminal">
          <div class="terminal-output">
            {#each cliHistory as entry}
              <div class="cli-line">
                <span class="cli-prompt">vectoros#</span>
                <span class="cli-cmd">{entry.cmd}</span>
              </div>
              <div class="cli-output" class:cli-error={entry.status === 'error'}>
                {entry.output}
              </div>
            {/each}
            {#if cliHistory.length === 0}
              <div class="cli-welcome">
                VectorOS Configuration CLI<br>
                Type "configure" to enter configuration mode.<br>
                Type "show" to display the current configuration.<br>
              </div>
            {/if}
          </div>
          <div class="terminal-input">
            <span class="cli-prompt">vectoros#</span>
            <input
              type="text"
              bind:value={cliCommand}
              on:keydown={handleCliKeydown}
              placeholder="Type a command..."
              class="cli-input"
            />
          </div>
        </div>
      </div>
    {/if}

  {/if}
</div>

<style>
  .config-page {
    max-width: 1400px;
  }

  h1 {
    margin-bottom: 1.5rem;
    color: #00ff88;
  }

  /* ── Tabs ──────────────────────────────────────── */

  .tab-bar {
    display: flex;
    gap: 0;
    border-bottom: 2px solid #333;
    margin-bottom: 1.5rem;
  }

  .tab {
    background: none;
    border: none;
    color: #888;
    padding: 0.75rem 1.5rem;
    cursor: pointer;
    font-size: 0.95rem;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    transition: all 0.2s;
  }

  .tab:hover {
    color: #e0e0e0;
  }

  .tab.active {
    color: #00ff88;
    border-bottom-color: #00ff88;
  }

  .tab-badge {
    background: #ff4444;
    color: #fff;
    padding: 0.1rem 0.4rem;
    border-radius: 0.6rem;
    font-size: 0.7rem;
    margin-left: 0.3rem;
  }

  /* ── Panels ────────────────────────────────────── */

  .panel-grid {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 1.5rem;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .panel-header h2 {
    margin: 0;
    color: #e0e0e0;
    font-size: 1.1rem;
  }

  .panel-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .staging-badge {
    background: #332200;
    color: #ffaa00;
    padding: 0.2rem 0.6rem;
    border-radius: 0.3rem;
    font-size: 0.75rem;
  }

  /* ── Tree View ─────────────────────────────────── */

  .tree-panel {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
    overflow-x: auto;
  }

  .tree-view {
    font-family: 'Monaco', 'Menlo', 'Consolas', monospace;
    font-size: 0.9rem;
    line-height: 1.6;
  }

  .tree-node {
    margin: 0;
  }

  .tree-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.15rem 0;
  }

  .tree-row.clickable {
    cursor: pointer;
  }

  .tree-row.clickable:hover {
    background: rgba(0, 255, 136, 0.05);
  }

  .tree-icon {
    color: #00ff88;
    font-size: 0.7rem;
    width: 1rem;
    text-align: center;
  }

  .tree-key {
    color: #7ec8e3;
  }

  .tree-branch-count {
    color: #666;
    font-size: 0.8rem;
  }

  .tree-children {
    padding-left: 1.5rem;
    border-left: 1px solid #333;
    margin-left: 0.5rem;
  }

  .tree-leaf {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0;
  }

  .tree-leaf:hover {
    background: rgba(0, 255, 136, 0.05);
  }

  .tree-top-level {
    padding-left: 0;
  }

  .tree-connector {
    width: 1rem;
    color: #333;
  }

  .tree-eq {
    color: #666;
  }

  .tree-value {
    color: #ffc857;
  }

  .tree-actions {
    display: none;
    gap: 0.2rem;
    margin-left: auto;
  }

  .tree-leaf:hover .tree-actions {
    display: flex;
  }

  .btn-tiny {
    background: none;
    border: 1px solid #555;
    color: #888;
    width: 1.4rem;
    height: 1.4rem;
    border-radius: 0.2rem;
    cursor: pointer;
    font-size: 0.7rem;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .btn-tiny:hover {
    border-color: #00ff88;
    color: #00ff88;
  }

  .btn-tiny-danger:hover {
    border-color: #ff4444;
    color: #ff4444;
  }

  /* ── Operations Panel ──────────────────────────── */

  .ops-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .ops-card {
    background: #1a1a2e;
    padding: 1.2rem;
    border-radius: 0.75rem;
  }

  .ops-card h3 {
    color: #e0e0e0;
    margin: 0 0 0.75rem 0;
    font-size: 0.95rem;
  }

  .ops-desc {
    color: #888;
    font-size: 0.85rem;
    margin: 0 0 0.75rem 0;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-bottom: 0.75rem;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
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

  input:focus {
    outline: none;
    border-color: #00ff88;
  }

  .button-row {
    display: flex;
    gap: 0.75rem;
  }

  .btn-primary {
    background: #00ff88;
    color: #0f0f23;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-primary:hover { opacity: 0.9; }
  .btn-primary:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-secondary {
    background: #333;
    color: #e0e0e0;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-secondary:hover { opacity: 0.9; }

  .btn-danger {
    background: #ff4444;
    color: #fff;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-danger:hover { opacity: 0.9; }
  .btn-danger:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-warning {
    background: #ffaa00;
    color: #0f0f23;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 0.4rem;
    font-weight: 600;
    cursor: pointer;
    font-size: 0.9rem;
  }

  .btn-warning:hover { opacity: 0.9; }
  .btn-warning:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-sm {
    background: #333;
    color: #e0e0e0;
    border: 1px solid #555;
    padding: 0.3rem 0.7rem;
    border-radius: 0.3rem;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .btn-sm:hover { border-color: #00ff88; }

  /* ── Diff View ─────────────────────────────────── */

  .diff-panel {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .diff-count {
    color: #888;
    font-size: 0.9rem;
  }

  .diff-view {
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.9rem;
    line-height: 1.8;
  }

  .diff-entry {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.3rem 0;
    border-bottom: 1px solid #222;
  }

  .diff-op {
    font-weight: bold;
    width: 4rem;
    text-align: center;
    padding: 0.1rem 0.3rem;
    border-radius: 0.2rem;
    font-size: 0.8rem;
  }

  .diff-set .diff-op {
    background: #003322;
    color: #00ff88;
  }

  .diff-delete .diff-op {
    background: #331111;
    color: #ff4444;
  }

  .diff-update .diff-op {
    background: #332200;
    color: #ffaa00;
  }

  .diff-path {
    color: #7ec8e3;
    min-width: 200px;
  }

  .diff-value {
    color: #00ff88;
  }

  .diff-old {
    color: #ff4444;
    text-decoration: line-through;
  }

  .diff-arrow {
    color: #666;
  }

  .diff-new {
    color: #00ff88;
  }

  /* ── History ───────────────────────────────────── */

  .history-panel {
    background: #1a1a2e;
    padding: 1.5rem;
    border-radius: 0.75rem;
  }

  .history-count {
    color: #888;
    font-size: 0.9rem;
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

  .history-actions {
    flex-shrink: 0;
  }

  /* ── Templates ─────────────────────────────────── */

  .templates-panel {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .template-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .template-entry {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background: #1a1a2e;
    border-radius: 0.5rem;
    border: 1px solid #333;
  }

  .template-info {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    flex: 1;
  }

  .template-name {
    color: #7ec8e3;
    font-weight: bold;
    font-size: 0.95rem;
  }

  .template-desc {
    color: #888;
    font-size: 0.85rem;
  }

  .template-date {
    color: #555;
    font-size: 0.75rem;
  }

  .template-actions {
    flex-shrink: 0;
  }

  /* ── CLI Terminal ──────────────────────────────── */

  .cli-panel {
    background: #1a1a2e;
    border-radius: 0.75rem;
    overflow: hidden;
  }

  .cli-hint {
    color: #666;
    font-size: 0.8rem;
  }

  .terminal {
    background: #0a0a1a;
    border-top: 1px solid #333;
  }

  .terminal-output {
    padding: 1rem;
    max-height: 500px;
    overflow-y: auto;
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.85rem;
    line-height: 1.6;
  }

  .cli-welcome {
    color: #888;
    line-height: 1.8;
  }

  .cli-line {
    display: flex;
    gap: 0.5rem;
  }

  .cli-prompt {
    color: #00ff88;
    user-select: none;
  }

  .cli-cmd {
    color: #e0e0e0;
  }

  .cli-output {
    color: #ccc;
    padding-left: 1.5rem;
    white-space: pre-wrap;
    margin-bottom: 0.5rem;
  }

  .cli-output.cli-error {
    color: #ff4444;
  }

  .terminal-input {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    border-top: 1px solid #222;
  }

  .cli-input {
    flex: 1;
    background: transparent;
    border: none;
    color: #e0e0e0;
    font-family: 'Monaco', 'Menlo', monospace;
    font-size: 0.9rem;
    outline: none;
  }

  /* ── Common ────────────────────────────────────── */

  .loading {
    color: #888;
    text-align: center;
    padding: 3rem;
  }

  .no-data {
    color: #666;
    text-align: center;
    padding: 2rem;
  }

  .error-card {
    background: #2e1a1a;
    border: 1px solid #ff4444;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    margin-bottom: 1rem;
    color: #ff4444;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .error-dismiss {
    background: none;
    border: none;
    color: #ff4444;
    cursor: pointer;
    font-size: 1.1rem;
  }
</style>
