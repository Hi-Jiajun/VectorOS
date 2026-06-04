# Landscape Router - Comprehensive Architecture Analysis

## Overview

Landscape is an eBPF-based Linux router platform written in Rust. It turns standard Linux distributions into full-featured routers with network service management, policy control, and APIs. The project uses eBPF (TC classifiers) for high-performance packet processing in the kernel, Rust for the control plane, and Vue 3 for the web UI.

**Version**: 0.19.2  
**License**: GPL v3 (GPL v2 for eBPF code)  
**Kernel Requirement**: Linux 6.9+ with BTF/BPF support  
**Privileges**: Root required  

---

## 1. Workspace Architecture

### Crate Structure

```
landscape/                          # Workspace root
├── landscape/                      # Core library (services, netlink, protocols)
├── landscape-common/               # Shared types, config, traits, database abstractions
├── landscape-database/             # SeaORM entities, repositories, migrations (SQLite)
├── landscape-dns/                  # DNS server (Hickory), per-flow DNS, DoH
├── landscape-ebpf/                 # eBPF programs (C) + Rust loader (libbpf-rs)
├── landscape-gateway/              # HTTP/HTTPS reverse proxy (Pingora-based)
├── landscape-macro/                # Proc macros (LdApiError, ExportTsEnum, etc.)
├── landscape-protobuf/             # Protobuf definitions (quick-protobuf)
├── landscape-types/                # Auto-generated TypeScript types (from OpenAPI)
├── landscape-webserver/            # Binary entry point, Axum web server, REST API
└── landscape-webui/                # Vue 3 + Naive UI frontend
```

**Default member**: `landscape-webserver` (the main binary)

### Dependency Flow

```
landscape-webserver
  ├── landscape (core library)
  │     ├── landscape-ebpf (eBPF loader + maps)
  │     ├── landscape-common (shared types)
  │     ├── landscape-database (SQLite via SeaORM)
  │     ├── landscape-dns (DNS server)
  │     └── landscape-protobuf
  ├── landscape-common (with openapi feature)
  ├── landscape-ebpf
  ├── landscape-gateway (optional, feature-gated)
  └── landscape-database
```

---

## 2. Core Architecture Patterns

### 2.1 Service Manager Pattern

The central abstraction is `ServiceManager<H: ServiceStarterTrait>`, which manages per-interface services with lifecycle control.

**Key types**:
- `WatchService` - wraps a `watch::Sender<ServiceStatus>` for state tracking
- `ServiceStatus` - enum: `Staring | Running | Stopping | Stop | Failed`
- `ServiceManager<H>` - maintains `HashMap<String, (WatchService, mpsc::Sender<Config>)>` per service
- `ServiceStarterTrait` - async trait with `start(config) -> WatchService`

**How it works**:
1. Each interface service (NAT, firewall, MSS clamp, etc.) gets a `ServiceManager`
2. When config changes, `update_service()` sends new config via mpsc channel
3. The service task receives config, stops old service, starts new one
4. Status transitions are validated via `can_transition_to()` method

**ControllerService trait** provides CRUD operations:
- `handle_service_config()` - validates conflicts, starts service, persists to DB
- `delete_and_stop_iface_service()` - removes from DB and stops
- Automatic rollback on DB write failure

**ConfigController trait** for non-service configs:
- `set()`, `checked_set()`, `delete()` with `after_update_config()` hook
- Conflict checking via `check_conflict()` before updates

### 2.2 Database Layer

**ORM**: SeaORM with SQLite backend  
**Migration**: `sea-orm-migration` with 40+ migration files  
**Pattern**: Repository pattern per entity

Each repository follows:
```rust
struct SomeRepository {
    database: DatabaseConnection,
}
impl Repository for SomeRepository {
    type Entity = some_entity::Entity;
    type ActiveModel = some_entity::ActiveModel;
    // CRUD operations
}
```

