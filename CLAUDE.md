# VectorOS - VPP-based Open Source Router

## Architecture

```
┌─────────────────────────────────────┐
│     Frontend (Svelte + Tailwind)    │
├─────────────────────────────────────┤
│    Control Plane (Rust + Axum)      │
│   Config / API / DHCP / DNS         │
├─────────────────────────────────────┤
│   FRRouting (BGP/OSPF) via FPM      │
├─────────────────────────────────────┤
│    VPP Data Plane (C, DPDK)         │
│   Graph-node forwarding engine      │
├─────────────────────────────────────┤
│        Linux + DPDK PMD             │
└─────────────────────────────────────┘
```

## Tech Stack

- **Data Plane**: VPP (C) - High-performance userspace packet processing
- **Control Plane**: Rust (tokio + axum) - Configuration, API, services
- **Frontend**: Svelte + Tailwind CSS - Router management UI
- **Routing**: FRRouting - BGP/OSPF via FPM socket

## Project Structure

```
vectoros/
├── Cargo.toml              # Rust workspace
├── control-plane/          # Rust control plane
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # Entry point
│       ├── api/            # REST API handlers
│       ├── config/         # Configuration management
│       ├── vpp/            # VPP API client
│       └── services/       # DHCP, DNS, etc.
├── frontend/               # Svelte frontend
│   ├── package.json
│   ├── src/
│   │   ├── routes/         # SvelteKit pages
│   │   ├── lib/            # Shared components
│   │   └── app.html        # HTML template
│   └── static/             # Static assets
├── vpp-plugins/            # VPP C plugins
│   └── pppoe-client/       # PPPoE client plugin
└── docs/                   # Documentation
```

## Development

### Control Plane (Rust)

```bash
# Build
cargo build

# Run
cargo run -- --config config.toml --api-listen 0.0.0.0:8080

# Test
cargo test
```

### Frontend (Svelte)

```bash
cd frontend

# Install dependencies
npm install

# Development server
npm run dev

# Build for production
npm run build

# Type check
npm run check
```

### VPP Integration

The control plane communicates with VPP via binary API socket:
- Default socket: `/run/vpp/api.sock`
- Protocol: VPP binary API (JSON/binary over Unix socket)

## API Endpoints

- `GET /api/health` - Health check
- `GET /api/config` - Get current configuration
- `GET /api/interfaces` - List network interfaces
- `GET /api/routes` - List routing table
- `GET /api/dhcp/leases` - List DHCP leases

## Configuration

Config file: `/etc/vectoros/config.toml`

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
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## License

Apache-2.0
