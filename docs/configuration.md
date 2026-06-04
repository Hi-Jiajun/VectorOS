# VectorOS Configuration Guide

## Overview

VectorOS uses a TOML configuration file for system settings. The configuration is loaded at startup and can be modified via the API or web interface.

**Default path:** `/etc/vectoros/config.toml`

## Configuration File Format

```toml
# VectorOS Configuration File
# Location: /etc/vectoros/config.toml

[vpp]
socket_path = "/run/vpp/api.sock"

[network]
wan_interface = "eth0"
lan_interface = "eth1"

[network.pppoe]
username = "user@isp.com"
password = "your-password"
interface = "eth0"

[network.pppoe.autoconnect]
enabled = true
max_retries = 0
retry_interval = 5
backoff_factor = 2.0
max_retry_interval = 300
check_interval = 10
health_check_interval = 60

[dhcp]
enabled = true
range_start = "192.168.1.100"
range_end = "192.168.1.200"
lease_time = 86400

[dns]
upstream = ["8.8.8.8", "1.1.1.1"]
cache_size = 1000
```

## Configuration Sections

### [vpp]

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `socket_path` | string | `/run/vpp/api.sock` | Path to VPP binary API socket |

### [network]

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `wan_interface` | string | none | WAN interface name (e.g., `eth0`, `wan0`) |
| `lan_interface` | string | none | LAN interface name (e.g., `eth1`, `lan0`) |

### [network.pppoe]

PPPoE client configuration for ISP connections.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `username` | string | required | PPPoE username (provided by ISP) |
| `password` | string | required | PPPoE password (provided by ISP) |
| `interface` | string | required | Physical interface for PPPoE (e.g., `eth0`) |

### [network.pppoe.autoconnect]

Automatic reconnection settings for PPPoE.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable auto-connect |
| `max_retries` | int | `0` | Maximum retries before giving up (0 = infinite) |
| `retry_interval` | int | `5` | Initial retry interval in seconds |
| `backoff_factor` | float | `2.0` | Exponential backoff multiplier |
| `max_retry_interval` | int | `300` | Maximum retry interval cap in seconds |
| `check_interval` | int | `10` | Interval between status checks in seconds |
| `health_check_interval` | int | `60` | Health check interval while connected in seconds |

### [dhcp]

DHCP server configuration for LAN clients.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable DHCP server |
| `range_start` | string | none | First IP in DHCP pool (e.g., `192.168.1.100`) |
| `range_end` | string | none | Last IP in DHCP pool (e.g., `192.168.1.200`) |
| `lease_time` | int | `86400` | Lease time in seconds (default: 24 hours) |

### [dns]

DNS resolver configuration.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `upstream` | list | `["8.8.8.8", "1.1.1.1"]` | Upstream DNS servers |
| `cache_size` | int | `1000` | DNS cache size (number of entries) |

## VyOS-Style Configuration Management

VectorOS supports VyOS-style hierarchical configuration with staging, commit, and rollback.

### Configuration Workflow

1. **Stage changes** -- Set values in the staging tree (not applied yet)
2. **Review** -- View staged changes and diff against active config
3. **Commit** -- Apply staged changes to the running configuration
4. **Rollback** -- Revert to a previous configuration version

### Example: Stage and Commit

```bash
# Stage a value
curl -X POST http://localhost:8080/api/config/set \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"path": "network.pppoe.username", "value": "newuser@isp.com"}'

# View staged changes
curl http://localhost:8080/api/config/diff \
  -H "Authorization: Bearer <token>"

# Commit changes
curl -X POST http://localhost:8080/api/config/commit \
  -H "Authorization: Bearer <token>"

# Or discard without committing
curl -X POST http://localhost:8080/api/config/discard \
  -H "Authorization: Bearer <token>"
```

### Rollback

```bash
# List configuration history
curl http://localhost:8080/api/config/history \
  -H "Authorization: Bearer <token>"

# Rollback to a specific version
curl -X POST http://localhost:8080/api/config/rollback/v3 \
  -H "Authorization: Bearer <token>"
```

### Configuration Templates

Save and reuse configuration snapshots:

```bash
# Save current config as template
curl -X POST http://localhost:8080/api/config/template/save \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name": "office-router", "description": "Standard office config"}'

# Apply template
curl -X POST http://localhost:8080/api/config/template/apply \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name": "office-router"}'
```

### Import/Export

```bash
# Export full configuration
curl http://localhost:8080/api/config/export \
  -H "Authorization: Bearer <token>"

# Import configuration
curl -X POST http://localhost:8080/api/config/import \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d @config-export.json
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `JWT_SECRET` | `vectoros-secret-key` | Secret key for JWT token signing |
| `VECTOROS_USERNAME` | `admin` | Default admin username |
| `VECTOROS_PASSWORD` | `vectoros` | Default admin password |
| `RUST_LOG` | `info` | Log level (debug, info, warn, error) |

## VPP Startup Configuration

VPP startup commands are defined in `/etc/vectoros/startup.vpp`:

```
! VectorOS VPP Startup Configuration
!
! Interface bindings (examples):
!   create interface rdma host-if enp1s0 name wan0
!   create interface rdma host-if enp2s0 name lan0
!
! Interface configuration:
!   set interface ip address lan0 192.168.1.1/24
!   set interface mtu packet 1500 lan0
!   set interface state lan0 up
!
! NAT configuration:
!   nat44 ei enable
!   nat44 ei add interface inside lan0
!   nat44 ei add interface outside wan0
```

To apply VPP startup commands:
```bash
vppctl exec /etc/vectoros/startup.vpp
```

Or configure VPP to load them automatically by adding to `/etc/vpp/startup.conf`:
```
startup { exec /etc/vectoros/startup.vpp }
```

## Database Configuration

The SQLite database is stored at `/var/lib/vectoros/vectoros.db` by default. Change the path with the `--db` command-line flag:

```bash
vectoros --db /custom/path/vectoros.db
```

## Configuration via Web Interface

The web interface provides a complete configuration management UI accessible at `http://<router-ip>:8080`. From the interface you can:

- View and edit all settings through a dark-themed dashboard
- Use the Configuration page for VyOS-style hierarchical editing
- Import/export configurations as JSON
- Save and apply configuration templates
- View configuration history and perform rollbacks
- Monitor real-time system and network statistics via WebSocket
