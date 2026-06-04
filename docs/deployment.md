# VectorOS Deployment Guide

## Prerequisites

### Hardware Requirements

- **CPU**: 2+ cores (recommended: 4+ for high-throughput routing)
- **RAM**: 2GB minimum (recommended: 4GB+)
- **Storage**: 10GB minimum
- **NICs**: 2+ DPDK-compatible network interfaces (one WAN, one LAN)
- **Architecture**: x86_64

### Software Requirements

- Linux kernel 4.19+ (5.10+ recommended)
- DPDK 21.11+ installed and configured
- VPP 22.10+ compiled with PPPoE client plugin
- Rust 1.70+ (for building from source)
- Node.js 18+ (for building frontend)
- Python 3.8+ (for VPP management tools)
- FRRouting 8.0+ (optional, for BGP/OSPF)

### System Packages

```bash
# Debian/Ubuntu
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  python3 \
  python3-pip \
  libclang-dev \
  pkg-config \
  libssl-dev \
  git \
  curl

# For DPDK
sudo apt-get install -y \
  dpdk \
  dpdk-dev \
  rdma-core \
  libibverbs-dev

# For VPP
sudo apt-get install -y \
  vpp \
  vpp-plugin-core \
  vpp-plugin-dpdk
```

## Installation

### Option 1: Build from Source

```bash
# Clone with submodules
git clone --recursive https://github.com/Hi-Jiajun/vectoros.git
cd vectoros

# Build control plane
cargo build --release

# Build frontend
cd frontend
npm install
npm run build
cd ..

# The binary is at: target/release/vectoros
# The frontend is at: frontend/dist/
```

### Option 2: Pre-built Packages

Check the GitHub releases page for pre-built packages.

## System Setup

### 1. Create Directories

```bash
sudo mkdir -p /etc/vectoros
sudo mkdir -p /var/lib/vectoros
sudo mkdir -p /var/log/vectoros
```

### 2. Install Configuration

```bash
sudo cp config.toml /etc/vectoros/config.toml
sudo cp config/startup.vpp /etc/vectoros/startup.vpp
```

Edit `/etc/vectoros/config.toml` with your settings:

```toml
[vpp]
socket_path = "/run/vpp/api.sock"

[network]
wan_interface = "wan0"
lan_interface = "lan0"

[network.pppoe]
username = "your-username@isp.com"
password = "your-password"
interface = "wan0"

[network.pppoe.autoconnect]
enabled = true

[dhcp]
enabled = true
range_start = "192.168.1.100"
range_end = "192.168.1.200"
lease_time = 86400

[dns]
upstream = ["8.8.8.8", "1.1.1.1"]
cache_size = 1000
```

### 3. Install Binary

```bash
sudo cp target/release/vectoros /usr/local/bin/
sudo chmod +x /usr/local/bin/vectoros
```

### 4. Install Frontend

```bash
sudo cp -r frontend/dist /etc/vectoros/frontend/
```

### 5. Set Environment Variables

Create `/etc/vectoros/env`:

```bash
JWT_SECRET=your-secret-key-change-this
VECTOROS_USERNAME=admin
VECTOROS_PASSWORD=your-secure-password
RUST_LOG=info
```

### 6. Create systemd Service

Create `/etc/systemd/system/vectoros.service`:

```ini
[Unit]
Description=VectorOS Control Plane
After=network.target vpp.service
Requires=vpp.service

[Service]
Type=simple
User=root
EnvironmentFile=/etc/vectoros/env
ExecStart=/usr/local/bin/vectoros \
  --config /etc/vectoros/config.toml \
  --db /var/lib/vectoros/vectoros.db \
  --api-listen 0.0.0.0:8080
Restart=on-failure
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable vectoros
sudo systemctl start vectoros
```

## DPDK Configuration

### Bind NICs to DPDK

```bash
# Load vfio-pci driver
sudo modprobe vfio-pci

# Get PCI addresses
lspci | grep Ethernet

# Bind interface to vfio-pci
sudo dpdk-devbind.py -b vfio-pci 0000:01:00.0
sudo dpdk-devbind.py -b vfio-pci 0000:01:00.1

# Verify binding
sudo dpdk-devbind.py --status
```

