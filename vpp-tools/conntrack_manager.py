#!/usr/bin/env python3
"""VectorOS Connection Tracking Manager

Monitors and reports on active NAT sessions and connection state
using VPP's NAT44 EI and related show commands.

Usage:
    conntrack_manager.py status        - Connection tracking status overview
    conntrack_manager.py connections   - List active connections (NAT sessions)
    conntrack_manager.py stats         - Connection statistics
    conntrack_manager.py top           - Top talkers (source/destination IPs)
    conntrack_manager.py filter        - Filter connections
        --ip <ip> --port <port> --protocol <tcp|udp|icmp>

VPP commands used:
    show nat44 ei sessions     - NAT44 EI session table
    show nat44 sessions        - NAT44 session table (alternative)
    show ip neighbors          - ARP / neighbor table
    show nat44 ei interfaces   - NAT44 EI interface list
    show nat44 ei summary      - NAT44 EI summary counters
"""

import json
import re
import subprocess
import sys
import argparse
from collections import defaultdict, Counter


def run_vppctl(cmd_args, timeout=10):
    """Run a vppctl command and return (stdout, returncode)."""
    try:
        result = subprocess.run(
            ["vppctl"] + cmd_args,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.stdout.strip(), result.returncode
    except FileNotFoundError:
        return "", 1
    except subprocess.TimeoutExpired:
        return "", 1
    except Exception:
        return "", 1


# ---------------------------------------------------------------------------
# Parsing helpers
# ---------------------------------------------------------------------------

def parse_nat_ei_sessions(output):
    """Parse 'show nat44 ei sessions' output into connection records.

    Typical line formats:
        TCP    inside  192.168.1.100:12345  outside  203.0.113.1:443  ...
        UDP    inside  192.168.1.100:5353   outside  8.8.8.8:53       ...
        ICMP   inside  192.168.1.100        outside  8.8.4.4          ...

    We also handle simpler formats that vppctl may produce:
        TCP 192.168.1.100:12345 -> 203.0.113.1:443
    """
    connections = []
    if not output:
        return connections

    for line in output.split("\n"):
        line = line.strip()
        if not line:
            continue
        # Skip header/summary lines
        low = line.lower()
        if "nat44" in low and ("interface" in low or "summary" in low or "session" in low and "count" in low):
            continue
        if line.startswith("---") or line.startswith("Total"):
            continue

        conn = _parse_connection_line(line)
        if conn:
            connections.append(conn)

    return connections


def parse_nat44_sessions(output):
    """Parse 'show nat44 sessions' (non-EI) output."""
    connections = []
    if not output:
        return connections

    for line in output.split("\n"):
        line = line.strip()
        if not line or "session count" in line.lower() or line.startswith("---"):
            continue

        conn = _parse_connection_line(line)
        if conn:
            connections.append(conn)

    return connections


def _parse_connection_line(line):
    """Parse a single connection/session line into a dict."""
    conn = {
        "protocol": "unknown",
        "src_ip": "",
        "src_port": 0,
        "dst_ip": "",
        "dst_port": 0,
        "state": "established",
        "nat_src_ip": "",
        "nat_src_port": 0,
        "direction": "",
    }

    # Detect protocol
    upper = line.upper()
    for proto in ["TCP", "UDP", "ICMP", "SCTP", "GRE", "ESP", "AH"]:
        if proto in upper:
            conn["protocol"] = proto
            break

    # Detect state for TCP
    for state in ["ESTABLISHED", "SYN-SENT", "SYN-RECEIVED", "FIN-WAIT",
                  "CLOSE-WAIT", "TIME-WAIT", "CLOSED", "LISTEN"]:
        if state in upper:
            conn["state"] = state.lower()
            break

    # Detect direction
    if "inside" in line.lower():
        conn["direction"] = "inside-outside"
    elif "outside" in line.lower():
        conn["direction"] = "outside-inside"

    # Extract IP:port pairs
    ip_port_pattern = r"(\d+\.\d+\.\d+\.\d+):(\d+)"
    ip_only_pattern = r"(\d+\.\d+\.\d+\.\d+)"

    matches_with_port = re.findall(ip_port_pattern, line)

    if len(matches_with_port) >= 2:
        conn["src_ip"] = matches_with_port[0][0]
        conn["src_port"] = int(matches_with_port[0][1])
        conn["dst_ip"] = matches_with_port[1][0]
        conn["dst_port"] = int(matches_with_port[1][1])
    elif len(matches_with_port) == 1:
        conn["src_ip"] = matches_with_port[0][0]
        conn["src_port"] = int(matches_with_port[0][1])
        # Try to find a standalone IP for destination
        all_ips = re.findall(ip_only_pattern, line)
        for ip in all_ips:
            if ip != matches_with_port[0][0]:
                conn["dst_ip"] = ip
                break
    else:
        all_ips = re.findall(ip_only_pattern, line)
        if len(all_ips) >= 2:
            conn["src_ip"] = all_ips[0]
            conn["dst_ip"] = all_ips[1]
        elif len(all_ips) == 1:
            conn["src_ip"] = all_ips[0]

    # NAT translation info (3rd IP:port pair)
    if len(matches_with_port) >= 3:
        conn["nat_src_ip"] = matches_with_port[2][0]
        conn["nat_src_port"] = int(matches_with_port[2][1])

    if conn["src_ip"] or conn["dst_ip"]:
        return conn

    return None


def get_all_connections():
    """Retrieve connections from multiple VPP sources and merge."""
    connections = []

    # Source 1: NAT44 EI sessions
    output, rc = run_vppctl(["show", "nat44", "ei", "sessions"])
    if rc == 0 and output:
        connections.extend(parse_nat_ei_sessions(output))

    # Source 2: NAT44 sessions (alternative)
    if not connections:
        output2, rc2 = run_vppctl(["show", "nat44", "sessions"])
        if rc2 == 0 and output2:
            connections.extend(parse_nat44_sessions(output2))

    return connections


# ---------------------------------------------------------------------------
# Aggregate computations
# ---------------------------------------------------------------------------

def compute_stats(connections):
    """Compute connection statistics."""
    total = len(connections)

    proto_dist = Counter(c["protocol"] for c in connections)
    state_dist = Counter(c["state"] for c in connections)

    # New vs established heuristic: TCP SYN* = new, else established
    new_count = 0
    established_count = 0
    other_count = 0
    for c in connections:
        state = c.get("state", "established")
        if "syn" in state or state == "listen":
            new_count += 1
        elif state == "established":
            established_count += 1
        else:
            other_count += 1

    return {
        "total_connections": total,
        "new_connections": new_count,
        "established_connections": established_count,
        "other_connections": other_count,
        "protocol_distribution": {
            "tcp": proto_dist.get("TCP", 0),
            "udp": proto_dist.get("UDP", 0),
            "icmp": proto_dist.get("ICMP", 0),
            "other": sum(v for k, v in proto_dist.items() if k not in ("TCP", "UDP", "ICMP")),
        },
        "state_distribution": dict(state_dist),
    }


def compute_top_talkers(connections, key="src_ip", limit=10):
    """Aggregate connections by the given IP field and rank by count."""
    counts = Counter(c.get(key, "") for c in connections if c.get(key))
    return [
        {"address": addr, "connection_count": count}
        for addr, count in counts.most_common(limit)
    ]


def filter_connections(connections, ip=None, port=None, protocol=None):
    """Filter connections by IP, port, or protocol."""
    result = connections
    if ip:
        result = [
            c for c in result
            if ip in c.get("src_ip", "") or ip in c.get("dst_ip", "")
               or ip in c.get("nat_src_ip", "")
        ]
    if port is not None:
        port = int(port)
        result = [
            c for c in result
            if c.get("src_port") == port or c.get("dst_port") == port
        ]
    if protocol:
        proto_upper = protocol.upper()
        result = [c for c in result if c.get("protocol") == proto_upper]
    return result


# ---------------------------------------------------------------------------
# VPP neighbor info
# ---------------------------------------------------------------------------

def get_arp_neighbors():
    """Parse 'show ip neighbors' for context."""
    output, rc = run_vppctl(["show", "ip", "neighbors"])
    neighbors = []
    if rc != 0 or not output:
        return neighbors

    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("IP") or line.startswith("---"):
            continue
        parts = line.split()
        if len(parts) >= 2:
            neighbors.append({
                "ip": parts[0],
                "mac": parts[1] if len(parts) > 1 else "",
                "interface": parts[2] if len(parts) > 2 else "",
            })
    return neighbors


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_status():
    """Show connection tracking status overview."""
    connections = get_all_connections()
    stats = compute_stats(connections)

    # Get NAT interface info
    nat_ifaces = []
    output, rc = run_vppctl(["show", "nat44", "ei", "interfaces"])
    if rc == 0 and output:
        for line in output.split("\n"):
            line = line.strip()
            if line and "NAT44 interfaces:" not in line:
                parts = line.split()
                if len(parts) >= 2:
                    nat_ifaces.append({"name": parts[0], "direction": parts[1]})

    # Get NAT summary counters if available
    nat_summary = ""
    output2, rc2 = run_vppctl(["show", "nat44", "ei", "summary"])
    if rc2 == 0 and output2 and "Unknown" not in output2:
        nat_summary = output2

    # Neighbor count for context
    neighbors = get_arp_neighbors()

    result = {
        "tracking_active": True,
        "data_source": "nat44_ei" if connections else "none",
        "stats": stats,
        "nat_interfaces": nat_ifaces,
        "nat_summary": nat_summary,
        "arp_neighbor_count": len(neighbors),
    }

    print(json.dumps(result, indent=2))


def cmd_connections():
    """List active connections."""
    connections = get_all_connections()

    result = {
        "total": len(connections),
        "data_source": "nat44_ei" if connections else "none",
        "connections": connections[:500],  # Cap output
    }

    print(json.dumps(result, indent=2))


def cmd_stats():
    """Show connection statistics."""
    connections = get_all_connections()
    stats = compute_stats(connections)

    # Add per-protocol top ports
    tcp_ports = Counter()
    udp_ports = Counter()
    for c in connections:
        if c["protocol"] == "TCP" and c.get("dst_port"):
            tcp_ports[c["dst_port"]] += 1
        elif c["protocol"] == "UDP" and c.get("dst_port"):
            udp_ports[c["dst_port"]] += 1

    stats["top_tcp_dst_ports"] = [
        {"port": p, "count": cnt} for p, cnt in tcp_ports.most_common(10)
    ]
    stats["top_udp_dst_ports"] = [
        {"port": p, "count": cnt} for p, cnt in udp_ports.most_common(10)
    ]

    print(json.dumps(stats, indent=2))


def cmd_top():
    """Show top talkers by source and destination IPs."""
    connections = get_all_connections()
    stats = compute_stats(connections)

    result = {
        "total_connections": stats["total_connections"],
        "top_sources": compute_top_talkers(connections, "src_ip", 10),
        "top_destinations": compute_top_talkers(connections, "dst_ip", 10),
        "protocol_distribution": stats["protocol_distribution"],
    }

    print(json.dumps(result, indent=2))


def cmd_filter(args):
    """Filter connections by IP, port, or protocol."""
    connections = get_all_connections()

    filtered = filter_connections(
        connections,
        ip=args.ip,
        port=args.port,
        protocol=args.protocol,
    )

    result = {
        "total_before": len(connections),
        "total_after": len(filtered),
        "filter": {
            "ip": args.ip,
            "port": args.port,
            "protocol": args.protocol,
        },
        "connections": filtered[:200],
    }

    print(json.dumps(result, indent=2))


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="VectorOS Connection Tracking Manager")
    subparsers = parser.add_subparsers(dest="action", help="Action to perform")

    subparsers.add_parser("status", help="Connection tracking status")
    subparsers.add_parser("connections", help="List active connections")
    subparsers.add_parser("stats", help="Connection statistics")
    subparsers.add_parser("top", help="Top talkers")

    filter_parser = subparsers.add_parser("filter", help="Filter connections")
    filter_parser.add_argument("--ip", help="Filter by IP address (matches src, dst, or NAT)")
    filter_parser.add_argument("--port", type=int, help="Filter by port number")
    filter_parser.add_argument("--protocol", help="Filter by protocol (tcp, udp, icmp)")

    args = parser.parse_args()

    if not args.action:
        parser.print_help()
        sys.exit(0)

    actions = {
        "status": cmd_status,
        "connections": cmd_connections,
        "stats": cmd_stats,
        "top": cmd_top,
        "filter": lambda: cmd_filter(args),
    }

    action_fn = actions.get(args.action)
    if action_fn:
        action_fn()
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
