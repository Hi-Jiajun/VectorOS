# VectorOS - VPP-based Open Source Router

## Core Philosophy

**VPP 优先原则：非必要不使用外部依赖**

- 优先使用 VPP 自带功能，避免引入未使用 VPP 能力的外部项目
- VPP 缺少的功能：适合 VPP 开发的提交官方 PR，不适合的作为 VectorOS 专属功能
- VPP 已有功能必须直接使用（NAT、ACL、隧道、VPN 等）

## Architecture

```
┌─────────────────────────────────────┐
│     Frontend (Svelte + Tailwind)    │
├─────────────────────────────────────┤
│    Control Plane (Rust + Axum)      │
│   Config / API / Auth / Security    │
├─────────────────────────────────────┤
│        VPP Data Plane (C)           │
│  PPPoE / NAT / ACL / VPN / Tunnel   │
│   Graph-node forwarding engine      │
├─────────────────────────────────────┤
│        Linux + DPDK/RDMA            │
└─────────────────────────────────────┘
```

## Tech Stack

- **Data Plane**: VPP (C) - High-performance userspace packet processing
- **Control Plane**: Rust (tokio + axum) - Configuration, API, services
- **Frontend**: Svelte + Tailwind CSS - Router management UI

## Project Structure

```
vectoros/
├── Cargo.toml              # Rust workspace
├── control-plane/          # Rust control plane
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # Entry point
│       ├── api/            # REST API handlers
│       ├── auth/           # JWT authentication
│       ├── config/         # Configuration management
│       ├── db/             # SQLite database
│       ├── security/       # Security middleware
│       ├── services/       # Service implementations
│       └── vpp/            # VPP API client
├── frontend/               # Svelte frontend
│   ├── dist/               # Built frontend
│   └── src/
│       └── routes/         # SvelteKit pages
├── vpp/                    # VPP source (submodule)
│   └── src/plugins/pppoeclient/  # PPPoE client plugin
├── vpp-tools/              # Python VPP tools
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

## VPP Features (直接使用)

| 功能 | VPP 插件 | 用途 |
|------|----------|------|
| PPPoE | pppoeclient | 宽带拨号 |
| NAT | nat, cnat | 地址转换 |
| 防火墙 | acl | 访问控制 |
| 隧道 | gre, vxlan, geneve | Overlay 网络 |
| VPN | ikev2, wireguard, l2tp | 远程接入 |
| IPv6 | 内置 | 完整 IPv6 支持 |
| QoS | 内置 | 流量整形 |
| 监控 | flowprobe, ipfix | 流量分析 |

## Contributing

1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## License

Apache-2.0
