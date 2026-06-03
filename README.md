# VectorOS

A high-performance open-source router system based on VPP (Vector Packet Processing).

## Features

- **High-Performance Data Plane**: VPP-based packet processing with DPDK
- **Modern Control Plane**: Rust-based configuration and API management
- **Web UI**: Svelte-based router management interface
- **Routing**: FRRouting integration for BGP/OSPF
- **Network Services**: DHCP, DNS, NAT, PPPoE

## Quick Start

### Prerequisites

- Linux kernel 4.19+
- DPDK-compatible NIC
- Rust 1.70+
- Node.js 18+

### Installation

```bash
# Clone the repository
git clone https://github.com/Hi-Jiajun/vectoros.git
cd vectoros

# Build control plane
cargo build --release

# Build frontend
cd frontend
npm install
npm run build
cd ..

# Run
sudo ./target/release/vectoros --config config.toml
```

## Architecture

VectorOS uses a layered architecture:

1. **Data Plane (VPP)**: High-performance packet forwarding using graph-node architecture
2. **Control Plane (Rust)**: Configuration management, API server, network services
3. **Frontend (Svelte)**: Web-based management interface

## Documentation

See [CLAUDE.md](CLAUDE.md) for development guidelines and architecture details.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Apache-2.0
