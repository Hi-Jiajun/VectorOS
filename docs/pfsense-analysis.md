# pfSense Architecture Analysis

Deep analysis of pfSense CE (v2.8.x) source code for design patterns applicable to VectorOS.

## 1. Architecture Overview

### 1.1 Technology Stack

| Layer | Technology |
|-------|-----------|
| Data Plane | FreeBSD kernel `pf` packet filter |
| Control Plane | PHP (procedural, no framework) |
| Web UI | Bootstrap CSS + raw PHP rendering |
| Config Storage | Single XML file (`config.xml`) |
| VPN | OpenVPN, IPsec (strongSwan), L2TP, PPPoE |
| DNS | Unbound resolver, dnsmasq forwarder |
| DHCP | ISC DHCP, Kea DHCP |
| HA | CARP + pfsync + XML-RPC config sync |

### 1.2 Code Organization

pfSense follows a **FreeBSD filesystem layout** rather than a traditional MVC structure:

```
src/
  etc/inc/              # Core PHP libraries (backend logic)
    config.lib.inc      # Config read/write/backup
    config.inc          # Additional config helpers
    filter.inc          # Firewall rule generation (pf ruleset builder)
    shaper.inc          # Traffic shaper (ALTQ + dummynet)
    openvpn.inc         # OpenVPN configuration
    ipsec.inc           # IPsec/strongSwan configuration
    vpn.inc             # PPPoE server, L2TP
    services.inc        # DHCP (ISC + Kea), RAdvD
    unbound.inc         # DNS resolver configuration
    interfaces.inc      # Network interface management
    gateway.inc         # Gateway management
    gwlb.inc            # Gateway load balancing/failover
    pfsense-utils.inc   # CARP, pfsync, system utilities
    upgrade_config.inc  # Config version migrations (105+ sequential steps)
    auth.inc            # Authentication
    priv.inc            # Privilege/authorization enforcement
    globals.inc         # Constants, global arrays, sysctl defaults
  usr/local/www/        # Web GUI PHP pages
    guiconfig.inc       # GUI framework (Bootstrap helpers, layout)
    firewall_rules.php  # Firewall rules list page
    firewall_nat*.php   # NAT port forward, 1:1, outbound, NPT
    interfaces_*.php    # Interface configuration pages
    vpn_*.php           # VPN configuration pages
    diag_*.php          # Diagnostic/tools pages
    widgets/            # Dashboard widgets
    classes/            # PHP classes
```

### 1.3 Key Design Pattern: Procedural + Global Config Array

pfSense uses a **global `$config` PHP array** as the in-memory representation of all settings. The entire system revolves around reading from and writing to this array, then persisting it as XML.

```php
// Reading config
$hostname = config_get_path('system/hostname', 'pfSense');
$dns_servers = config_get_path('dnsmasq/n dnsip', []);

// Writing config
config_set_path('system/hostname', 'new-hostname');
write_config('Changed hostname');
```

There is no ORM, no database, and no MVC framework. Each PHP page is a standalone script that includes shared libraries, reads config, processes POST data, and renders HTML directly.

## 2. Configuration Model

### 2.1 XML-Based Configuration (`config.xml`)

The entire pfSense configuration lives in a single XML file at `/conf/config.xml`. The top-level XML elements map directly to functional subsystems:

```xml
<pfsense>
  <system>
    <hostname>firewall</hostname>
    <domain>localdomain</domain>
    <timezone>America/New_York</timezone>
    ...
  </system>
  <interfaces>
    <wan>
      <if>igb0</if>
      <ipaddr>dhcp</ipaddr>
      <descr>WAN</descr>
    </wan>
    <lan>
      <if>igb1</if>
      <ipaddr>192.168.1.1</ipaddr>
      <subnet>24</subnet>
      <descr>LAN</descr>
    </lan>
  </interfaces>
  <filter>
    <rule>
      <tracker>1000000001</tracker>
      <interface>lan</interface>
      <type>pass</type>
      <source><any/></source>
      <destination><any/></destination>
      <descr>Allow LAN to any</descr>
    </rule>
  </filter>
  <nat>
    <rule>...</rule>  <!-- Port forwards -->
  </nat>
  <dhcpd>
    <lan>
      <range>
        <from>192.168.1.100</from>
        <to>192.168.1.200</to>
      </range>
      <staticmap>...</staticmap>
    </lan>
  </dhcpd>
  <openvpn>
    <openvpn-server>...</openvpn-server>
    <openvpn-client>...</openvpn-client>
  </openvpn>
  <ipsec>
    <phase1>...</phase1>
    <phase2>...</phase2>
  </ipsec>
  <shaper>...</shaper>
  <unbound>...</unbound>
  <gateways>...</gateways>
  ...
</pfsense>
```

