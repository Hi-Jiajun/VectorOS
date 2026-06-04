# VyOS Configuration System Analysis

Deep analysis of [VyOS vyos-1x](https://github.com/vyos/vyos-1x) source code, examining the configuration architecture, CLI model, validation, migration, templates, and operational mode. This document informs VectorOS design decisions.

## 1. Configuration Paradigm: Set/Delete/Commit Model

### Two Distinct Modes

VyOS has two CLI modes, each with different config access semantics:

**Operational Mode** (default on login): Read-only access to the running (effective) config. Used for `show`, `ping`, `traceroute`, etc.

**Configuration Mode** (entered via `configure`): Full read-write access. Every session builds a *proposed* config on top of the current running config. Changes are staged, not applied until `commit`.

```
vyos@router:~$ configure
[edit]
vyos@router# set interfaces ethernet eth0 address 192.168.1.1/24
[edit]
vyos@router# commit
[edit]
vyos@router# save
[edit]
vyos@router# exit
```

### Core Operations

| Operation | CLI Command | Effect |
|-----------|-------------|--------|
| **set** | `set <path> [value]` | Add or modify a config node |
| **delete** | `delete <path> [value]` | Remove a node or value |
| **comment** | `comment <path> "text"` | Attach comment to a node |
| **commit** | `commit` | Apply session config to running config |
| **save** | `save` | Persist running config to disk (`config.boot`) |
| **discard** | `discard` | Revert all uncommitted changes |
| **show** | `show <path>` | Display config at path |

### Session Architecture

Each `configure` session creates an isolated environment (`ConfigSession` class in `python/vyos/configsession.py`). The key implementation details:

```python
class ConfigSession:
    """The write API of VyOS."""
    
    def set(self, path, value=None):
        # Calls /opt/vyatta/sbin/my_set or vyconf backend
        self.__run_command([SET] + path + value)

    def delete(self, path, value=None):
        self.__run_command([DELETE] + path + value)

    def commit(self):
        # Applies all staged changes atomically
        out = self.__run_command([COMMIT])
        return out

    def discard(self):
        self.__run_command([DISCARD])
```

The backend has evolved: legacy uses `cli-shell-api` with shell scripts (`my_set`, `my_delete`, `my_commit`); the newer backend uses a gRPC-based `vyconf` service (`python/vyos/vyconf_session.py`).

### Config Tree (libvyosconfig)

The config is stored as a tree in memory, backed by a C library (`libvyosconfig`). The Python bindings (`python/vyos/configtree.py`) provide:

```python
class ConfigTree:
    # Core operations - all operate on the in-memory tree
    def set_add_value(self, path, value)   # Add value to a node
    def delete_value(self, path, value)    # Remove a specific value
    def delete_node(self, path)            # Remove entire subtree
    def exists(self, path)                 # Check node existence
    def return_value(self, path)           # Get single value
    def return_values(self, path)          # Get all values (multi-node)
    def list_nodes(self, path)             # List child node names
    def to_string(self)                    # Serialize to config file format
    def to_json(self)                      # Serialize to JSON dict
```

Config file format uses nested braces with space-separated paths:
```
interfaces {
    ethernet eth0 {
        address "192.168.1.1/24"
        description "WAN interface"
    }
}
```

### Commit-Confirm Safety Net

VyOS has a `commit-confirm` mechanism for remote management: if you commit with a timer and don't `confirm` within N minutes, the config reverts. This prevents lockout from bad remote changes:

```python
def commit_confirm(self, minutes=DEFAULT_COMMIT_CONFIRM_MINUTES):
    # Commit, but set a timer that reverts to previous config
    # if not confirmed within 'minutes'
```

## 2. Configuration Tree Structure

### Default Config (`data/config.boot.default`)

The minimal default config has these branches:

```
interfaces {
    loopback lo {}
}
service {
    ntp {
        allow-client { address "127.0.0.0/8" ... }
        server time1.vyos.net {}
    }
}
system {
    config-management { commit-revisions "100" }
    host-name "vyos"
    login {
        user vyos { authentication { ... } }
    }
    syslog { ... }
}
```

### Main Branches (from interface-definitions)

The full config tree is defined via XML files in `interface-definitions/`. Major branches:

| Branch | Description |
|--------|-------------|
| `interfaces` | All network interfaces (ethernet, bridge, bond, tunnel, pppoe, wireguard, etc.) |
| `firewall` | Firewall rules, groups, zones, flowtables |
| `service` | NTP, DNS, SSH, HTTPS, DHCP relay/server, syslog, SNMP |
| `system` | Hostname, login, syslog, config-management, option |
| `protocols` | Static routes, BGP, OSPF, ISIS, PIM, BFD (via FRRouting) |
| `vpn` | IPsec, OpenVPN, L2TP, PPTP, WireGuard VPN tunnels |
| `nat` | Source/Destination NAT, NAT66 |
| `vpp` | VPP data plane settings (VectorOS-specific) |
| `policy` | Route maps, prefix lists, community lists |
| `qos` | Traffic shaping, policing |
| `container` | OCI container management |
| `high-availability` | VRRP, conntrack-sync |
| `load-balancing` | WAN load balancing, HAProxy |

### Node Taxonomy

VyOS defines three fundamental node types (from `config.py` docstring):

1. **Leaf nodes**: Can have values, cannot have children
   - Single-value: `system host-name` has one string
   - Multi-value: `system name-server` can have multiple values
   - Valueless: `system ip disable-forwarding` is a boolean flag (presence = true)

2. **Non-leaf nodes with fixed children**: Parent nodes whose children are predefined
   - e.g., `system` always has `login`, `name-server`, `host-name` etc.

3. **Tag nodes**: Non-leaf nodes with arbitrary child names
   - e.g., `system task-scheduler task` where task names are user-defined
   - e.g., `interfaces ethernet eth0` where interface names are user-defined
   - Represented as `tagNode` in XML definitions

## 3. Config Validation Approach

### Multi-Layer Validation

VyOS uses a three-layer validation system:

#### Layer 1: XML Schema Validation (CLI-level)

Interface definitions (`interface-definitions/*.xml.in`) define constraints, completions, and defaults:

```xml
<tagNode name="ethernet" owner="${vyos_conf_scripts_dir}/interfaces_ethernet.py">
  <properties>
    <help>Ethernet Interface</help>
    <priority>318</priority>
    <valueHelp>
      <format>ethN</format>
      <description>Ethernet interface name</description>
    </valueHelp>
    <constraint>
      <regex>((eth|lan)[0-9]+|(eno|ens|enp|enx).+)</regex>
    </constraint>
    <constraintErrorMessage>Invalid Ethernet interface name</constraintErrorMessage>
  </properties>
  <children>
    <leafNode name="duplex">
      <properties>
        <constraint>
          <regex>(auto|half|full)</regex>
        </constraint>
      </properties>
      <defaultValue>auto</defaultValue>
    </leafNode>
  </children>
</tagNode>
```

Key XML properties:
- `<constraint>`: Regex or value list that validates input
- `<constraintErrorMessage>`: Custom error on constraint failure
- `<defaultValue>`: Default value when node is absent
- `<completionHelp>`: Tab-completion suggestions (list, script, or EXUFFICIENT)
- `<owner>`: Which `conf_mode` script handles this node
- `<priority>`: Commit execution order (lower = earlier)

The XML is compiled into a Python cache (`xml_ref/cache/`) for runtime use.

#### Layer 2: Commit Script Validation (`conf_mode/*.py`)

Each feature has a conf_mode script with four mandatory functions:

```python
# src/conf_mode/interfaces_pppoe.py (representative pattern)

def get_config(config=None):
    """Retrieve CLI config as a dictionary"""
    conf = Config()
    base = ['interfaces', 'pppoe']
    ifname, pppoe = get_interface_dict(conf, base)
    return pppoe

def verify(pppoe):
    """Validate the config dict; raise ConfigError on failure"""
    verify_source_interface(pppoe)
    verify_authentication(pppoe)
    verify_vrf(pppoe)
    if int(pppoe['mru']) > int(pppoe['mtu']):
        raise ConfigError('PPPoE MRU needs to be lower than MTU!')

def generate(pppoe):
    """Render config files from templates"""
    render(config_pppoe, 'pppoe/peer.j2', pppoe, permission=0o640)

def apply(pppoe):
    """Apply changes to the system"""
    if 'deleted' in pppoe:
        call(f'systemctl stop ppp@{ifname}.service')
    else:
        call(f'systemctl restart ppp@{ifname}.service')
```

The `verify()` function is the primary validation mechanism. It is called during `commit` and raises `ConfigError` to reject invalid configs.

#### Layer 3: Common Validation Helpers (`configverify.py`)

Reusable validation functions for cross-cutting concerns:

```python
def verify_mtu(config):
    """Check MTU against hardware min/max"""
    min_mtu = Interface(config['ifname']).get_min_mtu()
    max_mtu = Interface(config['ifname']).get_max_mtu()
    if mtu < min_mtu or mtu > max_mtu:
        raise ConfigError(f'Interface MTU out of range!')

def verify_vrf(config):
    """Check VRF exists and is compatible"""
    if not interface_exists(vrf):
        raise ConfigError(f'VRF "{vrf}" does not exist!')

def verify_tunnel(config):
    """Validate tunnel source/remote/encapsulation"""
    # Complex validation logic for tunnel parameters
```

### Validation Flow During Commit

```
User: commit
  |
  v
ConfigSession.commit()
  |
  v
For each changed node, determined by XML owner attribute:
  |
  v
run_config_mode_script(target, config)
  |-- mod.get_config(config)     # Read config into dict
  |-- mod.verify(config)         # Validate (raises ConfigError to abort)
  |-- mod.generate(config)       # Render templates to config files
  |-- mod.apply(config)          # Apply (restart services, etc.)
```

Scripts are executed in priority order (the `<priority>` tag in XML). Dependencies between scripts are declared in `data/config-mode-dependencies/vyos-1x.json`.

### Config Diff System

The `configdiff.py` module provides structured diffing between session and effective configs:

```python
class ConfigDiff:
    def get_child_nodes_diff(self, path, expand_nodes=Diff.ADD | Diff.DELETE | ...):
        """Returns dict with keys: merge, delete, add, stable"""
        
    def is_node_changed(self, path):
        """Returns True if any child under path was changed"""
```

This is used by scripts to determine what actually changed and only reconfigure affected parts.

## 4. Config Migration System

### Version Tracking

Every config file contains version metadata in the footer:

```
// Warning: Do not remove the following line.
// vyos-config-version: "interfaces@22:firewall@5:system@21:vpp@6"
// Release version: 1.5-rolling
```

The `component_version.py` module tracks per-component version numbers. Each component (interfaces, firewall, system, vpp, etc.) has its own version counter.

### Migration Script Architecture

Migration scripts live in `src/migration-scripts/<component>/N-to-M`:

```
src/migration-scripts/
├── vpp/
│   ├── 1-to-2      # Remove deprecated xdp-options
│   ├── 2-to-3
│   ├── 3-to-4
│   ├── 4-to-5
│   └── 5-to-6      # Large refactor: move interfaces, reorganize resources
├── interfaces/
├── firewall/
├── nat/
└── ...
```

Each migration script exports a `migrate(config: ConfigTree)` function that modifies the config tree in-place:

```python
# src/migration-scripts/vpp/1-to-2
from vyos.configtree import ConfigTree

def migrate(config: ConfigTree) -> None:
    base = ['vpp', 'settings', 'interface']
    if not config.exists(base):
        return
    
    for iface_name in config.list_nodes(base):
        xdp_options_base = base + [iface_name, 'xdp-options']
        if config.exists(xdp_options_base + ['no-syscall-lock']):
            config.delete(xdp_options_base + ['no-syscall-lock'])
        if config.exists(xdp_options_base) and len(config.list_nodes(xdp_options_base)) == 0:
            config.delete(xdp_options_base)
```

### Migration Execution Flow

```python
class ConfigMigrate:
    def run_migration_scripts(self):
        # 1. Sort components alphabetically (with special bgp/quagga ordering)
        # 2. For each component, find migration scripts N-to-M
        # 3. Filter scripts: only run those where N >= current component version
        # 4. Execute scripts sequentially via ComposeConfig
        # 5. Update component version numbers after successful migration
        for file in script_list:
            self.compose.apply_file(f, func_name='migrate')
```

The `ComposeConfig` class provides safe execution with checkpoint support:

```python
class ComposeConfig:
    def apply_file(self, func_file: str, func_name: str):
        """Load and apply a migration function to the config tree"""
        mod = load_as_module_source(mod_name, func_file)
        func = getattr(mod, func_name)
        self.apply_func(func)
```

Key design properties:
- Scripts are idempotent (check `config.exists()` before modifying)
- Scripts are ordered by version number (N-to-M)
- Failed migrations create checkpoint files for debugging
- Component versions are updated atomically after all scripts succeed

## 5. Template System

### Engine: Jinja2

VyOS uses Jinja2 for all config file generation (`python/vyos/template.py`):

```python
def _get_environment(location=None):
    return Environment(
        auto_reload=False,       # Don't check for file changes
        cache_size=100,          # Cache parsed templates
        loader=FileSystemLoader(location or DEFAULT_TEMPLATE_DIR),
        trim_blocks=True,
        undefined=ChainableUndefined,
        extensions=['jinja2.ext.loopcontrols', 'jinja2.ext.do']
    )
```

### Template Rendering API

```python
# Render to file
render(destination, template, content, permission=0o640, user='root', group='vyattacfg')

# Render to string
rendered = render_to_string(template, content)
```

### Template Organization

Templates mirror the config tree structure:

```
data/templates/
├── dhcp-server/
│   ├── kea-dhcp4.conf.j2      # Kea DHCP4 config
│   └── kea-dhcp6.conf.j2      # Kea DHCP6 config
├── dns-forwarding/
│   ├── recursor.conf.j2       # DNS recursor config
│   └── recursor.conf.lua.j2   # Lua config for DNS
├── firewall/                   # nftables rules
├── frr/                        # FRRouting config
├── pppoe/                      # PPPoE peer config
├── ssh/                        # SSH server config
├── vpp/                        # VPP startup config
├── system/                     # System-level configs
└── ...50+ service directories
```

### Custom Filters

VyOS registers custom Jinja2 filters for config rendering:

```python
@register_filter('force_to_list')
def force_to_list(value):
    """Convert scalars to single-item lists"""
    
@register_filter('seconds_to_human')
def seconds_to_human(seconds, separator=""):
    """Convert seconds to human-readable values like 1d6h15m23s"""
```

### Example: Kea DHCP4 Template

```jinja2
{
    "Dhcp4": {
        "interfaces-config": {
{% if listen_address is vyos_defined %}
            "interfaces": {{ listen_address | kea_address_json }},
            "dhcp-socket-type": "udp",
{% elif listen_interface is vyos_defined %}
            "interfaces": {{ listen_interface | tojson }},
            "dhcp-socket-type": "raw",
{% else %}
            "interfaces": [ "*" ],
            "dhcp-socket-type": "raw",
{% endif %}
        },
        "lease-database": {
            "type": "memfile",
            "persist": true,
            "name": "{{ lease_file }}"
        },
{% if shared_network_name is vyos_defined %}
        "shared-networks": {{ shared_network_name | kea_shared_network_json }},
{% endif %}
    }
}
```

Templates use custom filters like `kea_address_json`, `kea_shared_network_json` to convert VyOS config dicts into Kea's expected JSON format.

## 6. Operational Mode

### Op-Mode Definition

Operational commands are defined via XML in `op-mode-definitions/`:

```xml
<node name="show">
  <children>
    <node name="interfaces">
      <command>${vyos_op_scripts_dir}/interfaces.py show_summary_extended</command>
      <children>
        <leafNode name="counters">
          <command>${vyos_op_scripts_dir}/interfaces.py show_counters</command>
        </leafNode>
        <leafNode name="detail">
          <command>${vyos_op_scripts_dir}/interfaces.py show</command>
        </leafNode>
      </children>
    </node>
  </children>
</node>
```

### Op-Mode Scripts (`src/op_mode/`)

Each op-mode script is a Python module with typed functions:

```python
# Op-mode functions follow naming conventions
def show_interfaces(...)       # show commands
def clear_conntrack(...)       # clear/reset commands
def reset_firewall(...)        # reset commands
def restart_dns(...)           # restart commands
```

The `opmode.py` framework provides:
- Automatic CLI argument parsing from function signatures
- Structured error hierarchy (`Error`, `UnconfiguredSubsystem`, `DataUnavailable`, etc.)
- Output capture for `show`/`generate` commands

```python
class Error(Exception):
    """Base class for op-mode errors"""
    pass

class UnconfiguredSubsystem(Error):
    """Subsystem not configured, can't perform operation"""
    pass

class DataUnavailable(Error):
    """Data not available (possibly transient)"""
    pass
```

### Config Query from Op-Mode

Op-mode scripts need read access to config. VyOS provides `ConfigTreeQuery`:

```python
from vyos.configquery import ConfigTreeQuery

query = ConfigTreeQuery()
if query.exists(['interfaces', 'ethernet', 'eth0']):
    addr = query.value(['interfaces', 'ethernet', 'eth0', 'address'])
```

## 7. Service Integration Patterns

### The Commit Pipeline

Services integrate through the commit pipeline, which follows a strict four-phase pattern:

```
get_config() -> verify() -> generate() -> apply()
```

This separation enables:
- **Dry-run**: `get_config()` + `verify()` without side effects
- **Idempotency**: `generate()` produces deterministic output
- **Incremental updates**: `apply()` can check what changed

### Config Dependency System

Services declare dependencies via `data/config-mode-dependencies/vyos-1x.json`:

```json
{
    "interfaces_ethernet": {
        "static_arp": ["protocols_static_arp"]
    },
    "firewall": {
        "conntrack": ["system_conntrack"],
        "group_resync": ["system_conntrack", "nat", "nat66", "policy_route"]
    },
    "nat": {
        "conntrack": ["system_conntrack"]
    }
}
```

When `interfaces_ethernet` is committed, `protocols_static_arp` is also triggered. Dependencies form a DAG and are resolved using Python's `graphlib.TopologicalSorter`.

```python
def set_dependents(case: str, config: 'Config', tagnode=None):
    """Register that the current script needs other scripts to run after it"""
    d = get_dependency_dict(config)
    k = caller_name()  # Which script is calling
    for target in d[k][case]:
        func = def_closure(target, config, tagnode)
        append_uniq(l, func)

def call_dependents():
    """Execute all registered dependent scripts"""
    while l:
        f = l.pop(0)
        f()
```

### Priority-Based Execution

Each conf_mode script has a priority (from XML `<priority>` tag). During commit, scripts are sorted by priority and executed in order. Lower priority = earlier execution. For example:
- `interfaces_ethernet.py`: priority 318
- `firewall.py`: lower priority (runs before interfaces)

This ensures foundational services (firewall, conntrack) are configured before dependent services (interfaces, NAT).

### Service Template Rendering Flow

```
Config dict (from get_config)
  |
  v
Jinja2 template (data/templates/<service>/*.j2)
  |
  v
Rendered config file (/etc/<service>/<config-file>)
  |
  v
Service restart/reload (systemctl restart <service>)
```

### Example: PPPoE Service Integration

```
1. get_config():
   - Read 'interfaces pppoe pppoe0' from config tree
   - Convert to dict with key_mangling ('-' -> '_')
   - Check if critical params changed (for reconnect decision)
   
2. verify():
   - Check source-interface exists
   - Check authentication config
   - Check VRF compatibility
   - Check MTU/MRU consistency
   
3. generate():
   - Render 'pppoe/peer.j2' template
   - Write to /etc/ppp/peers/pppoe0
   
4. apply():
   - If deleted: stop ppp@pppoe0.service, remove interface
   - If critical params changed: restart ppp@pppoe0.service
   - Otherwise: update interface settings without restart
```

## 8. Key Design Patterns for VectorOS

### Pattern 1: Four-Phase Commit (get/verify/generate/apply)

Every config module follows this exact pattern. This is the single most important pattern to adopt.

```python
def get_config(config=None):
    """Read config tree into a Python dict"""
    
def verify(config):
    """Validate config; raise ConfigError on failure"""
    
def generate(config):
    """Render templates to config files"""
    
def apply(config):
    """Apply changes to running system"""
```

**VectorOS adoption**: Use this exact four-phase pattern for all control-plane config modules. The Axum API handlers can call these phases for preview (`get_config` + `verify`) and apply (`generate` + `apply`).

### Pattern 2: Hierarchical Config as Nested Dict

VyOS converts its config tree to Python dicts with `key_mangling=('-', '_')` to replace hyphens with underscores:

```python
config = conf.get_config_dict(
    ['interfaces', 'ethernet', 'eth0'],
    key_mangling=('-', '_'),
    get_first_key=True,
    with_defaults=True,
    with_recursive_defaults=True,
)
# config = {'address': ['192.168.1.1/24'], 'description': 'WAN', ...}
```

**VectorOS adoption**: Store config as a nested `serde_json::Value` or typed Rust structs. Use `key_mangling` equivalent for CLI compatibility (hyphenated keys) vs internal use (underscored keys).

### Pattern 3: XML-Defined CLI Schema

The XML definitions serve as the single source of truth for:
- CLI tree structure
- Validation constraints (regex, value lists)
- Tab-completion data
- Default values
- Owner scripts (which module handles what)
- Priority ordering

**VectorOS adoption**: Define a YAML or TOML schema for CLI nodes that includes constraints, defaults, owners, and priorities. Compile to a runtime lookup cache. This avoids scattered validation logic.

### Pattern 4: Config Diff for Incremental Updates

The `DiffTree` and `ConfigDiff` classes provide structured comparison:

```python
D = get_config_diff(conf, key_mangling=('-', '_'))
if D.is_node_changed(['interfaces', 'ethernet', eth0, 'mtu']):
    # Only reconfigure MTU
```

**VectorOS adoption**: Implement a `ConfigDiff` that compares old vs new config and produces a set of "changed paths". Each service handler can then decide whether to do a full restart or incremental update.

### Pattern 5: Tag Nodes for Dynamic Collections

Tag nodes (e.g., `interfaces ethernet eth0`) allow arbitrary child names. The `get_interface_dict()` helper handles common patterns:

```python
ifname, ethernet = get_interface_dict(conf, base=['interfaces', 'ethernet'])
# ifname = 'eth0', ethernet = {config dict for eth0}
# If being deleted: 'deleted' key is set in dict
```

**VectorOS adoption**: For VPP interface management, use tag nodes under `interfaces vpp <type> <name>`. Provide helper functions similar to `get_interface_dict()`.

### Pattern 6: Dependency DAG for Commit Ordering

The dependency system prevents ordering issues. When firewall changes, conntrack restarts first. When interfaces change, static ARP updates.

**VectorOS adoption**: Define a dependency graph in a config file. During API commit, resolve dependencies and execute in topological order. This is critical when VPP config changes affect multiple subsystems.

### Pattern 7: Versioned Config Migration

Each config section has its own version number. Migration scripts are small, focused, and idempotent. The system can handle migrations from any old version by running the chain of N-to-M scripts.

**VectorOS adoption**: Track a config schema version per module (interfaces, vpp, dhcp, etc.). When the schema changes, write migration scripts that transform old config trees to new ones. Use a similar `N-to-M` naming convention.

### Pattern 8: Commit-Confirm for Remote Safety

The commit-confirm timer prevents lockout from bad remote changes.

**VectorOS adoption**: For the web UI and API, implement a `commit-confirm` with timeout. If the user doesn't confirm, revert to the previous config.

## 9. CLI Command Structure to Follow

### Recommended VectorOS CLI Tree

Based on VyOS patterns and VectorOS requirements:

```
configure
  interfaces
    ethernet <ifname>
      address <addr>
      description <text>
      mtu <number>
      vpp
        disable
    vpp
      bonding <name>
      bridge <name>
      tap <name>
      loopback <name>
      vxlan <name>
  vpp
    settings
      cpu-cores <number>
      log-level <level>
      interface-rx-mode <mode>
    nat
      nat44
        ...
    acl
      ...
    ipfix
      ...
  service
    dhcp-server
      ...
    dns
      ...
  firewall
    ...
  system
    host-name <name>
```

### Op-Mode Commands

```
show
  interfaces
    vpp
    summary
    detail
    counters
  vpp
    info
    neighbors
    fib
  dhcp
    leases
    server
  firewall
    statistics
  routes
    summary
    detail
  nat
    mappings
    statistics
```

### Config Script File Naming

Follow VyOS convention:
- Config mode: `src/conf_mode/<feature>.py` (e.g., `interfaces_ethernet.py`)
- Op mode: `src/op_mode/<feature>.py` (e.g., `interfaces.py`)
- Templates: `data/templates/<feature>/` (e.g., `dhcp-server/`)
- Migration: `src/migration-scripts/<component>/<N-to-M>`

## 10. Summary of Key Files in vyos-1x

| Path | Purpose |
|------|---------|
| `python/vyos/config.py` | Core Config class - read access to config tree |
| `python/vyos/configsession.py` | Write API - set/delete/commit/discard |
| `python/vyos/configtree.py` | Python bindings for libvyosconfig (C library) |
| `python/vyos/config_mgmt.py` | Commit management, revisions, rollback |
| `python/vyos/configdiff.py` | Diff system for session vs effective config |
| `python/vyos/configdep.py` | Dependency DAG for commit ordering |
| `python/vyos/configverify.py` | Common validation helpers |
| `python/vyos/configdict.py` | Dict helpers: `get_interface_dict`, `node_changed` |
| `python/vyos/configquery.py` | Read-only config query from op-mode |
| `python/vyos/template.py` | Jinja2 rendering with custom filters |
| `python/vyos/migrate.py` | Config migration engine |
| `python/vyos/opmode.py` | Op-mode framework and error hierarchy |
| `python/vyos/frrender.py` | FRRouting config generation |
| `python/vyos/compose_config.py` | Composable config transformations |
| `python/vyos/xml_ref/` | XML reference cache and schema queries |
| `python/vyos/defaults.py` | System path constants |
| `data/config.boot.default` | Default config file |
| `data/templates/` | Jinja2 templates for all services |
| `data/config-mode-dependencies/` | Commit dependency graph |
| `interface-definitions/*.xml.in` | CLI schema definitions |
| `op-mode-definitions/*.xml.in` | Op-mode command definitions |
| `src/conf_mode/*.py` | Config mode scripts (get/verify/generate/apply) |
| `src/op_mode/*.py` | Op-mode scripts |
| `src/migration-scripts/` | Version-to-version migration scripts |
| `libvyosconfig/` | C library for config tree management |
