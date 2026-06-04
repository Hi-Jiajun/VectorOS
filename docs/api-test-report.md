# VectorOS API Test Report

**Date:** 2026-06-05
**VM:** 192.168.1.7 (VectorOS v0.1.0, VPP v26.06-rc0)
**Test Method:** SSH + curl to localhost:8080

## Summary

| # | Endpoint | HTTP Code | Valid JSON | Status |
|---|----------|-----------|------------|--------|
| 1 | GET /api/health | 200 | Yes | PASS |
| 2 | GET /api/system | 200 | Yes | PASS |
| 3 | GET /api/pppoe/status | 200 | Yes | PASS |
| 4 | GET /api/interfaces | 200 | Yes | PASS |
| 5 | GET /api/nat/status | 200 | Yes | PASS |
| 6 | GET /api/dhcp/status | 200 | Yes | PASS |
| 7 | GET /api/dns/status | 200 | Yes | PASS |
| 8 | GET /api/vpn/status | 200 | Yes | PASS |
| 9 | GET /api/conntrack/status | 200 | Yes | PASS (after fix) |
| 10 | GET /api/monitor/metrics | 200 | Yes | PASS |
| 11 | GET /api/services | 200 | Yes | PASS |
| 12 | GET /api/config/tree | 200 | Yes | PASS |
| 13 | GET /api/diag/status | 200 | Yes | PASS |
| 14 | GET /api/firewall/status | 200 | Yes | PASS (after fix) |

## Authentication

- **POST /api/auth/login** returns JWT token and CSRF token
- All endpoints except `/api/health` require `Authorization: Bearer <token>` header
- Unauthenticated requests return HTTP 401

## Endpoint Response Formats

### 1. GET /api/health
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### 2. GET /api/system
```json
{
  "performance": {
    "errors": { "counters": [...], "total_drops": 3210820, "total_errors": 0 },
    "interfaces": [{ "name": "lan0", "rx_bps": 65546, "tx_bytes": 2278702, ... }],
    "memory": { "free": 190, "percent": 62.7, "total": 512, "used": 321 },
    "nat": { "session_count": 0, "session_rate": 0.0 },
    "packet_rate": { "rx_bytes_per_sec": 133921, "tx_bytes_per_sec": 0 },
    "pppoe": { "sessions_active": 0, "sessions_discovery": 1, "total_clients": 1 },
    "threads": { "worker_threads": 0 }
  },
  "system": {
    "cpu": { "count": 6, "percent": 45.9 },
    "disk": { "percent": 40.1, "total": 41602887680, "used": 16697421824 },
    "memory": { "percent": 82.3, "total": 3491876864, "used": 2875461632 }
  },
  "vpp": {
    "interface_count": 6,
    "version": "vpp v26.06-rc0~703-g33c6e2e36 ..."
  }
}
```

### 3. GET /api/pppoe/status
```json
{
  "status": "ok",
  "clients": [{ "client_state": 0, "sw_if_index": 1, "mru": 1492, "mtu": 1492, ... }],
  "interfaces": [{ "name": "lan0", "state": "up", "mtu": 9000, "sw_if_index": 2 }]
}
```

### 4. GET /api/interfaces
```json
{
  "interfaces": [
    { "name": "lan0", "state": "up", "mtu": 9000, "sw_if_index": 2, "interface_type": "bridge", "ip_addresses": [], "mac_address": "" },
    { "name": "pppoe-wan0", "state": "up", "mtu": 1492, "sw_if_index": 4, "interface_type": "pppoe" }
  ]
}
```

### 5. GET /api/nat/status
```json
{
  "enabled": true,
  "interfaces": [
    { "direction": "in", "name": "lan0" },
    { "direction": "out", "name": "pppoe-wan0" }
  ],
  "session_count": 0
}
```

### 6. GET /api/dhcp/status
```json
{
  "leases": [],
  "status": "active"
}
```

### 7. GET /api/dns/status
```json
{
  "cache_size": 1000,
  "interface": "lan0",
  "status": "active",
  "upstream": ["8.8.8.8", "1.1.1.1"],
  "upstream_v6": ["2001:4860:4860::8888", "2606:4700:4700::1111"]
}
```

### 8. GET /api/vpn/status
```json
{
  "status": "ok",
  "backends": { "ipsec_kernel": true, "wireguard_vpp": true, "openvpn": false },
  "ipsec": { "count": 0, "security_associations": [] },
  "openvpn": { "count": 0, "connections": [] },
  "wireguard": { "count": 0, "tunnels": [] }
}
```

### 9. GET /api/conntrack/status
```json
{
  "tracking_active": true,
  "data_source": "none",
  "stats": {
    "total_connections": 0,
    "new_connections": 0,
    "established_connections": 0,
    "protocol_distribution": { "tcp": 0, "udp": 0, "icmp": 0, "other": 0 }
  },
  "nat_interfaces": [{ "direction": "in", "name": "lan0" }],
  "nat_summary": "NAT44 pool addresses:",
  "arp_neighbor_count": 13
}
```