### 2.2 Config Read/Write Pipeline

**Read path:**
1. Check for serialized PHP cache (`config.cache`)
2. If cache is stale/missing, parse `config.xml` via `parse_xml_config()`
3. If XML is invalid, attempt recovery from `/cf/conf/backup/*.xml` (newest first)
4. Store result in global `$config` array

**Write path:**
1. Modify `$config` in memory via `config_set_path()`
2. Call `write_config()` which:
   - Backs up current config to timestamped file
   - Records revision entry (user, timestamp, description)
   - Serializes `$config` to XML via `dump_xml_config()`
   - Atomic write: temp file + fsync + rename + dir sync
   - Removes cache file
   - Triggers CARP XML-RPC sync to partner node
3. Reload affected subsystem

### 2.3 Config Versioning and Migration

pfSense maintains a **continuous chain of migration functions** in `upgrade_config.inc` (currently 105+ steps). Each migration is named `upgrade_XXX_to_YYY` where XXX and YYY are zero-padded version numbers.

Key properties:
- Never skip a version number (no-ops exist as placeholders)
- Each function transforms the XML config from one schema to the next
- Migrations handle field renames, structural reorganizations, data conversions
- Example: version 051->052 does a major OpenVPN config restructuring with CA/cert extraction

This is directly analogous to database migration systems like Rails or Flyway, but applied to an XML config file.

### 2.4 Atomic File Writing

```php
function safe_write_file($file, $content, $force_binary = false) {
    $temp_file = tempnam(dirname($file), basename($file));
    file_put_contents($temp_file, $content);
    $fd = fopen($temp_file, "r+");
    fflush($fd); fsync($fd); fclose($fd);
    rename($temp_file, $file);
    clearstatcache(true, dirname($file));
}
```

## 3. Firewall Rule Model

### 3.1 Rule Data Model

Each firewall rule is an associative array stored in `$config['filter']['rule']`:

```php
[
    'tracker' => '1000000001',     // Unique integer ID (correlates with pf internals)
    'type' => 'pass',              // pass | block | reject | match
    'interface' => 'lan',          // Interface or 'floating'
    'floating' => '',              // Set if floating rule
    'direction' => 'in',           // in | out | any
    'ipprotocol' => 'inet',        // inet (IPv4) | inet6 (IPv6)
    'protocol' => 'tcp',           // tcp | udp | icmp | any | etc.
    'source' => [
        'address' => '192.168.1.0/24',
        'port' => '',
        'not' => '',               // Invert match
    ],
    'destination' => [
        'address' => 'any',
        'port' => '443',
    ],
    'gateway' => 'wan_gw',         // Gateway group name
    'descr' => 'Allow HTTPS',
    'disabled' => '',              // Rule exists but is inactive
    'log' => '',                   // Enable logging
    'schedule' => '',              // Schedule name for time-based rules
    'associated-rule-id' => '',    // Links NAT rule to filter rule
]
```

### 3.2 Ruleset Generation Pipeline

The `filter_configure_sync()` function orchestrates a multi-stage pipeline:

