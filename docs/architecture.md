# VectorOS Architecture

## System Overview

VectorOS is a VPP-based router system with a layered architecture that separates data plane, control plane, and management plane.

```
┌─────────────────────────────────────────────────────────────┐
│              Frontend (Svelte + Tailwind)                    │
│         Dashboard / Interfaces / Routes / DHCP / DNS        │
├─────────────────────────────────────────────────────────────┤
│            Control Plane (Rust + Axum)                      │
│     REST API / Config / DHCP / DNS / State Collection       │
│                    ↓ Binary API Socket                      │
├─────────────────────────────────────────────────────────────┤
│              VPP Data Plane (C, DPDK)                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  pppoeclient plugin (3-layer architecture)            │   │
│  │  PPPoE Discovery → PPPoX Shim → Embedded pppd        │   │
│  └──────────────────────────────────────────────────────┘   │
│  NAT / Routing / Interface Mgmt / VLAN / QinQ              │
├─────────────────────────────────────────────────────────────┤
│              FRRouting (BGP/OSPF) via FPM                   │
├─────────────────────────────────────────────────────────────┤
│              Linux + DPDK PMD / RDMA                        │
└─────────────────────────────────────────────────────────────┘
```

## Component Architecture

### 1. VPP Data Plane (C)

VPP (Vector Packet Processing) is the core data plane engine. It processes packets in batches (vectors) for maximum throughput using DPDK for userspace NIC access.

**Key characteristics:**
- Userspace packet processing via DPDK
- Graph-node architecture for modular packet processing pipelines
- Support for RDMA and DPDK PMD interface binding
- Native NAT44, DHCP, and IP forwarding
- PPPoE client plugin with 3-layer architecture

**Interface binding methods:**
- **RDMA host-interface**: Direct RDMA binding without driver changes (simplest)
- **Host interfaces**: Standard kernel interface passthrough
- **DPDK binding**: Full DPDK driver binding via vfio-pci

### 2. Control Plane (Rust)

The control plane is a Rust application built on tokio (async runtime) and Axum (web framework). It manages VPP configuration, runs network services, and exposes a REST API.

**Technology stack:**
- **Runtime**: tokio (async I/O, timers, channels)
- **Web framework**: Axum 0.7 with JSON serialization
- **VPP communication**: Binary API socket (`/run/vpp/api.sock`) + vppctl CLI
- **Database**: SQLite via rusqlite for configuration persistence
- **Authentication**: JWT Bearer tokens (jsonwebtoken + bcrypt)
- **OpenAPI**: Auto-generated via utoipa

**Module structure:**
```
control-plane/src/
  main.rs           -- Entry point, CLI parsing, service initialization
  api/              -- REST API (routes, handlers, WebSocket, OpenAPI)
  auth/             -- JWT authentication middleware
  config/           -- TOML configuration loader
  db/               -- SQLite database for config persistence
  vpp/              -- VPP binary API client and native command execution
  services/         -- Service implementations (DHCP, DNS, firewall, etc.)
```

### 3. Frontend (Svelte)

The frontend is a SvelteKit application with Tailwind CSS, served as static files by the control plane.

**Key features:**
- Dark-themed dashboard with real-time WebSocket updates
- Interface management (bind/unbind VFs, configure IP/MTU)
- PPPoE client management
- Firewall rule management with groups, aliases, and schedules
- FRRouting BGP/OSPF configuration
- VPN tunnel management
- Network diagnostics (ping, traceroute, DNS, port scan)
- VyOS-style hierarchical configuration management

**Pages:**
```
frontend/src/routes/
  +page.svelte              -- Dashboard
  interfaces/               -- Interface management
  pppoe/                    -- PPPoE client
  firewall/                 -- Firewall rules
  frr/                      -- FRRouting
  dhcp/                     -- DHCP server
  dns/                      -- DNS resolver
  vpn/                      -- VPN tunnels
  qos/                      -- QoS policers
  traffic/                  -- Traffic control
  flow/                     -- Flow monitoring
  conntrack/                -- Connection tracking
  config/                   -- Configuration management
  monitor/                  -- System monitoring
  logs/                     -- Log viewer
  diag/                     -- Network diagnostics
  services/                 -- Service management
  settings/                 -- System settings
  ipv6/                     -- IPv6 management
```

### 4. VPP Tools (Python)

Python scripts provide management operations that interface with VPP. These are called by the control plane for certain operations.

