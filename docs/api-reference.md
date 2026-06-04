# VectorOS API Reference

VectorOS exposes a REST API for router management. The API is served by the control plane on port 8080 (configurable).

**Base URL:** `http://<router-ip>:8080`

**OpenAPI documentation:** `GET /api-docs/openapi.json`

## Authentication

All API endpoints require a JWT Bearer token except `/api/health` and `/api/auth/login`.

### Login

```
POST /api/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "vectoros"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "token": "eyJhbGciOiJIUzI1NiJ9...",
    "expires_in": 86400
  },
  "timestamp": "2026-06-04T12:00:00Z"
}
```

**Use the token in subsequent requests:**
```
Authorization: Bearer <token>
```

## Response Format

All responses follow a standard envelope:

**Success:**
```json
{
  "success": true,
  "data": { ... },
  "timestamp": "2026-06-04T12:00:00Z"
}
```

**Error:**
```json
{
  "success": false,
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error description"
  },
  "timestamp": "2026-06-04T12:00:00Z"
}
```

## System Endpoints

### Health Check

```
GET /api/health
```

No authentication required. Returns system health status.

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### System Status

```
GET /api/system
```

Returns CPU, memory, disk usage, VPP version, and interface count.

**Response:**
```json
{
  "system": {
    "cpu": { "percent": 12.5, "count": 4 },
    "memory": { "total": 8589934592, "used": 2147483648, "percent": 25.0 },
    "disk": { "total": 107374182400, "used": 21474836480, "percent": 20.0 }
  },
  "vpp": {
    "version": "26.06-rc0",
    "interface_count": 3
  }
}
```

### VPP Performance Metrics

```
GET /api/system/vpp-performance
```

Returns packet rates, per-interface throughput, NAT sessions, PPPoE stats, memory, threads, and error counters.

## Interface Endpoints

### List Interfaces

```
GET /api/interfaces
```

**Response:**
```json
{
  "interfaces": [
    {
      "name": "wan0",
      "sw_if_index": 1,
      "state": "up",
      "mtu": 1500,
      "mac_address": "aa:bb:cc:dd:ee:ff",
      "ip_addresses": ["10.0.0.1/24"],
      "interface_type": "physical"
    }
  ]
}
```

### Bring Interface Up/Down

```
POST /api/interfaces/{name}/up
POST /api/interfaces/{name}/down
```

### Configure Interface

```
POST /api/interfaces/{name}/config
Content-Type: application/json

{
  "mtu": 1500,
  "ip_add": "192.168.1.1/24",
  "ip_remove": "192.168.1.0/24",
  "promiscuous": false
}
```

### Interface Statistics

```
GET /api/interfaces/{name}/stats
```

**Response:**
```json
{
  "stats": {
    "name": "wan0",
    "rx_packets": 1234567,
    "tx_packets": 987654,
    "rx_bytes": 1234567890,
    "tx_bytes": 987654321,
    "rx_errors": 0,
    "tx_errors": 0,
    "rx_drops": 0,
    "tx_drops": 0
  }
}
```

### VF Interface Binding

Bind a physical VF interface to VPP:

```
POST /api/interfaces/bind
Content-Type: application/json

{
  "vf_name": "enp1s0",
  "vpp_name": "wan0",
  "method": "rdma",
  "pci": "0000:01:00.0"
}
```

**Methods:**
- `rdma` -- RDMA host-interface binding (default, no driver change)
- `dpdk` -- DPDK driver binding (requires vfio-pci)

Unbind an interface:

```
POST /api/interfaces/unbind
Content-Type: application/json

{
  "vpp_name": "wan0"
}
```

List bound interfaces:

```
GET /api/interfaces/bound
```

Configure a bound interface:

```
POST /api/interfaces/{name}/configure-bound
Content-Type: application/json

{
  "ip": "192.168.1.1/24",
  "mtu": 1500
}
```

## PPPoE Endpoints

### List PPPoE Clients

```
GET /api/pppoe/clients
```

### PPPoE Status

```
GET /api/pppoe/status
```

### Create PPPoE Client

```
POST /api/pppoe/create
Content-Type: application/json

{
  "username": "user@isp.com",
  "password": "secret",
  "interface": "enp1s0",
  "mtu": 1492,
  "mru": 1492,
  "use_peer_dns": true,
  "add_default_route4": false,
  "add_default_route6": false
}
```

### PPPoE Auto-Connect

```
POST /api/pppoe/autoconnect/start     -- Start auto-connect
POST /api/pppoe/autoconnect/stop      -- Stop auto-connect
GET  /api/pppoe/autoconnect/status    -- Get auto-connect status
POST /api/pppoe/autoconnect/config    -- Configure auto-connect parameters
```

## NAT Endpoints

### NAT Status

```
GET /api/nat/status
```