```
1. filter_generate_optcfg_array()    Build interface metadata (IPs, subnets, VIPs)
2. filter_generate_aliases()         Create pf table definitions (nested alias expansion)
3. filter_generate_gateways()        Build GW<name> route-to variables
4. filter_generate_dummynet_rules()  Limiter/dummynet queue definitions
5. filter_generate_altq_queues()     ALTQ queue definitions (HFSC/CBQ/PRIQ)
6. filter_nat_rules_generate()       All NAT/rdr rules
7. filter_rules_generate()           All pass/block filter rules
8. filter_generate_scrubbing()       Scrub rules (MSS clamp, fragment reassembly)
9. Set system limits (timeouts, state limits, optimization)
10. reload_filter()                  Write rules.debug + pfctl -o basic -f
```

### 3.3 PFConfig Class

The `PFConfig` class manages a hierarchical structure of categories, groups, and subcategories that map to sections of the final pf configuration:

- `set_config(category, group, subcategory, rules)` - Store rules under a path
- `get_config()` - Serialize all categories into a single pf configuration string
- `generate_config()` - Iterate categories in defined order, emitting comments and rules

### 3.4 Change Detection

pfSense uses an **xxh3 hash** embedded in a label tag on a default deny rule to detect changes without re-parsing the full ruleset:

```
Active hash (from `pfctl -sr`) vs Saved hash (from `rules.debug`) vs Generated hash
```

If all three match, no reload is needed. This is an efficient dirty-checking mechanism.

### 3.5 Tracker ID System

Every rule gets a globally unique `tracker` integer. Different rule types use different tracker ranges:
- Anti-lockout rules: `ANTILOCKOUT_TRACKER_START` to `ANTILOCKOUT_TRACKER_END`
- RFC1918 blocking: `RFC1918_TRACKER_START` to `RFC1918_TRACKER_END`
- Bogon blocking: `BOGONS_TRACKER_START` to `BOGONS_TRACKER_END`
- User rules: Start at `1000000001` and increment

Tracker IDs are used to:
- Correlate with pf's internal rule counters (for live statistics)
- Kill states matching a specific rule (`pfctl -k`)
- Display per-rule state/byte/packet counts in the GUI

## 4. NAT Configuration

### 4.1 NAT Rule Types

pfSense supports four NAT types, each stored in a separate config section:

| Type | Config Path | Purpose |
|------|------------|---------|
| Port Forward | `nat/rule` | DNAT (Destination NAT) - redirect inbound traffic |
| 1:1 NAT | `nat/onetoone` | Static bidirectional NAT mapping |
| Outbound NAT | `nat/outbound` | SNAT for outbound traffic (auto/manual/hybrid) |
| NPTv6 | `nat/npt` | Network Prefix Translation for IPv6 |

### 4.2 NAT Rule Model (Port Forward)

```php
[
    'interface' => 'wan',
    'protocol' => 'tcp',
    'source' => ['address' => 'any'],
    'destination' => ['address' => 'any', 'port' => '443'],
    'target' => '192.168.1.100',         // Redirect target IP
    'local-port' => '443',               // Internal port
    'associated-rule-id' => 'pass',       // Auto-create filter rule
    'natreflection' => 'enable',          // NAT reflection mode
    'descr' => 'HTTPS to web server',
    'filter-rule-association' => 'associated-rule-id',
]
```

### 4.3 NAT Reflection

For port forwards, pfSense supports NAT reflection (accessing internal services via the WAN IP from the LAN). Two modes:

1. **Pure NAT**: Creates internal rdr rules using localhost ports
2. **NAT+Proxy**: Uses a proxy process for hairpin NAT

Implementation: `filter_generate_reflection_nat()` and `filter_generate_reflection_proxy()` create temporary rdr/NAT rules during ruleset generation.

### 4.4 Outbound NAT Modes

- **Automatic**: Rules auto-generated for all internal subnets
- **Manual**: User defines every outbound NAT rule
- **Hybrid**: Auto-generated rules plus user-defined rules

## 5. VPN Management

### 5.1 OpenVPN Architecture

**Config storage:** `openvpn/openvpn-server[]` and `openvpn/openvpn-client[]` in the XML config.

