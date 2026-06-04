# VectorOS Documentation

VectorOS is a high-performance open-source router system built on VPP (Vector Packet Processing). This documentation covers architecture, configuration, API reference, deployment, development, and troubleshooting.

## Documentation Index

| Document | Description |
|----------|-------------|
| [Architecture](architecture.md) | System architecture, component design, and data flow |
| [API Reference](api-reference.md) | REST API endpoints, request/response formats, and authentication |
| [Configuration](configuration.md) | Configuration file format, options, and management |
| [Deployment](deployment.md) | Installation, deployment, and production setup |
| [Development](development.md) | Development environment setup, contributing, and code structure |
| [Troubleshooting](troubleshooting.md) | Common issues and solutions |

## Quick Start

```bash
# Clone with submodules
git clone --recursive https://github.com/Hi-Jiajun/vectoros.git
cd vectoros

# Build control plane
cargo build --release

# Build frontend
cd frontend && npm install && npm run build && cd ..

# Run
sudo ./target/release/vectoros --config config.toml
```

The web interface is available at `http://<router-ip>:8080`.

Default credentials: `admin` / `vectoros`

## System Overview

```
Frontend (Svelte + Tailwind)
        |
Control Plane (Rust + Axum)
        |
VPP Data Plane (C, DPDK)
        |
Linux + DPDK PMD / RDMA
```

VectorOS uses VPP for wire-speed packet processing with DPDK, a Rust control plane for management and API services, and a Svelte web interface for configuration and monitoring.

## Key Features

- **PPPoE Client** with CHAP/PAP authentication, VLAN/QinQ, auto-reconnect
- **Firewall** with rules, groups, aliases, schedules, GeoIP, and IDS integration
- **NAT** (NAT44) for internet sharing
- **DHCP/DNS** server for LAN clients
- **FRRouting** integration for BGP/OSPF routing protocols
- **VPN** support for WireGuard, IPsec, and OpenVPN
- **QoS** with policers, traffic shaping, and rate limiting
- **Connection tracking** and flow monitoring
- **VyOS-style** hierarchical configuration with staging and rollback
- **Real-time monitoring** via WebSocket
- **OpenAPI documentation** auto-generated from code