```
vpp-tools/
  pppoe_manager.py       -- PPPoE client management
  interface_bind.py      -- VF interface binding
  nat_manager.py         -- NAT configuration
  dhcp_manager.py        -- DHCP server management
  dns_manager.py         -- DNS resolver management
  firewall_manager.py    -- Firewall rules and groups
  frr_manager.py         -- FRRouting integration
  vpn_manager.py         -- VPN tunnel management
  traffic_control.py     -- Traffic shaping
  qos_manager.py         -- QoS policers
  conntrack_manager.py   -- Connection tracking
  flow_monitor.py        -- Flow monitoring
  vpp_stats.py           -- VPP performance statistics
  config_manager.py      -- Configuration persistence
  system_monitor.py      -- System resource monitoring
  diag_manager.py        -- Network diagnostics
  log_manager.py         -- Log management
  ipv6_manager.py        -- IPv6 management
  dhcpv6_manager.py      -- DHCPv6 management
  ebpf_manager.py        -- eBPF program management
  backup_manager.py      -- Configuration backup
  monitor.py             -- System monitor
  config_cli.py          -- VyOS-style CLI configuration
```

## Data Flow

### API Request Flow

```
Browser → HTTP Request → Axum Router → Auth Middleware → Handler
                                                         ↓
                                              ┌──────────────────────┐
                                              │  Service Layer       │
                                              │  (services/*.rs)     │
                                              └──────────┬───────────┘
                                                         ↓
                                              ┌──────────────────────┐
                                              │  VPP Interface       │
                                              │  (vppctl CLI or      │
                                              │   binary API)        │
                                              └──────────┬───────────┘
                                                         ↓
                                                    VPP Data Plane
```

### WebSocket Real-time Updates

```
Browser ← WebSocket ← Stats Broadcaster (2s interval)
                           ↓
                    System Info (/proc)
                    VPP Performance (vpp_stats.py)
                    PPPoE Status (vppctl)
```

### Configuration Management Flow (VyOS-style)

```
API Request → set_value() → Staging Tree → commit() → Active Config
                        ↘ discard() → discard staging
                        ↘ rollback(version) → restore previous
```

## Service Manager

The ServiceManager orchestrates the lifecycle of all managed services with a strict state machine:

```
Stopped → Starting → Running → Stopping → Stopped
                   ↘ Failed ↗
```

**Managed services:**
- PPPoE Client
- DHCP Server
- DNS Resolver
- NAT
- Firewall
- VPN

**Features:**
- Automatic rollback on failed restarts
- Runtime state synchronization via `probe()`
- Hot-reload support for configuration changes

## VPP Communication

The control plane communicates with VPP through two channels:

1. **vppctl CLI** -- Used for most operational commands (interface management, NAT, PPPoE status). Invoked via `std::process::Command`.

2. **Binary API Socket** -- Used for direct VPP API calls. The `VppClient` connects to `/run/vpp/api.sock` and uses the VPP binary API protocol for message-based communication.

```
Control Plane
  ├── vppctl (CLI) → "show interface", "set interface state", etc.
  └── VppClient (Binary API) → VPP API socket
       ├── Message encoding/decoding
       ├── Plugin message ID lookup
       └── PPPoE client API
```

## Database Schema

SQLite database at `/var/lib/vectoros/vectoros.db`:

| Table | Purpose |
|-------|---------|
| `config` | Key-value configuration storage |
| `config_history` | Configuration version history |
| `interfaces` | Interface configurations |
| `pppoe_sessions` | PPPoE session records |
| `firewall_rules` | Firewall rule definitions |
| `dhcp_leases` | DHCP lease records |
| `vpn_connections` | VPN connection records |
| `system_logs` | System log entries |

## Authentication

JWT Bearer token authentication protects all API endpoints except:
- `GET /api/health` (health probes)
- `POST /api/auth/login` (login endpoint)

**Token flow:**
1. Client sends credentials to `/api/auth/login`
2. Server validates credentials and returns a JWT (24-hour expiry)
3. Client includes `Authorization: Bearer <token>` header on subsequent requests
4. Middleware validates the token on each request

**Default credentials:** `admin` / `vectoros` (configurable via environment variables `VECTOROS_USERNAME` and `VECTOROS_PASSWORD`).

**JWT secret:** Configurable via `JWT_SECRET` environment variable (defaults to `vectoros-secret-key`).