**Server modes:**
| Mode | Description |
|------|------------|
| `p2p_tls` | Peer-to-peer with SSL/TLS |
| `p2p_shared_key` | Peer-to-peer with shared key (deprecated) |
| `server_tls` | Remote Access with SSL/TLS |
| `server_user` | Remote Access with User Auth only |
| `server_tls_user` | Remote Access with SSL/TLS + User Auth |

**Configuration generation (`openvpn_reconfigure`):**
1. Create tun/tap device (`ifconfig create` + rename)
2. Build `config.ovpn` incrementally (string concatenation)
3. Write key material (CA, cert, key, DH, TLS auth/crypt)
4. Set file permissions (0600 on sensitive files)
5. Start process with `openvpn --config config.ovpn`

**Key features:**
- Client Specific Overrides (CSC) push per-CN configs (static IPs, routes, options)
- Auth plugin for user authentication (`openvpn-plugin-auth-script.so`)
- Status monitoring via Unix management sockets
- Process lifecycle: acquire lock -> reconfigure -> SIGTERM -> wait -> SIGKILL -> check CARP master -> start

### 5.2 IPsec Architecture

**Config storage:** `ipsec/phase1[]` and `ipsec/phase2[]`

**Two-phase model:**
- **Phase 1 (IKE SA)**: Establishes secure channel (certificates, PSK, EAP)
- **Phase 2 (Child SA)**: Defines encrypted tunnels carrying traffic

**StrongSwan integration:**
- Uses `swanctl` format (not legacy `ipsec.conf`)
- Connection naming: `con{ikeid}` pattern
- 9 authentication methods supported (cert, PSK, EAP-MSChapv2, EAP-TLS, EAP-RADIUS, etc.)

**Configuration generation (`ipsec_configure`):**
1. Create strongSwan directory tree
2. Setup gateway interfaces and VTI tunnels
3. Generate `strongswan.conf` (nested PHP array -> config format)
4. Build swanctl connection blocks for all P1/P2 entries
5. Write certificates, keys, CRLs, PSK entries
6. Configure mobile IP pools
7. Setup bypass rules for local LAN traffic

### 5.3 PPPoE/L2TP Server

Managed in `vpn.inc` using the `mpd5` daemon. Generates `mpd.conf` and `mpd.secret` files.

## 6. Traffic Shaper (QoS)

### 6.1 Dual Backend Architecture

pfSense supports two fundamentally different shaping mechanisms:

| Backend | Kernel Framework | Use Case |
|---------|-----------------|----------|
| ALTQ | `pf`-attached queueing | Interface-based QoS (HFSC, CBQ, PRIQ, FAIRQ) |
| Dummynet | `ipfw`-attached pipes | Flexible flow-based shaping with delay/loss emulation |

### 6.2 ALTQ Class Hierarchy

```
altq_root_queue (per-interface root)
  ├── hfsc_queue     (Hierarchical Fair-Service Curve - most complex)
  │     └── hfsc_queue (nested children supported)
  ├── cbq_queue      (Class-Based Queueing, supports borrowing)
  │     └── cbq_queue (nested children)
  ├── priq_queue     (Priority Queue, leaf-only, 16 priority levels)
  └── fairq_queue    (Fair Queueing, leaf-only, per-host bandwidth limits)
```

### 6.3 Dummynet Class Hierarchy

```
dummynet_class (base)
  ├── dnpipe_class   (pipe with bandwidth, delay, loss, queue size)
  │     └── dnqueue_class  (queue within pipe, with weight and scheduler)
```

### 6.4 HFSC Service Curves

HFSC (the most capable scheduler) uses three service curve parameters per queue:
- **Realtime (m1, d, m2)**: Minimum guaranteed bandwidth
- **Linkshare (m1, d, m2)**: Fair bandwidth sharing
- **Upperlimit (m1, d, m2)**: Maximum cap

Where m1 = initial bandwidth, d = time constant, m2 = stable bandwidth.

### 6.5 Active Queue Management

Supported AQMs: `droptail`, `codel`, `pie`, `red`, `gred`
Supported schedulers: `wf2q+`, `fifo`, `qfq`, `rr`, `prio`, `fq_codel`, `fq_pie`