**LandscapeDBServiceProvider** is the central DB access point, generated via macro:
```rust
define_store!(
    iface_store: (NetIfaceRepository, ifaces),
    dhcp_v4_server_store: (DHCPv4ServerRepository, dhcpv4_services),
    // ... 30+ stores
);
```

Each store getter returns a repository instance. The provider supports:
- `truncate_and_fit_from()` - atomic import with rollback
- `validate_init_config_can_import()` - dry-run validation in memory DB

### 2.3 Configuration Management

**Two-layer config**:
1. **File config** (`landscape_init.toml`): Auth, web, log, store, metric, DNS, UI, time, gateway settings
2. **Database config**: All per-interface services, flow rules, DNS rules, firewall rules, etc.

**Runtime config** (`RuntimeConfig`):
- Loaded from TOML file at startup
- Merged with defaults
- Supports hot-reload for some settings (auth, DNS, metric, time)

**Init config** (`InitConfig`):
- TOML file with all database configs bundled
- Supports import/export for migration between routers
- Version validation ensures compatibility
- Atomic import with rollback on failure

### 2.4 API Design

**Framework**: Axum 0.8 with utoipa OpenAPI generation  
**Auth**: JWT Bearer tokens with auto-refresh  
**Documentation**: Scalar API docs at `/api/docs`

**Route structure**:
```
/api/auth/login          # POST - login
/api/v1/interfaces/      # Network interfaces
/api/v1/services/        # Per-interface services (NAT, firewall, etc.)
/api/v1/dns/             # DNS rules, redirects, upstreams
/api/v1/flow/            # Flow rules, dst IP rules
/api/v1/nat/             # Static NAT mappings
/api/v1/geo/             # Geo sites, geo IPs
/api/v1/devices/         # Enrolled devices
/api/v1/cert/            # ACME certificates
/api/v1/docker/          # Docker management
/api/v1/metrics/         # Monitoring data
/api/v1/gateway/         # Reverse proxy rules
/api/v1/system/          # System config, sysinfo
/api/ws/docker/          # WebSocket for Docker tasks
/api/ws/pty/             # WebSocket for web terminal
```

**API response format**:
```json
{
  "code": 0,
  "msg": "success",
  "data": { ... }
}
```

**Error handling**: Custom `LandscapeApiError` enum with `LdApiError` derive macro that auto-generates:
- Error ID strings (e.g., `"service.zone_mismatch"`)
- HTTP status codes
- Structured error responses

**OpenAPI generation**: Each domain builds its own `OpenApiRouter`, then specs are merged with correct URL prefixes and tag groups for Scalar UI sidebar.

---

## 3. eBPF Traffic Steering

### 3.1 TC Pipeline Architecture

Landscape uses Linux Traffic Control (TC) with eBPF programs organized as **tail-call pipelines**.

**WAN pipeline** (5 stages each direction):
```
Ingress: PPPOE -> MSS -> Firewall -> NAT -> WAN Route
Egress:  WAN Route -> MSS -> NAT -> Firewall -> PPPOE
```

**LAN pipeline** (1 stage, extensible):
```
Ingress: Route
Egress:  Route
```

Each stage is a BPF program registered in a `BPF_MAP_TYPE_PROG_ARRAY`. Programs return `TC_ACT_UNSPEC` to continue to next stage, or `TC_ACT_OK/TC_ACT_SHOT/TC_ACT_REDIRECT` to terminate.

**Key insight**: Stages are dynamically registered/unregistered. When NAT is enabled on an interface, its BPF programs are inserted into the pipeline's prog array. When disabled, the slot is deleted. This means:
- Direct traffic continues working even if a component fails
- No performance overhead for unused features
- Hot-pluggable service composition

### 3.2 Flow Matching (Traffic Steering)

The core traffic steering uses an **LPM trie** (`BPF_MAP_TYPE_LPM_TRIE`) with two match modes:

