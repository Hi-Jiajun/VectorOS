# VectorOS

[中文](README.md) | [English](README.en.md)

---

A high-performance open-source router system based on VPP (Vector Packet Processing).

## Features

- **High-Performance Data Plane**: VPP-based userspace packet processing with DPDK/RDMA
- **PPPoE Dialer**: Built-in PPPoE Client plugin with CHAP/PAP auth, VLAN/QinQ, auto-reconnect, exponential backoff
- **Modern Control Plane**: Written in Rust (tokio + axum), memory-safe and high-performance
- **Web Management UI**: Svelte + Tailwind CSS with dark-theme dashboard
- **Routing**: FRRouting integration for BGP/OSPF via FPM
- **Network Services**: DHCP, DNS, NAT

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│              Frontend (Svelte + Tailwind)                   │
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

## Project Structure

```
vectoros/
├── Cargo.toml                      # Rust workspace
├── control-plane/                  # Rust control plane
│   └── src/
│       ├── main.rs                 # Entry point
│       ├── api/                    # REST API handlers
│       ├── config/                 # Configuration management
│       ├── vpp/                    # VPP Binary API client
│       └── services/               # DHCP, DNS services
├── frontend/                       # Svelte frontend
│   └── src/routes/                 # Page routes
├── vpp/                            # VPP source (git submodule)
│   └── src/plugins/pppoeclient/    # PPPoE Client plugin
└── vpp-plugins/                    # Additional VPP plugins
```

## Quick Start

### Prerequisites

- Linux kernel 4.19+
- DPDK-compatible NIC
- Rust 1.70+
- Node.js 18+

### Clone

```bash
git clone --recursive https://github.com/Hi-Jiajun/vectoros.git
cd vectoros
```

If already cloned without submodule:

```bash
git submodule update --init
```

### Build Control Plane

```bash
cargo build --release
```

### Build Frontend

```bash
cd frontend
npm install
npm run build
cd ..
```

### Run

```bash
sudo ./target/release/vectoros --config config.toml
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/config` | GET | Get current configuration |
| `/api/interfaces` | GET | List network interfaces |
| `/api/routes` | GET | List routing table |
| `/api/dhcp/leases` | GET | List DHCP leases |

## Configuration

Config path: `/etc/vectoros/config.toml`

```toml
[vpp]
socket_path = "/run/vpp/api.sock"

[network]
wan_interface = "eth0"
lan_interface = "eth1"

[network.pppoe]
username = "user"
password = "pass"
interface = "eth0"

[dhcp]
enabled = true
range_start = "192.168.1.100"
range_end = "192.168.1.200"
lease_time = 86400

[dns]
upstream = ["8.8.8.8", "1.1.1.1"]
cache_size = 1000
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/xxx`)
3. Commit your changes
4. Push to the branch (`git push origin feature/xxx`)
5. Create a Pull Request

## License

Apache-2.0