### 6.6 Data Flow

```
User Form Input
  -> validate_input()
  -> ReadConfig() [deserialize from XML]
  -> wconfig() [serialize back to XML]
  -> build_rules() [generate pf/ALTQ or dummynet rules]
  -> Rules applied via pfctl
  -> get_queue_stats() [pfctl -s queue -v for live stats]
```

## 7. DHCP/DNS Services

### 7.1 DHCP Dual Backend

pfSense supports both ISC DHCP and Kea DHCP, selectable per deployment.

**Kea DHCP (modern path):**
- Configuration built as PHP array -> serialized to JSON
- Uses `kea-dhcp4` / `kea-dhcp6` with modular hooks:
  - `libdhcp_lease_cmds.so` - binding variables
  - `libdhcp_ha.so` - high availability (hot-standby with TLS)
  - `libdhcp_run_script.so` - external script execution
- Static mappings as reservations with hw-address/client-id
- Kea2Unbound hook for DNS resolver synchronization

**ISC DHCP (legacy path):**
- Generates text-based `dhcpd.conf` with standard ISC syntax
- CARP-based failover with primary/secondary role detection
- DDNS zone configuration with forward/reverse mapping

### 7.2 DNS Resolver (Unbound)

Configuration generation in `unbound.inc`:
- Performance auto-tuning based on CPU count (thread count, slab sizes)
- Dynamic module stack: Python, DNS64, DNSSEC validator, Iterator
- Automatic ACLs from interface subnets, VPN tunnels, OpenVPN networks
- Static host entries as `local-data` / `local-data-ptr`
- DNS-over-TLS support with certificate-based forwarding
- `unbound-checkconf` validation before deployment

### 7.3 RAdvD (Router Advertisements)

Generates `radvd.conf` for IPv6 with:
- Managed/assist/stateless DHCPv6/router modes
- RDNSS and DNSSL options for IPv6 DNS delivery
- NAT64 prefix advertisements
- CARP VIP binding for HA

## 8. High Availability (CARP/pfsync)

### 8.1 Architecture

```
Node A (MASTER)                    Node B (BACKUP)
  ├─ CARP VIP 10.0.0.1            ├─ CARP VIP 10.0.0.1
  ├─ pfsync (state sync)           ├─ pfsync (state sync)
  ├─ XML-RPC config sync           ├─ XML-RPC config sync
  └─ pf + services                 └─ pf + services
```

### 8.2 CARP (Common Address Redundancy Protocol)

- Controlled via `net.inet.carp.allow` sysctl
- Status detection: `get_carp_status()` checks sysctl, `get_carp_interface_status()` parses `ifconfig` for MASTER/BACKUP/INIT
- Safety mechanism: won't enable CARP if no CARP VIPs are configured
- Captive portal HA awareness: checks if node is not MASTER before redirecting

### 8.3 pfsync (State Synchronization)

- Kernel-level mechanism synchronizing firewall state tables between nodes
- Ensures connections survive failover
- Runs alongside CARP

### 8.4 XML-RPC Config Sync

Configuration synchronization uses `backup_config_section()` / `restore_config_section()`:
- Serializes config sections to XML
- Transmits to partner node via XML-RPC
- Preserves package repo configuration during system section sync
- Disables security checks during trusted partner sync
- After restore, calls `reload_all_sync()` for full reconfiguration

Synced sections: filter, nat, dhcpd, openvpn, ipsec, shaper, etc.

## 9. Web Interface Patterns

### 9.1 No Framework - Procedural PHP

pfSense does not use any PHP framework (no Laravel, Symfony, etc.). Each page is a standalone PHP script that:

1. Includes shared libraries (`require_once("guiconfig.inc")`)
2. Processes POST data (action dispatch)
3. Reads config from global `$config` array
4. Renders HTML directly with Bootstrap CSS
5. Uses Post-Redirect-GET pattern for mutations

### 9.2 Page Naming Convention