### 10. GET /api/monitor/metrics
```json
{
  "status": "ok",
  "health_score": 15,
  "metrics": {
    "cpu_cores": [{ "core": 0, "percent": 100.0 }],
    "cpu_count": 6,
    "cpu_percent": 100.0,
    "disk_usage": [{ "device": "/dev/sda2", "percent": 42.0 }],
    "memory": { "percent": 82.2, "total": 3491876864 },
    "network": [{ "name": "ens18", "rx_bytes": 1084482055, "state": "up" }],
    "processes": [{ "name": "dnsmasq", "running": true }],
    "vpp": { "available": true, "pppoe_active": 0, "nat_sessions": 0 }
  }
}
```

### 11. GET /api/services
```json
{
  "status": "ok",
  "count": 6,
  "services": [
    { "name": "pppoe", "display_name": "PPPoE Client", "state": "running" },
    { "name": "nat", "display_name": "NAT", "state": "running" },
    { "name": "firewall", "display_name": "Firewall", "state": "running" },
    { "name": "vpn", "display_name": "VPN", "state": "running" },
    { "name": "dhcp", "display_name": "DHCP Server", "state": "running" },
    { "name": "dns", "display_name": "DNS Forwarder", "state": "running" }
  ]
}
```

### 12. GET /api/config/tree
```json
{
  "status": "ok",
  "tree": {
    "dhcp": { "enabled": false, "start_ip": "192.168.1.100", "end_ip": "192.168.1.200" },
    "dns": { "enabled": false, "upstream": ["8.8.8.8", "1.1.1.1"] },
    "firewall": { "enabled": false, "rules": [] },
    "interfaces": { "eth0": { ... }, "eth1": { "address": ["192.168.1.1/24"] } },
    "nat": { "enabled": false, "inside_if": "eth1", "outside_if": "eth0" },
    "pppoe": { "enabled": false, "interface": "eth0" }
  }
}
```

### 13. GET /api/diag/status
```json
{
  "status": "ok",
  "timestamp": "2026-06-04T16:27:00",
  "tools": { "dns_lookup": true, "ping": true, "port_scan": true, "traceroute": false }
}
```

### 14. GET /api/firewall/status
```json
{
  "status": "ok",
  "enabled": true,
  "default_policy": "block",
  "active_rules": 0,
  "total_rules": 0,
  "rules": [],
  "aliases": [],
  "groups": [],
  "schedules": [],
  "geoip": { "enabled": false },
  "ids": { "enabled": false },
  "shaper": { "enabled": false },
  "vpp_acl_status": ""
}
```

## Issues Found and Fixed

### 1. Conntrack hardcoded Python script path (CRITICAL)
- **File:** `control-plane/src/services/conntrack.rs:11`
- **Problem:** Hardcoded path `/home/hiliang/Github/vectoros/vpp-tools/conntrack_manager.py` does not exist on the VM. The VM has the file at `/root/VectorOS/vpp-tools/conntrack_manager.py`.
- **Result:** All conntrack endpoints (`/api/conntrack/status`, `/connections`, `/stats`, `/top`, `/filter`, `/detail`) returned error JSON with HTTP 200.
- **Fix:** Changed path to `/root/VectorOS/vpp-tools/conntrack_manager.py`.

### 2. Flow monitor hardcoded Python script path (CRITICAL)
- **File:** `control-plane/src/services/flow.rs:12`
- **Problem:** Same hardcoded path issue as conntrack.
- **Fix:** Changed path to `/root/VectorOS/vpp-tools/flow_monitor.py`.

### 3. Firewall VPP ACL command incorrect (MODERATE)
- **File:** `control-plane/src/services/firewall.rs:895`
- **Problem:** `vppctl show acl` is not a valid VPP command. The correct command is `show acl-plugin acl`.
- **Result:** `vpp_acl_status` field contained error string `"show acl-plugin: unknown input '"'"''"`.
- **Fix:** Changed to `["show", "acl-plugin", "al"]`.

### 4. Conntrack VPP NAT44 EI summary command unavailable (MINOR)
- **File:** `vpp-tools/conntrack_manager.py:311` and `control-plane/src/services/conntrack.rs:211`
- **Problem:** `show nat44 ei summary` does not exist in VPP 26.06-rc0. Available commands include `addresses`, `sessions`, `interfaces`.
- **Result:** `nat_summary` field contained error string `"show nat44 ei: unknown input 'summary'"`.
- **Fix:** Changed to `show nat44 ei addresses`.

### 5. Build errors fixed (CRITICAL)
- **File:** `control-plane/src/security/csrf.rs:64` - Made `generate_random_token()` public (was private but referenced from `session.rs`).
- **File:** `control-plane/src/api/handlers.rs:224-251` - Fixed borrow-after-move errors in `iface_up` and `iface_down` by cloning the name before moving into closures.
- **File:** `control-plane/src/api/websocket.rs:208` - Added missing `Ok(Err(_))` arm in match on `system_info_handle.await`.
- **File:** `control-plane/src/api/handlers.rs:1-12` - Added `header` import from `axum::http`.

## Notes

- Authentication was added to the new binary build. All endpoints except `/api/health` now require JWT auth.
- The `POST /api/auth/login` endpoint accepts `{"username": "admin", "password": "vectoros"}` (default credentials).
- All 14 tested endpoints return valid JSON with HTTP 200 after fixes were applied.
- The VPP data plane on the VM is running v26.06-rc0 with 6 interfaces (lan0, lan1, local0, pppoe-wan0, tap0, wan0).