**Response:**
```json
{
  "enabled": true,
  "interfaces": [
    { "name": "lan0", "direction": "inside" },
    { "name": "wan0", "direction": "outside" }
  ],
  "session_count": 42
}
```

### Enable NAT

```
POST /api/nat/enable
```

## DHCP Endpoints

### DHCP Status

```
GET /api/dhcp/status
```

### Enable DHCP Server

```
POST /api/dhcp/enable
Content-Type: application/json

{
  "interface": "lan0",
  "start_ip": "192.168.1.100",
  "end_ip": "192.168.1.200",
  "gateway": "192.168.1.1",
  "lease_time": 86400,
  "dns_servers": "8.8.8.8,1.1.1.1"
}
```

## DNS Endpoints

### DNS Status

```
GET /api/dns/status
```

### Enable DNS Resolver

```
POST /api/dns/enable
Content-Type: application/json

{
  "upstream": "8.8.8.8",
  "upstream_v6": "2001:4860:4860::8888",
  "interface": "lan0",
  "cache_size": 1000
}
```

## FRRouting Endpoints

### FRR Status

```
GET /api/frr/status
```

### List Routes

```
GET /api/frr/routes
```

### Add Static Route

```
POST /api/frr/add-route
Content-Type: application/json

{
  "prefix": "10.0.0.0/8",
  "nexthop": "192.168.1.254",
  "interface": "wan0",
  "distance": 1
}
```

### Delete Static Route

```
POST /api/frr/del-route
Content-Type: application/json

{
  "prefix": "10.0.0.0/8",
  "nexthop": "192.168.1.254"
}
```

## Firewall Endpoints

### Firewall Status

```
GET /api/firewall/status
```

### Add Rule

```
POST /api/firewall/add-rule
Content-Type: application/json

{
  "action": "deny",
  "direction": "in",
  "src_ip": "10.0.0.0/8",
  "dst_port": "22",
  "protocol": "tcp",
  "description": "Block SSH from internal",
  "log": true
}
```

### Update Rule

```
POST /api/firewall/update-rule
Content-Type: application/json

{
  "id": 1,
  "action": "permit",
  "enabled": true
}
```

### Delete Rule

```
POST /api/firewall/del-rule
Content-Type: application/json

{ "id": 1 }
```

### Reorder Rules

```
POST /api/firewall/reorder
Content-Type: application/json

{ "rule_ids": [3, 1, 2] }
```

### Enable/Disable Firewall

```
POST /api/firewall/enable
POST /api/firewall/disable
```

### Firewall Groups

```
GET  /api/firewall/groups                         -- List groups
POST /api/firewall/groups/add                     -- Add group
POST /api/firewall/groups/{name}/delete           -- Delete group
POST /api/firewall/groups/{name}/add-rule         -- Add rule to group
POST /api/firewall/groups/{name}/remove-rule      -- Remove rule from group
```

### Firewall Aliases

```
GET  /api/firewall/aliases                        -- List aliases
POST /api/firewall/aliases/add                    -- Add alias
POST /api/firewall/aliases/{name}                 -- Update alias
POST /api/firewall/aliases/{name}/delete          -- Delete alias
POST /api/firewall/aliases/{name}/refresh         -- Refresh URL alias
```

### Firewall Schedules

```
GET  /api/firewall/schedules                      -- List schedules
POST /api/firewall/schedules/add                  -- Add schedule
POST /api/firewall/schedules/{name}/delete        -- Delete schedule
```

### GeoIP Filtering

```
POST /api/firewall/geoip
Content-Type: application/json

{
  "enabled": true,
  "default_action": "permit",
  "blocked_countries": ["CN", "RU"],
  "allowed_countries": [],
  "db_path": "/var/lib/vectoros/geoip.db"
}
```

### Traffic Shaper

```
GET  /api/firewall/shaper/status                  -- Shaper status
POST /api/firewall/shaper/interface               -- Set interface bandwidth
POST /api/firewall/shaper/interface/{name}/delete -- Remove interface shaper
POST /api/firewall/shaper/queue                   -- Add queue
POST /api/firewall/shaper/queue/{name}/delete     -- Delete queue
```

### IDS/IPS (Suricata)

```
POST /api/firewall/ids/config                     -- Update IDS config
GET  /api/firewall/ids/alerts                     -- Get alerts
POST /api/firewall/ids/alerts/clear               -- Clear alerts
GET  /api/firewall/ids/stats                      -- IDS statistics
```

## QoS Endpoints

```
GET  /api/qos/status                              -- QoS status
POST /api/qos/policer                             -- Create policer
DELETE /api/qos/policer/{name}                    -- Delete policer
POST /api/qos/interface/{name}/limit              -- Set interface rate limit
```

## Flow Monitoring Endpoints

```
GET  /api/flows/status                            -- Flow status
GET  /api/flows/top                               -- Top talkers
POST /api/flows/export                            -- Configure export collector
POST /api/flows/export/enable                     -- Enable export
POST /api/flows/export/disable                    -- Disable export
POST /api/flows/classify-setup                    -- Setup flow classification
GET  /api/flows/list                              -- List active flows
```