Files follow `{section}_{subsection}.php` and `{section}_{subsection}_edit.php`:
- `firewall_rules.php` / `firewall_rules_edit.php`
- `firewall_nat.php` / `firewall_nat_edit.php`
- `interfaces.php` / `interfaces_assign.php`
- `diag_arp.php` / `diag_backup.php`

### 9.3 GUI Helper Functions (from guiconfig.inc)

| Function | Purpose |
|----------|---------|
| `print_info_box()` | Bootstrap alert messages |
| `print_apply_box()` | "Apply Changes" banner |
| `display_top_tabs()` | Tab navigation (pills or dropdown) |
| `genhtmltitle()` | Breadcrumb generation |
| `alias_info_popup()` | Bootstrap popover for address aliases |
| `gateway_info_popup()` | Status-colored gateway info table |
| `pprint_address()` | Human-readable address formatting |
| `update_if_changed()` | Change audit logging |
| `do_input_validation()` | Input validation with error collection |

### 9.4 CSRF Protection

- Uses `csrf-magic.js` with configurable session timeout (default 240 minutes)
- POST requests end current session: `phpsession_end(true)`
- No-cache headers on all pages

### 9.5 Inline Asset Loading

Functions minimize HTTP requests by inlining JS/CSS:
- `outputJavaScriptFileInline()` - wraps in `<script>` tags
- `outputCSSFileInline()` - wraps in `<style>` tags

## 10. REST API

### 10.1 Community API Package (pfSense-pkg-RESTAPI)

The REST API is a **community-contributed package** (by Jared Hendrickson), not part of core pfSense. It is installed separately.

**Architecture:**
- Model-based: each endpoint maps to a PHP model class
- 200+ REST endpoints + GraphQL API
- Token-based authentication
- OpenAPI/Swagger documentation built-in
- Versioned API (v2)

**Endpoint categories:**

| Category | Example Endpoints |
|----------|-------------------|
| System | `/api/v1/system/general`, `/api/v1/system/advanced` |
| Interfaces | `/api/v1/interfaces`, `/api/v1/interfaces/bridge` |
| Firewall | `/api/v1/firewall/rules`, `/api/v1/firewall/alias` |
| NAT | `/api/v1/firewall/_nat` |
| Routing | `/api/v1/system/routing/gateway` |
| Services | `/api/v1/services/dhcp`, `/api/v1/services/dns` |
| VPN | `/api/v1/vpn/ipsec/phase1`, `/api/v1/vpn/openvpn` |
| Diagnostics | `/api/v1/diagnostics/backup`, `/api/v1/diagnostics/command` |
| Users | `/api/v1/user`, `/api/v1/group` |

**HTTP methods:** GET (read), POST (create), PATCH (update), DELETE (remove)

### 10.2 Core pfSense "API" (Internal)

Core pfSense does not have a REST API. Inter-node communication uses **XML-RPC** for config synchronization between CARP partners.

## 11. Key Design Patterns for VectorOS Adoption

### 11.1 Config Version Migration System

**Pattern:** Sequential, numbered migration functions that transform config from one schema version to the next. Never skip a version number.

**VectorOS relevance:** Our TOML/YAML config could adopt a similar versioned migration approach. Each config version bump triggers a chain of migration functions.

```
Current:  config.toml v1
Migration: migrate_v1_to_v2() -> transforms fields, adds new defaults
```

### 11.2 Atomic Config Writes

**Pattern:** Write to temp file, fsync, then rename over the original. Provides crash-safe config updates.

**VectorOS relevance:** Implement for our VPP config and control plane state persistence.

### 11.3 Ruleset Hash-Based Change Detection

**Pattern:** Embed xxh3 hash of generated ruleset in a pf label. Compare active vs saved vs generated hashes to skip unnecessary reloads.

**VectorOS relevance:** We can hash VPP CLI commands or binary API call sequences to detect when a reload is actually needed.

### 11.4 Tracker ID System for State Correlation

**Pattern:** Every rule gets a unique integer tracker that correlates with data plane internals. Used for statistics, state killing, and audit.