1. **MAC matching** (prefix length 80): Match by source MAC address
2. **IP matching** (prefix length 32-160): Match by source IP/CIDR

**Flow ID** (u32) is assigned to each matched packet. The flow ID determines:
- Which WAN interface to use
- DNS resolver configuration
- NAT behavior
- Firewall rules

**Flow mark encoding** (u32):
```
Bits 0-7:   Flow ID (0-255)
Bits 8-14:  Action (KeepGoing=0, Direct=1, Drop=2, Redirect=3)
Bit 15:     Allow reuse port flag
```

**DNS-based flow matching**: When a DNS response comes back, resolved IPs are written into eBPF maps (`flow4_dns_map`, `flow6_dns_map`) with the flow's mark. This allows domain-based traffic steering without deep packet inspection.

### 3.3 NAT Implementation

**NAT v3** (`land_nat_v3.bpf.c`):
- Custom eBPF NAT implementation (not conntrack-based)
- Uses port queues (`BPF_MAP_TYPE_QUEUE`) for free port allocation
- Supports IPv4 and IPv6 NAT
- Separate ingress/egress programs via tail-call arrays
- Port range configuration per protocol (TCP, UDP, ICMP)

**Static NAT mappings**: Stored in eBPF maps, support:
- WAN port -> LAN IP:port mapping
- Multiple L4 protocols per mapping
- Device-based targeting (enrolled devices)

**Stricter NAT4**: Default behavior restricts NAT type, with per-IP/domain overrides for NAT1 (full cone) when needed.

### 3.4 Firewall

**eBPF firewall** (`firewall.bpf.c`):
- Runs as a stage in the WAN pipeline
- IPv4 and IPv6 block lists stored in BPF maps
- Allow rules for specific protocols
- Per-interface configuration

**Blacklist system**: GeoIP-based and domain-based blacklists that dynamically populate eBPF maps.

### 3.5 MSS Clamping

**eBPF MSS clamp** (`mss_clamp.bpf.c`):
- Modifies TCP MSS option in SYN packets
- Per-interface configuration
- Runs as a pipeline stage

### 3.6 Pinned Maps

All eBPF maps are pinned to `/sys/fs/bpf/landscape/<instance>/` for persistence across program restarts. Map paths are organized by function:
- `wan_ip_binding`, `nat4_st_map`, `nat6_static_mappings`
- `firewall_block_ip4_map`, `firewall_block_ip6_map`
- `flow_match_map`, `flow4_dns_map`, `flow4_ip_map`
- `rt4_lan_map`, `rt4_target_slot_map`
- `dns_flow_socks`, `metric_map`

---

## 4. DNS System

### 4.1 Per-Flow DNS

Each flow ID gets its own DNS resolver instance with:
- Independent upstream configuration
- Independent cache
- Independent rules and redirects
- Optional DoH (DNS over HTTPS) listener

**Implementation** (`landscape-dns`):
- Built on Hickory DNS (formerly Trust-DNS)
- UDP listener per flow on a shared port (SO_REUSEPORT)
- Optional DoH listener per flow
- Moka cache with configurable TTL

### 4.2 DNS Rules

Rules are evaluated in priority order:
1. **Redirect rules**: Map domains to specific answers (local IPs, upstream servers)
2. **DNS rules**: Control which upstream resolver handles specific domains
3. **Default**: Use the flow's default upstream

**DNS marks**: When DNS rules match, a mark is generated that gets written to eBPF maps. This allows the kernel to steer traffic for resolved domains without userspace involvement.

### 4.3 DNS Redirect

Supports multiple answer modes:
- `AllLocalIps`: Return all local interface IPs
- `SpecificIp`: Return configured IP addresses
- Dynamic redirects from gateway rules (auto-populated)

### 4.4 System DNS

On startup, Landscape takes over `/etc/resolv.conf` (backs up original), pointing it to `127.0.0.1`. On shutdown, it restores the original file.