## Connection Tracking Endpoints

```
GET  /api/conntrack/status                        -- ConnTrack status
GET  /api/conntrack/connections                   -- List connections
GET  /api/conntrack/stats                         -- Statistics
GET  /api/conntrack/top                           -- Top talkers
POST /api/conntrack/filter                        -- Filter connections
GET  /api/conntrack/detail                        -- NAT detail
```

## Traffic Control Endpoints

```
GET    /api/traffic/status                        -- Traffic status
POST   /api/traffic/limit                         -- Set bandwidth limit
DELETE /api/traffic/limit/interface/{iface}       -- Remove interface limit
DELETE /api/traffic/limit/ip/{ip}                 -- Remove IP limit
POST   /api/traffic/priority                      -- Set traffic priority
POST   /api/traffic/app-class                     -- Add app classification
DELETE /api/traffic/app-class/{name}              -- Remove app classification
POST   /api/traffic/defaults                      -- Load default rules
GET    /api/traffic/stats                         -- Traffic statistics
POST   /api/traffic/reset                         -- Reset all rules
```

## VPN Endpoints

```
GET  /api/vpn/status                              -- VPN status
GET  /api/vpn/connections                         -- List connections
POST /api/vpn/wireguard/config                    -- Configure WireGuard
POST /api/vpn/ipsec/config                        -- Configure IPsec
POST /api/vpn/openvpn/config                      -- Configure OpenVPN
POST /api/vpn/down                                -- Bring down tunnel
```

## Diagnostics Endpoints

```
GET  /api/diag/status                             -- Diagnostics status
POST /api/diag/ping                               -- Ping host
POST /api/diag/traceroute                         -- Traceroute to host
POST /api/diag/dns                                -- DNS lookup
POST /api/diag/portscan                           -- Port scan
```

## Configuration Management Endpoints

VyOS-style hierarchical configuration with staging and commit:

```
GET  /api/config/tree                             -- Full config tree
GET  /api/config/staging                          -- Staged (uncommitted) changes
POST /api/config/set                              -- Set value (staged)
POST /api/config/delete                           -- Delete value (staged)
POST /api/config/commit                           -- Commit staged changes
POST /api/config/discard                          -- Discard staged changes
POST /api/config/rollback/{version}               -- Rollback to version
GET  /api/config/diff                             -- Diff committed vs staged
GET  /api/config/diff/{v1}/{v2}                   -- Diff two versions
GET  /api/config/history                          -- Version history
GET  /api/config/templates                        -- List templates
POST /api/config/template/save                    -- Save as template
POST /api/config/template/apply                   -- Apply template
POST /api/config/cli/session                      -- Create CLI session
POST /api/config/cli/execute                      -- Execute CLI command
GET  /api/config/export                           -- Export configuration
POST /api/config/import                           -- Import configuration
POST /api/config/validate                         -- Validate configuration
GET  /api/config/import/history                   -- Import history
```

## Service Management Endpoints

```
GET  /api/services                                -- List all services
GET  /api/services/{name}/status                  -- Get service status
POST /api/services/{name}/start                   -- Start service
POST /api/services/{name}/stop                    -- Stop service
POST /api/services/{name}/restart                 -- Restart service
POST /api/services/{name}/reload                  -- Reload service config
```

## Monitoring Endpoints

```
GET  /api/monitor/metrics                         -- Current metrics
GET  /api/monitor/history                         -- Historical metrics
GET  /api/monitor/alerts                          -- System alerts
POST /api/monitor/alerts/ack                      -- Acknowledge alert
```

## Log Endpoints

```
POST /api/logs                                    -- Query logs
POST /api/logs/clear                              -- Clear all logs
```

## IPv6 Endpoints

```
GET /api/ipv6/status                              -- IPv6 status
GET /api/ipv6/neighbors                           -- NDP table
GET /api/dhcpv6/status                            -- DHCPv6 status
```

## WebSocket

Connect to `ws://<router-ip>:8080/ws` for real-time dashboard updates.

**Message types:**

| Type | Description |
|------|-------------|
| `SystemUpdate` | CPU, memory, disk usage (every 2s) |
| `VppUpdate` | Packet rates, NAT sessions, PPPoE status |
| `InterfaceUpdate` | Interface state and byte counters |
| `AlertUpdate` | System alerts and notifications |
| `Connected` | Connection confirmation |

**Example message:**
```json
{
  "type": "SystemUpdate",
  "cpu_percent": 12.5,
  "cpu_count": 4,
  "memory_total": 8589934592,
  "memory_used": 2147483648,
  "memory_percent": 25.0,
  "disk_total": 107374182400,
  "disk_used": 21474836480,
  "disk_percent": 20.0
}
```
