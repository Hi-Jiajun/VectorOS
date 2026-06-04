# eBPF Integration Plan for VectorOS

## Overview

This document outlines how eBPF can complement VPP in VectorOS for advanced traffic steering, monitoring, and filtering. The approach is inspired by the Landscape project, which uses eBPF as its primary data plane.

## 1. Current Architecture vs Proposed Hybrid Architecture

### Current Architecture (VPP-only)

```
┌─────────────────────────────────────────┐
│           Frontend (Svelte)             │
├─────────────────────────────────────────┤
│        Control Plane (Rust + Axum)      │
├─────────────────────────────────────────┤
│         VPP Graph Node Pipeline         │
│   ┌─────┐   ┌─────┐   ┌─────┐         │
│   │ Node│──▶│ Node│──▶│ Node│         │
│   └─────┘   └─────┘   └─────┘         │
├─────────────────────────────────────────┤
│              DPDK + NIC                 │
└─────────────────────────────────────────┘
```

### Proposed Hybrid Architecture (VPP + eBPF)

```
┌─────────────────────────────────────────┐
│           Frontend (Svelte)             │
├─────────────────────────────────────────┤
│        Control Plane (Rust + Axum)      │
│    ┌─────────────┐  ┌──────────────┐   │
│    │ VPP Manager │  │ eBPF Manager │   │
│    └─────────────┘  └──────────────┘   │
├─────────────────────────────────────────┤
│           ┌───────────────┐             │
│           │  VPP Graph    │             │
│           │  Node Engine  │             │
│           └───────────────┘             │
│                    ▲                    │
│                    │                    │
│           ┌───────────────┐             │
│           │   eBPF Maps   │ ◀──────────── Shared state between
│           │  (Ring Buffer)│             │   eBPF and VPP
│           └───────────────┘             │
├─────────────────────────────────────────┤
│           ┌───────────────┐             │
│           │  eBPF/XDP     │             │
│           │  Pre-filter   │ ◀──────────── Early packet filtering
│           └───────────────┘             │   at driver level
├─────────────────────────────────────────┤
│              DPDK + NIC                 │
└─────────────────────────────────────────┘
```

## 2. How eBPF Complements VPP

### 2.1 Pre-filtering at XDP Level

eBPF XDP programs run at the driver level before packets enter the kernel network stack or VPP. This provides:

- **Early packet drop**: Drop malicious packets before they consume VPP resources
- **DDoS protection**: Rate limiting at the NIC driver level
- **Load balancing**: Distribute packets across VPP worker threads

```c
// Example: XDP pre-filter for DDoS protection
SEC("xdp")
int xdp_pre_filter(struct xdp_md *ctx) {
    void *data = (void *)(long)ctx->data;
    void *data_end = (void *)(long)ctx->data_end;

    struct ethhdr *eth = data;
    if (eth + 1 > data_end) return XDP_DROP;

    // Rate limit SYN floods
    if (eth->h_proto == htons(ETH_P_IP)) {
        struct iphdr *ip = (void *)(eth + 1);
        if (ip + 1 > data_end) return XDP_DROP;

        if (ip->protocol == IPPROTO_TCP) {
            struct tcphdr *tcp = (void *)(ip + 1);
            if (tcp->syn && !tcp->ack) {
                // Rate limit SYN packets
                if (rate_limit_check(ctx)) return XDP_DROP;
            }
        }
    }

    return XDP_PASS;  // Let VPP handle the packet
}
```

### 2.2 Traffic Monitoring and Statistics

eBPF maps provide real-time traffic statistics without impacting VPP performance:

```c
// Traffic statistics map
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 256);
    __type(key, __u32);
    __type(value, struct traffic_stats);
} traffic_stats SEC(".maps");

struct traffic_stats {
    __u64 packets;
    __u64 bytes;
    __u64 drops;
};
```

### 2.3 Connection Tracking

eBPF can maintain connection state that VPP can query:

```c
// Connection tracking map
struct {
    __uint(type, BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, 65536);
    __type(key, struct conn_key);
    __type(value, struct conn_state);
} conn_track SEC(".maps");
```

