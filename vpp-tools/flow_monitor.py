#!/usr/bin/env python3
"""VectorOS Flow Monitor

Collects flow statistics from VPP and manages NetFlow/IPFIX export.

Usage:
    flow_monitor.py status          - Show flow monitoring status
    flow_monitor.py top             - Show top talkers (by bytes/packets)
    flow_monitor.py export-config   - Get current export config
    flow_monitor.py export-set      - Set flow export collector
        --collector-ip <ip> --collector-port <port>
    flow_monitor.py export-enable   - Enable flow export
    flow_monitor.py export-disable  - Disable flow export
    flow_monitor.py classify-setup  - Set up classify-based flow table
    flow_monitor.py flows           - List active flows
"""

import json
import re
import subprocess
import sys
import time
import argparse
from collections import defaultdict

# Persistent state file for flow export config and accumulated stats
STATE_FILE = "/tmp/vpp_flow_monitor_state.json"


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


def load_state():
    """Load persistent state from disk."""
    try:
        with open(STATE_FILE, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {
            "export_enabled": False,
            "collector_ip": "",
            "collector_port": 0,
            "flow_cache": {},
            "flow_history": [],
            "last_update": 0,
        }


def save_state(state):
    """Persist state to disk."""
    try:
        with open(STATE_FILE, "w") as f:
            json.dump(state, f)
    except OSError:
        pass


def get_vpp_flows():
    """Parse VPP flow information from various sources.

    Tries multiple approaches:
    1. 'show ip flows' if available
    2. 'show ip fib' for routing info
    3. NAT session table as proxy for active flows
    4. Classify table sessions if configured
    """
    flows = []
    total_active = 0

    # Attempt 1: Native flow table (may not be available in all VPP builds)
    output, rc = run_vppctl(["show", "ip", "flows"])
    if rc == 0 and output and "Unknown command" not in output:
        flows = parse_native_flows(output)
        if flows:
            return {
                "source": "native",
                "active_count": len(flows),
                "flows": flows,
            }

    # Attempt 2: NAT sessions as a proxy for active flows
    nat_output, _ = run_vppctl(["show", "nat44", "ei", "sessions"])
    nat_flows = parse_nat_sessions(nat_output)
    if nat_flows:
        flows.extend(nat_flows)

    # Attempt 3: NAT44 sessions (alternative format)
    if not flows:
        nat44_output, _ = run_vppctl(["show", "nat44", "sessions"])
        nat44_flows = parse_nat44_sessions(nat44_output)
        flows.extend(nat44_flows)

    # Attempt 4: Classify sessions
    classify_output, _ = run_vppctl(["show", "classify", "sessions"])
    classify_flows = parse_classify_sessions(classify_output)
    flows.extend(classify_flows)

    # Attempt 5: IP neighbor table for flow context
    if not flows:
        neighbor_output, _ = run_vppctl(["show", "ip", "neighbors"])
        neighbor_flows = parse_ip_neighbors(neighbor_output)
        flows.extend(neighbor_flows)

    return {
        "source": "derived" if flows else "none",
        "active_count": len(flows),
        "flows": flows,
    }


def parse_native_flows(output):
    """Parse 'show ip flows' output into flow records."""
    flows = []
    if not output or "Unknown command" in output:
        return flows

    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("IP") or line.startswith("---"):
            continue

        flow = parse_flow_line(line)
        if flow:
            flows.append(flow)

    return flows


def parse_flow_line(line):
    """Parse a single flow line with 5-tuple info.

    Handles formats like:
        192.168.1.1:443 -> 10.0.0.5:12345 TCP pkts 100 bytes 50000
        10.0.0.5:12345 -> 192.168.1.1:443 TCP pkts 200 bytes 100000
    """
    parts = line.split()
    if len(parts) < 3:
        return None

    flow = {
        "src_ip": "",
        "dst_ip": "",
        "src_port": 0,
        "dst_port": 0,
        "protocol": "unknown",
        "packets": 0,
        "bytes": 0,
        "duration_sec": 0,
    }

    # Try to find IP:port patterns
    ip_port_pattern = r"(\d+\.\d+\.\d+\.\d+):(\d+)"
    matches = re.findall(ip_port_pattern, line)

    if len(matches) >= 2:
        flow["src_ip"] = matches[0][0]
        flow["src_port"] = int(matches[0][1])
        flow["dst_ip"] = matches[1][0]
        flow["dst_port"] = int(matches[1][1])
    elif len(matches) == 1:
        flow["src_ip"] = matches[0][0]
        flow["src_port"] = int(matches[0][1])

    # Protocol detection
    for proto in ["TCP", "UDP", "ICMP", "SCTP"]:
        if proto in line.upper():
            flow["protocol"] = proto
            break

    # Extract packet count
    pkt_match = re.search(r"pkts?\s+(\d+)", line, re.IGNORECASE)
    if pkt_match:
        flow["packets"] = int(pkt_match.group(1))

    # Extract byte count
    byte_match = re.search(r"bytes?\s+(\d+)", line, re.IGNORECASE)
    if byte_match:
        flow["bytes"] = int(byte_match.group(1))

    # Extract duration
    dur_match = re.search(r"dur(?:ation)?\s+(\d+)", line, re.IGNORECASE)
    if dur_match:
        flow["duration_sec"] = int(dur_match.group(1))

    if flow["src_ip"] or flow["dst_ip"]:
        return flow

    return None


def parse_nat_sessions(output):
    """Parse 'show nat44 ei sessions' for flow records."""
    flows = []
    if not output:
        return flows

    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("NAT") or line.startswith("---"):
            continue

        flow = parse_flow_line(line)
        if flow:
            flows.append(flow)

    return flows


def parse_nat44_sessions(output):
    """Parse 'show nat44 sessions' for flow records."""
    flows = []
    if not output:
        return flows

    for line in output.split("\n"):
        line = line.strip()
        if not line or "session" in line.lower() and "count" in line.lower():
            continue

        flow = parse_flow_line(line)
        if flow:
            flows.append(flow)

    return flows


def parse_classify_sessions(output):
    """Parse 'show classify sessions' for flow records."""
    flows = []
    if not output:
        return flows

    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("classify") or line.startswith("---"):
            continue

        flow = parse_flow_line(line)
        if flow:
            flows.append(flow)

    return flows


def parse_ip_neighbors(output):
    """Parse IP neighbors table as a basic flow proxy."""
    flows = []
    if not output:
        return flows

    for line in output.split("\n"):
        line = line.strip()
        if not line or line.startswith("IP") or line.startswith("---"):
            continue

        ip_match = re.search(r"(\d+\.\d+\.\d+\.\d+)", line)
        if ip_match:
            flows.append({
                "src_ip": ip_match.group(1),
                "dst_ip": "",
                "src_port": 0,
                "dst_port": 0,
                "protocol": "unknown",
                "packets": 0,
                "bytes": 0,
                "duration_sec": 0,
            })

    return flows


def compute_top_talkers(flows, key, limit=10):
    """Aggregate flows by the given key and return top talkers."""
    aggregated = defaultdict(lambda: {"bytes": 0, "packets": 0, "flow_count": 0})

    for f in flows:
        val = f.get(key, "")
        if not val:
            continue
        aggregated[val]["bytes"] += f.get("bytes", 0)
        aggregated[val]["packets"] += f.get("packets", 0)
        aggregated[val]["flow_count"] += 1

    result = []
    for addr, stats in sorted(aggregated.items(), key=lambda x: x[1]["bytes"], reverse=True):
        result.append({
            "address": addr,
            **stats,
        })

    return result[:limit]


def compute_protocol_distribution(flows):
    """Compute packet distribution by protocol."""
    distribution = defaultdict(lambda: {"bytes": 0, "packets": 0, "flow_count": 0})

    for f in flows:
        proto = f.get("protocol", "unknown")
        distribution[proto]["bytes"] += f.get("bytes", 0)
        distribution[proto]["packets"] += f.get("packets", 0)
        distribution[proto]["flow_count"] += 1

    result = []
    total_bytes = sum(d["bytes"] for d in distribution.values()) or 1

    for proto, stats in sorted(distribution.items(), key=lambda x: x[1]["bytes"], reverse=True):
        result.append({
            "protocol": proto,
            **stats,
            "percentage": round((stats["bytes"] / total_bytes) * 100, 1),
        })

    return result


def cmd_status():
    """Show flow monitoring status."""
    state = load_state()
    flow_data = get_vpp_flows()
    flows = flow_data.get("flows", [])

    # Check if flow export plugin is loaded
    plugin_output, _ = run_vppctl(["show", "plugins"])
    flow_plugins = []
    for line in plugin_output.split("\n"):
        if "flow" in line.lower() or "netflow" in line.lower() or "ipfix" in line.lower():
            flow_plugins.append(line.strip())

    # Try to get flow export status
    export_output, _ = run_vppctl(["show", "flow", "export"])
    export_active = False
    if export_output and "Unknown command" not in export_output:
        export_active = "enabled" in export_output.lower() or "active" in export_output.lower()

    result = {
        "monitoring_active": True,
        "active_flows": flow_data["active_count"],
        "flow_source": flow_data["source"],
        "export_enabled": state.get("export_enabled", False) or export_active,
        "collector_ip": state.get("collector_ip", ""),
        "collector_port": state.get("collector_port", 0),
        "flow_plugins_found": flow_plugins,
        "top_sources": compute_top_talkers(flows, "src_ip", 5),
        "top_destinations": compute_top_talkers(flows, "dst_ip", 5),
        "protocol_distribution": compute_protocol_distribution(flows),
    }

    print(json.dumps(result, indent=2))


def cmd_top():
    """Show top talkers by bytes and packets."""
    state = load_state()
    flow_data = get_vpp_flows()
    flows = flow_data.get("flows", [])

    # Merge with cached flows if available
    cached = state.get("flow_cache", {})
    if cached:
        for key, val in cached.items():
            exists = any(
                f.get("src_ip") == val.get("src_ip") and f.get("dst_ip") == val.get("dst_ip")
                for f in flows
            )
            if not exists:
                flows.append(val)

    top_by_bytes_src = compute_top_talkers(flows, "src_ip", 10)
    top_by_bytes_dst = compute_top_talkers(flows, "dst_ip", 10)
    top_by_packets_src = sorted(
        compute_top_talkers(flows, "src_ip", 20),
        key=lambda x: x["packets"],
        reverse=True,
    )[:10]

    result = {
        "active_flows": len(flows),
        "flow_source": flow_data["source"],
        "top_sources_by_bytes": top_by_bytes_src,
        "top_destinations_by_bytes": top_by_bytes_dst,
        "top_sources_by_packets": top_by_packets_src,
        "protocol_distribution": compute_protocol_distribution(flows),
    }

    print(json.dumps(result, indent=2))


def cmd_export_config():
    """Get current flow export configuration."""
    state = load_state()

    # Try to read from VPP directly
    export_output, _ = run_vppctl(["show", "flow", "export"])
    vpp_config = {}
    if export_output and "Unknown command" not in export_output:
        for line in export_output.split("\n"):
            line = line.strip()
            if "collector" in line.lower():
                parts = line.split()
                if len(parts) >= 2:
                    vpp_config["collector_info"] = line

    result = {
        "export_enabled": state.get("export_enabled", False),
        "collector_ip": state.get("collector_ip", ""),
        "collector_port": state.get("collector_port", 0),
        "vpp_export_output": export_output if "Unknown command" not in (export_output or "") else "",
        "vpp_config": vpp_config,
    }

    print(json.dumps(result, indent=2))


def cmd_export_set(args):
    """Set flow export collector IP and port."""
    state = load_state()
    collector_ip = args.collector_ip
    collector_port = args.collector_port

    if not collector_ip:
        print(json.dumps({"error": "collector-ip is required"}))
        sys.exit(1)
    if not collector_port:
        print(json.dumps({"error": "collector-port is required"}))
        sys.exit(1)

    state["collector_ip"] = collector_ip
    state["collector_port"] = collector_port
    save_state(state)

    # Configure VPP flow export
    # Try multiple VPP command syntaxes for compatibility
    results = []

    # Method 1: set flow export collector
    out1, rc1 = run_vppctl(["set", "flow", "export", "collector",
                             collector_ip, str(collector_port)])
    results.append({"cmd": "set flow export collector", "rc": rc1, "output": out1})

    # Method 2: ipfix collector command
    out2, rc2 = run_vppctl(["ipfix", "collector", "ip4", collector_ip,
                             "transport", "udp", "port", str(collector_port)])
    results.append({"cmd": "ipfix collector", "rc": rc2, "output": out2})

    # Method 3: flow export plugin config
    out3, rc3 = run_vppctl(["flow", "export", "set", "collector",
                             collector_ip, str(collector_port)])
    results.append({"cmd": "flow export set collector", "rc": rc3, "output": out3})

    # Determine success - at least one command should work
    any_success = any(r["rc"] == 0 and "Unknown" not in r["output"] for r in results)

    print(json.dumps({
        "status": "ok" if any_success else "warning",
        "message": f"Collector set to {collector_ip}:{collector_port}",
        "collector_ip": collector_ip,
        "collector_port": collector_port,
        "vpp_results": results,
        "note": "Some VPP builds may not support all flow export commands. "
                "Check that the flow export plugin is loaded with 'show plugins'.",
    }, indent=2))


def cmd_export_enable():
    """Enable flow export."""
    state = load_state()

    if not state.get("collector_ip"):
        print(json.dumps({
            "error": "No collector configured. Run export-set first.",
        }))
        sys.exit(1)

    results = []

    # Try enabling via various command syntaxes
    out1, rc1 = run_vppctl(["set", "flow", "export", "enable"])
    results.append({"cmd": "set flow export enable", "rc": rc1, "output": out1})

    out2, rc2 = run_vppctl(["flow", "export", "enable"])
    results.append({"cmd": "flow export enable", "rc": rc2, "output": out2})

    any_success = any(r["rc"] == 0 and "Unknown" not in r["output"] for r in results)

    state["export_enabled"] = any_success
    save_state(state)

    print(json.dumps({
        "status": "ok" if any_success else "warning",
        "message": "Flow export enabled" if any_success else "Flow export enable command sent (check VPP plugin support)",
        "export_enabled": True,
        "collector": f"{state['collector_ip']}:{state['collector_port']}",
        "vpp_results": results,
    }, indent=2))


def cmd_export_disable():
    """Disable flow export."""
    results = []

    out1, rc1 = run_vppctl(["set", "flow", "export", "disable"])
    results.append({"cmd": "set flow export disable", "rc": rc1, "output": out1})

    out2, rc2 = run_vppctl(["flow", "export", "disable"])
    results.append({"cmd": "flow export disable", "rc": rc2, "output": out2})

    state = load_state()
    state["export_enabled"] = False
    save_state(state)

    print(json.dumps({
        "status": "ok",
        "message": "Flow export disabled",
        "export_enabled": False,
        "vpp_results": results,
    }, indent=2))


def cmd_classify_setup():
    """Set up a classify-based flow table using VPP classify feature.

    Creates a classify table and session to track flows by 5-tuple.
    This is a fallback when native flow export is not available.
    """
    results = []

    # Step 1: Create classify table
    out1, rc1 = run_vppctl([
        "classify", "table",
        "l3", "ip6",
        "permit",
    ])
    results.append({"cmd": "classify table", "rc": rc1, "output": out1})

    # Step 2: Add a "next-classify" session to count all traffic
    out2, rc2 = run_vppctl([
        "classify", "session",
        "table-index", "0",
        "permit",
    ])
    results.append({"cmd": "classify session", "rc": rc2, "output": out2})

    # Step 3: Set up ACL-based classify for IP flows
    out3, rc3 = run_vppctl(["acl", "plugin", "enable"])
    results.append({"cmd": "acl plugin enable", "rc": rc3, "output": out3})

    # Step 4: Try to create a per-flow classify rule
    out4, rc4 = run_vppctl([
        "classify", "table",
        "l3", "ip",
        "permit",
    ])
    results.append({"cmd": "classify table ip", "rc": rc4, "output": out4})

    any_success = any(r["rc"] == 0 and "Unknown" not in r["output"] for r in results)

    # Show the classify tables
    show_out, _ = run_vppctl(["show", "classify", "table"])

    print(json.dumps({
        "status": "ok" if any_success else "warning",
        "message": "Classify table setup attempted",
        "classify_tables": show_out,
        "vpp_results": results,
    }, indent=2))


def cmd_flows():
    """List active flows."""
    flow_data = get_vpp_flows()

    result = {
        "active_flows": flow_data["active_count"],
        "flow_source": flow_data["source"],
        "flows": flow_data["flows"][:100],  # Limit output
    }

    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description="VectorOS Flow Monitor")
    subparsers = parser.add_subparsers(dest="action", help="Action to perform")

    subparsers.add_parser("status", help="Show flow monitoring status")
    subparsers.add_parser("top", help="Show top talkers")
    subparsers.add_parser("export-config", help="Get export configuration")
    subparsers.add_parser("export-enable", help="Enable flow export")
    subparsers.add_parser("export-disable", help="Disable flow export")
    subparsers.add_parser("classify-setup", help="Set up classify-based flow table")
    subparsers.add_parser("flows", help="List active flows")

    export_set_parser = subparsers.add_parser("export-set", help="Set export collector")
    export_set_parser.add_argument("--collector-ip", required=True, help="Collector IP address")
    export_set_parser.add_argument("--collector-port", required=True, type=int, help="Collector port")

    args = parser.parse_args()

    if not args.action:
        parser.print_help()
        sys.exit(0)

    actions = {
        "status": cmd_status,
        "top": cmd_top,
        "export-config": cmd_export_config,
        "export-set": cmd_export_set,
        "export-enable": cmd_export_enable,
        "export-disable": cmd_export_disable,
        "classify-setup": cmd_classify_setup,
        "flows": cmd_flows,
    }

    action_fn = actions.get(args.action)
    if action_fn:
        if args.action == "export-set":
            action_fn(args)
        else:
            action_fn()
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