### Hugepages

```bash
# Allocate 1GB hugepages
echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Verify
cat /proc/meminfo | grep Huge
```

## VPP Configuration

### Start VPP

```bash
# Start VPP service
sudo systemctl start vpp

# Or start manually
sudo vpp -c /etc/vpp/startup.conf
```

### VPP Startup Configuration

Create `/etc/vpp/startup.conf`:

```
unix {
  nodaemon
  log /var/log/vpp/vpp.log
  cli-listen /run/vpp/cli.sock
  full-coredump
}

api-trace {
  on
}

api-segment {
  gid vpp
}

socksvr {
  default
  socket-name /run/vpp/api.sock
}

plugins {
  plugin dpdk_plugin.so { enable }
  plugin pppoeclient_plugin.so { enable }
  plugin nat_plugin.so { enable }
  plugin dhcp_plugin.so { enable }
  plugin dns_plugin.so { enable }
}

logging {
  default-log-level info
}
```

### Interface Binding via RDMA

For RDMA host-interface binding (no driver change needed):

```bash
# Create RDMA host interfaces in VPP
vppctl create interface rdma host-if enp1s0 name wan0
vppctl create interface rdma host-if enp2s0 name lan0

# Configure interfaces
vppctl set interface ip address lan0 192.168.1.1/24
vppctl set interface mtu packet 1500 wan0
vppctl set interface mtu packet 1500 lan0
vppctl set interface state wan0 up
vppctl set interface state lan0 up
```

## FRRouting Setup (Optional)

For BGP/OSPF routing protocol support:

```bash
# Install FRRouting
sudo apt-get install -y frr frr-pythontools

# Enable required daemons
sudo sed -i 's/bgpd=no/bgpd=yes/' /etc/frr/daemons
sudo sed -i 's/ospfd=no/ospfd=yes/' /etc/frr/daemons

# Start FRR
sudo systemctl restart frr
```

## Network Topology

### Typical Deployment

```
Internet
    |
[ISP Modem] --- [WAN NIC: enp1s0] --- [VPP: wan0]
                                          |
                                     [VectorOS]
                                          |
                                     [VPP: lan0] --- [LAN NIC: enp2s0] --- [LAN Switch]
                                                                          |
                                                                    [LAN Clients]
```

### VLAN Configuration

For VLAN-tagged WAN connections:

```bash
# Create VLAN sub-interface in VPP
vppctl create sub-interface wan0 100
vppctl set interface state wan0.100 up
```

## Firewall Configuration

Enable the firewall via the API or web interface:

```bash
# Enable firewall
curl -X POST http://localhost:8080/api/firewall/enable \
  -H "Authorization: Bearer <token>"

# Add a basic rule (allow established connections)
curl -X POST http://localhost:8080/api/firewall/add-rule \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "action": "permit",
    "direction": "in",
    "protocol": "tcp",
    "description": "Allow established connections"
  }'
```

## Monitoring

### System Monitoring

The web interface provides real-time monitoring via WebSocket. Access the dashboard at `http://<router-ip>:8080`.

### Logs

View logs via API:

```bash
# Get recent logs
curl -X POST http://localhost:8080/api/logs \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"level": "info", "limit": 100}'
```

### Prometheus Metrics (Future)

A `/metrics` endpoint is planned for Prometheus integration.

## Backup and Restore

```bash
# Export configuration
curl http://localhost:8080/api/config/export \
  -H "Authorization: Bearer <token>" > backup.json

# Import configuration
curl -X POST http://localhost:8080/api/config/import \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d @backup.json
```

## Security Hardening

1. **Change default credentials** immediately after deployment
2. **Set a strong JWT secret** via the `JWT_SECRET` environment variable
3. **Restrict API access** to trusted networks (use firewall rules)
4. **Enable HTTPS** by placing a reverse proxy (nginx, Caddy) in front
5. **Keep the system updated** with latest security patches

```bash
# Change credentials via environment
export VECTOROS_USERNAME=myadmin
export VECTOROS_PASSWORD=strong-password-here
export JWT_SECRET=long-random-secret-string
```