**VectorOS relevance:** Tag VPP flow entries or rule objects with unique IDs for correlation between control plane config and data plane state.

### 11.5 Hierarchical Queue/Class Model for QoS

**Pattern:** Object-oriented class hierarchy where each queue type inherits from a base and implements `build_rules()`, `validate_input()`, `wconfig()`, `ReadConfig()`.

**VectorOS relevance:** Model VPP QoS (tc, traffic class, mark/translate) using a similar class hierarchy. Each VPP QoS primitive maps to a class.

### 11.6 Dual Backend Support

**Pattern:** Support multiple implementations (ISC DHCP / Kea DHCP, ALTQ / dummynet) behind a common interface, selected by configuration.

**VectorOS relevance:** Could support multiple VPP versions or alternative data plane backends (DPDK vs AF_XDP) behind a common API.

### 11.7 Post-Redirect-Get (PRG) for Web Mutations

**Pattern:** All form submissions redirect back to the list page after processing, preventing duplicate submissions on refresh.

**VectorOS relevance:** Our Svelte frontend already uses client-side routing, but the backend API should return proper HTTP status codes and redirect-like responses.

### 11.8 Change Audit Logging

**Pattern:** `update_if_changed()` tracks old vs new values, builds human-readable change descriptions, and records them in the config revision history.

**VectorOS relevance:** Implement config change tracking with who/when/what for audit trail in our control plane.

### 11.9 "Apply Changes" Deferred Activation

**Pattern:** Config changes are saved immediately but not applied to the data plane until the user clicks "Apply Changes." This allows batching multiple changes.

**VectorOS relevance:** VPP supports batch configuration. Save config changes first, then apply them atomically via VPP binary API.

### 11.10 Aliases / Named Objects

**Pattern:** Firewall addresses and ports can be aliases (named groups of hosts, networks, or ports). Aliases are expanded at ruleset generation time.

**VectorOS relevance:** Implement named network objects that expand to VPP ACL entries or classification rules.

## 12. Limitations and Anti-Patterns

### 12.1 No MVC / No Separation of Concerns

pfSense PHP pages mix business logic, data access, and HTML rendering in single files. This makes testing and maintenance difficult.

**VectorOS advantage:** Our Rust control plane + Svelte frontend naturally separates concerns.

### 12.2 Global State

The global `$config` array creates implicit coupling between all subsystems. Any function can read/write any config section.

**VectorOS advantage:** Rust's type system enforces ownership and module boundaries.

### 12.3 No Formal API in Core

Core pfSense lacks a REST API. Configuration is XML-RPC-only between nodes. The community REST API is an afterthought packaged separately.

**VectorOS advantage:** We build the API first-class from day one.

### 12.4 Single XML File Bottleneck

All configuration in one file means:
- Full file rewrite on every change
- Concurrent access requires locking
- Large configs slow down parsing
- Backup/restore is all-or-nothing (though section backup exists)

**VectorOS advantage:** TOML/YAML files per subsystem, or a database, avoid the single-file bottleneck.

### 12.5 Procedural PHP (No Type Safety)

No static analysis, no type hints (mostly), no interfaces. Functions operate on untyped arrays. Runtime errors are common.

**VectorOS advantage:** Rust's compile-time type safety catches configuration errors at build time.

---

## Summary

pfSense is a mature, battle-tested firewall system with 20+ years of development. Its core strengths are:

1. **Battle-tested config model**: XML-based with versioned migrations and atomic writes
2. **Complete feature set**: Firewall, NAT, VPN, QoS, DHCP, DNS, HA in one system
3. **pf integration**: Deep integration with FreeBSD's packet filter

Its main weaknesses from a modern engineering perspective are:

1. **Procedural PHP**: No framework, no type safety, no separation of concerns
2. **No core API**: Configuration via XML only, REST API is a community add-on
3. **Single-file config**: All settings in one XML file

For VectorOS, we can adopt pfSense's proven patterns (config versioning, change detection, aliases, deferred apply) while leveraging Rust's type safety and our API-first architecture.