---

## 5. Gateway (Reverse Proxy)

**Based on**: Pingora (Cloudflare's proxy framework)  
**Feature-gated**: Optional `gateway` feature

**Capabilities**:
- HTTP/HTTPS reverse proxy with TLS termination
- Host-based routing (domain matching)
- SNI proxy mode (passthrough TLS)
- Path-group routing with prefix matching
- Load balancing (round-robin, weighted)
- Request header injection
- Client IP header forwarding (X-Forwarded-For, etc.)
- Health checking
- Dynamic DNS redirect sync (auto-creates DNS redirects for gateway domains)

**Rule validation**:
- Domain conflict detection (exact match, wildcard overlap)
- Path prefix deduplication
- Header name/value validation
- Upstream target validation

---

## 6. Frontend

**Framework**: Vue 3 with Composition API  
**UI Library**: Naive UI  
**State Management**: Pinia with persisted state  
**Build Tool**: Vite 7  
**i18n**: vue-i18n  
**Charts**: ApexCharts  
**Terminal**: xterm.js  
**Flow visualization**: @vue-flow/core  

**Key pages**:
- Dashboard (Landscape view with topology)
- Service status pages (NAT, firewall, DHCP, routing, etc.)
- Flow rules configuration
- DNS rules and redirects
- Static NAT mappings
- Firewall management
- Geo site/IP management
- Docker management
- Certificate management
- Gateway configuration
- System configuration
- Enrolled devices
- Metrics and monitoring

**API communication**:
- Auto-generated TypeScript types from OpenAPI spec (via `orval`)
- Axios-based HTTP client
- WebSocket for Docker tasks and web terminal
- Pinia stores with interval-based polling for real-time status

---

## 7. Key Design Patterns Worth Adopting

### 7.1 Service Manager with Lifecycle Control

The `ServiceManager<Starter>` pattern provides:
- Per-interface service isolation
- Graceful restart on config change (stop old, start new)
- Status tracking with validated transitions
- Automatic rollback on failure

**For VectorOS**: This pattern maps directly to managing VPP graph nodes per interface.

### 7.2 eBPF TC Pipeline with Dynamic Stages

The tail-call pipeline architecture allows:
- Hot-pluggable packet processing stages
- Zero overhead for disabled features
- Graceful degradation (direct traffic continues if proxy fails)
- Composable service chains

**For VectorOS**: Could be adapted for VPP graph node composition, though VPP has its own graph scheduler.

### 7.3 Flow-Based Traffic Steering

The flow ID concept with LPM trie matching:
- MAC + IP matching for device identification
- DNS-based domain matching (resolved IPs mapped to flows)
- Per-flow DNS, NAT, and firewall behavior
- Encoded marks in skb->mark for kernel-level decisions

**For VectorOS**: The flow concept could map to VPP's session/feature infrastructure.

### 7.4 Database-Driven Configuration with Atomic Import/Export

- All configs stored in SQLite with SeaORM
- TOML-based init config for migration
- Atomic import with rollback
- Version validation
- Conflict checking before updates

**For VectorOS**: This pattern for config management is directly applicable.

### 7.5 OpenAPI-First API Design

- utoipa derive macros on handler functions
- Per-domain router composition
- Auto-generated TypeScript bindings
- Scalar API docs UI
- Structured error responses with IDs

### 7.6 Two-Layer Config Architecture

- File config for system-level settings (auth, web, log)
- Database config for all feature configs
- Hot-reload support for some settings
- Runtime config struct with defaults

---

## 8. Comparison with VectorOS Approach

| Aspect | Landscape | VectorOS |
|--------|-----------|----------|
| Data Plane | eBPF TC classifiers | VPP (DPDK) |
| Packet Processing | Kernel-space eBPF | Userspace VPP graph |
| Control Plane | Rust (Axum) | Rust (Axum) |
| Frontend | Vue 3 + Naive UI | Svelte + Tailwind |
| Database | SQLite (SeaORM) | TOML config files |
| DNS | Hickory DNS (per-flow) | To be implemented |
| NAT | Custom eBPF NAT | VPP NAT plugin |
| Routing | eBPF + Linux routing | VPP IP fib |
| Firewall | eBPF blacklist/allow | VPP ACL plugin |
| Gateway | Pingora reverse proxy | To be implemented |
| Config Migration | TOML import/export | N/A |
| API Docs | Scalar (utoipa) | N/A |

### Key Differences

1. **Packet processing location**: Landscape processes packets in kernel-space (eBPF), while VectorOS uses userspace VPP. This means Landscape has lower latency but less flexibility for complex processing.

2. **Configuration storage**: Landscape uses SQLite with SeaORM, VectorOS uses TOML files. Landscape's approach is better for dynamic configuration via API.

3. **Flow concept**: Landscape's flow ID system is a powerful abstraction for per-device/per-application policy. VectorOS could benefit from a similar concept mapped to VPP's session infrastructure.

4. **DNS architecture**: Landscape's per-flow DNS with eBPF integration is sophisticated. VectorOS currently lacks DNS.

5. **Gateway**: Landscape includes a full reverse proxy. VectorOS could integrate a similar feature.

---

## 9. Features to Consider Adopting

### High Priority
1. **Service Manager pattern** - Directly applicable for VPP service lifecycle
2. **Database-driven config** - Replace TOML with SQLite for dynamic config
3. **OpenAPI-first API** - Auto-generate docs and TypeScript bindings
4. **Flow-based traffic steering** - Map to VPP session/feature infrastructure
5. **Per-flow DNS** - Critical for router functionality

### Medium Priority
6. **eBPF TC pipeline concept** - Adapt for VPP graph node composition
7. **Config import/export** - Essential for router migration
8. **Structured error responses** - Improve API consistency
9. **WebSocket for real-time updates** - Docker tasks, terminal, status
10. **Gateway/reverse proxy** - Common router feature

### Low Priority
11. **GeoIP/GeoSite management** - Nice-to-have for advanced routing
12. **ACME certificate management** - Useful for HTTPS gateway
13. **DDNS support** - Common router feature
14. **Enrolled device management** - Per-device policy
15. **Web terminal** - Debugging convenience

---

## 10. Technical Details

### Key Dependencies

**Rust**:
- `axum` 0.8 - Web framework
- `sea-orm` 1.1 - Database ORM
- `libbpf-rs` 0.26 - eBPF loader
- `hickory-*` 0.25 - DNS implementation
- `pingora` 0.8 - Reverse proxy
- `utoipa` 5 - OpenAPI generation
- `tokio` 1.49 - Async runtime
- `arc-swap` 1.8 - Lock-free config swapping

**Frontend**:
- `vue` 3.4 - UI framework
- `naive-ui` 2.41 - Component library
- `pinia` 3.0 - State management
- `vue-router` 5.0 - Routing
- `apexcharts` 5.7 - Charts
- `@vue-flow/core` 1.43 - Flow visualization
- `@xterm/xterm` 6.0 - Terminal emulator

**Build**:
- `libbpf-cargo` 0.26 - eBPF build integration
- `vmlinux` - Kernel type definitions
- `orval` 8.4 - TypeScript codegen from OpenAPI

### Performance Considerations

- eBPF programs run in kernel-space with minimal overhead
- TC classifier priority ordering ensures correct processing order
- Pinned maps survive program restarts
- SO_REUSEPORT for DNS allows multiple resolver instances
- Moka cache with configurable TTL for DNS
- DuckDB for metrics (optional, feature-gated)

### Security

- JWT authentication with auto-refresh
- System token for API automation (file-based, 0o400 permissions)
- TLS with ACME certificate support
- Bearer token auth for all API endpoints
- WebSocket auth via query string token