## 3. Use Cases

### 3.1 Packet Filtering

| Use Case | VPP Native | eBPF Enhancement |
|----------|------------|------------------|
| ACL (Access Control Lists) | VPP ACL plugin | eBPF for O(1) lookups |
| DDoS Protection | Rate limiting plugin | XDP-level drop (lower latency) |
| GeoIP Blocking | Manual IP ranges | eBPF with IP geolocation maps |
| Protocol Filtering | Punt/redirect | XDP drop before VPP processing |

### 3.2 Traffic Shaping

| Use Case | VPP Native | eBPF Enhancement |
|----------|------------|------------------|
| Bandwidth Limiting | QoS plugin | TC-BPF for fine-grained control |
| Per-flow Rate Limiting | Policer plugin | eBPF hash + token bucket |
| Queue Management | QoS queue | TC-BPF + fq_codel |

### 3.3 Monitoring

| Use Case | VPP Native | eBPF Enhancement |
|----------|------------|------------------|
| Flow Statistics | IP flow cache | eBPF ring buffer (zero-copy) |
| Latency Measurement | Built-in counters | eBPF timestamps at driver |
| Protocol Distribution | Packet trace | eBPF histogram maps |
| Anomaly Detection | Manual analysis | eBPF + userspace ML |

### 3.4 Security

| Use Case | VPP Native | eBPF Enhancement |
|----------|------------|------------------|
| Intrusion Detection | Manual rules | eBPF + Suricata integration |
| DNS Monitoring | DNS plugin | eBPF DNS packet capture |
| TLS Inspection | Punt to proxy | eBPF kprobe for TLS keys |

## 4. Architecture: eBPF Hooks vs VPP Graph Nodes

### 4.1 Where eBPF Runs

```
Packet Journey:

NIC Driver
    │
    ▼
[XDP Hook] ◀── eBPF pre-filter (earliest point)
    │
    ▼
Kernel Network Stack
    │
    ▼
[TC Hook] ◀── eBPF traffic control (before VPP)
    │
    ▼
DPDK Poll Mode Driver
    │
    ▼
VPP Graph Nodes
    │
    ├──▶ [Interface Input Node]
    ├──▶ [IP4/IPv6 Lookup]
    ├──▶ [NAT Node]
    ├──▶ [Firewall Node]
    └──▶ [Interface Output Node]
```

### 4.2 When to Use eBPF vs VPP

**Use eBPF when:**
- Early packet filtering (XDP level)
- Low-latency statistics collection
- Custom protocol parsing
- Kernel-level integration required
- Dynamic program updates without restart

**Use VPP when:**
- High-throughput forwarding (10Gbps+)
- Complex packet transformation
- Protocol state machines (PPPoE, IPsec)
- DPDK-optimized processing
- Mature plugin ecosystem

### 4.3 Shared State via eBPF Maps

eBPF and VPP can share state through eBPF maps:

```rust
// Rust control plane reading eBPF maps
use aya::maps::HashMap;

pub struct EbpfManager {
    program: Xdp,
    stats_map: HashMap<MapData, u32, TrafficStats>,
}

impl EbpfManager {
    pub fn get_traffic_stats(&self) -> TrafficStats {
        self.stats_map.get(&0, 0).unwrap_or_default()
    }

    pub fn update_blocklist(&mut self, ip: u32) -> Result<()> {
        self.blocklist_map.insert(ip, 1, 0)?;
        Ok(())
    }
}
```

## 5. Performance Comparison

### 5.1 Raw Throughput

| Metric | VPP (DPDK) | eBPF (XDP) | Notes |
|--------|------------|------------|-------|
| Max Throughput | 40+ Gbps | 20+ Gbps | VPP wins for pure forwarding |
| Packets Per Second | 200M+ pps | 100M+ pps | Both are line-rate capable |
| Latency (P99) | < 10μs | < 5μs | eBPF has lower overhead |

### 5.2 When eBPF Outperforms VPP

- **Simple packet drops**: XDP can drop at ~24M pps vs VPP's ~18M pps
- **DDoS mitigation**: XDP drops before kernel stack = less CPU usage
- **Monitoring**: eBPF ring buffer is zero-copy, VPP requires polling

### 5.3 Recommended Approach

Use **hybrid** approach:
1. eBPF for **pre-filtering** (drops 50% of unwanted traffic)
2. VPP for **forwarding** (handles remaining 50% at line rate)
3. eBPF for **monitoring** (collects stats without VPP overhead)

## 6. Integration with VectorOS Control Plane

### 6.1 New Service: eBPF Manager

```rust
// control-plane/src/services/ebpf.rs

pub struct EbpfService {
    programs: HashMap<String, LoadedProgram>,
    maps: HashMap<String, Box<dyn Map>>,
}

impl EbpfService {
    pub async fn load_program(&mut self, path: &str) -> Result<()> {
        // Load eBPF program from compiled ELF
    }

    pub async fn unload_program(&mut self, name: &str) -> Result<()> {
        // Detach and free eBPF program
    }

    pub async fn get_stats(&self) -> Result<TrafficStats> {
        // Read from eBPF maps
    }

    pub async fn update_blocklist(&mut self, ips: &[u32]) -> Result<()> {
        // Update blocklist map
    }
}
```

### 6.2 API Endpoints

```
GET  /api/v1/ebpf/programs     - List loaded eBPF programs
POST /api/v1/ebpf/programs     - Load new eBPF program
DEL  /api/v1/ebpf/programs/:id - Unload eBPF program
GET  /api/v1/ebpf/stats        - Get traffic statistics
GET  /api/v1/ebpf/maps         - List eBPF maps
PUT  /api/v1/ebpf/maps/:name   - Update map data
```

### 6.3 Frontend Integration

Add "eBPF" navigation item to the sidebar with:
- Program management (load/unload)
- Real-time traffic statistics
- Map viewer/editor
- Program logs

## 7. Implementation Roadmap

### Phase 1: Foundation (2 weeks)

- [ ] Set up Aya (Rust eBPF framework) dependency
- [ ] Create basic eBPF service stub
- [ ] Implement program loading/unloading
- [ ] Add API endpoints for program management

### Phase 2: Pre-filtering (2 weeks)

- [ ] Implement XDP pre-filter program
- [ ] Add DDoS protection use case
- [ ] Create blocklist management API
- [ ] Frontend page for filter rules

### Phase 3: Monitoring (2 weeks)

- [ ] Implement traffic statistics collection
- [ ] Add flow tracking via eBPF maps
- [ ] Create monitoring dashboard
- [ ] Integrate with existing metrics

### Phase 4: Advanced Features (4 weeks)

- [ ] Traffic shaping with TC-BPF
- [ ] Connection tracking
- [ ] Dynamic program updates
- [ ] Performance optimization

## 8. Dependencies

### Rust Crates

```toml
[dependencies]
aya = "0.12"          # eBPF framework
aya-obj = "0.1"       # eBPF object file parsing
tokio = { version = "1", features = ["full"] }
```

### System Requirements

- Linux kernel 5.10+ (for BPF ring buffer)
- libbpf development headers
- clang/llvm for eBPF compilation

## 9. References

- [Aya Documentation](https://aya-rs.dev/)
- [eBPF.io](https://ebpf.io/)
- [Landscape Project](https://github.com/landscape-av/landscape)
- [VPP Documentation](https://docs.fd.io/vpp/)
- [XDP Tutorial](https://github.com/xdp-project/xdp-tutorial)

## 10. Conclusion

eBPF provides a powerful complement to VPP in VectorOS:

1. **Pre-filtering**: Drop unwanted packets before they reach VPP
2. **Monitoring**: Collect statistics with minimal overhead
3. **Flexibility**: Update programs without restarting VPP
4. **Security**: Kernel-level protection against attacks

The hybrid approach combines VPP's high-throughput forwarding with eBPF's flexibility and low-latency filtering. This gives VectorOS the best of both worlds: VPP's DPDK performance for forwarding and eBPF's kernel integration for security and monitoring.
