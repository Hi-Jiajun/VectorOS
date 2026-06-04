#!/usr/bin/env python3
"""VectorOS VPP Performance Metrics Collector

Collects VPP-specific performance metrics via vppctl commands:
- Packet processing rate (packets/second)
- Interface throughput (bytes/second)
- NAT session count and rate
- PPPoE session statistics
- Memory usage (VPP heap)
- Worker thread utilization
- Drop/error counters

Stores previous values in a temp file for rate calculation between calls.
"""

import json
import os
import subprocess
import sys
import time

PREV_FILE = "/tmp/vpp_stats_prev.json"


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


def parse_show_interface(output):
    """Parse 'show interface' output into per-interface stats.

    VPP 'show interface' output looks like:

        Name               Idx    State  MTU (L3/IP4/IP6/MH/V4/V6)
        GigabitEthernet0/0/0  1     up   9000  ...
        ...
        rx                    packets                    bytes
        GigabitEthernet0/0/0   123456                 98765432
        ...

    We return a dict mapping interface name -> {rx_packets, tx_packets, rx_bytes, tx_bytes}.
    """
    interfaces = {}
    if not output:
        return interfaces

    section = None  # "header", "rx", "tx"
    for line in output.split("\n"):
        stripped = line.strip()
        if not stripped:
            section = None
            continue

        # Detect section transitions
        if stripped.startswith("rx") and "packets" in stripped:
            section = "rx"
            continue
        if stripped.startswith("tx") and "packets" in stripped:
            section = "tx"
            continue

        if section in ("rx", "tx"):
            parts = stripped.split()
            if len(parts) >= 2:
                name = parts[0]
                try:
                    pkts = int(parts[1])
                    byt = int(parts[2]) if len(parts) >= 3 else 0
                except ValueError:
                    continue
                if name not in interfaces:
                    interfaces[name] = {
                        "rx_packets": 0,
                        "tx_packets": 0,
                        "rx_bytes": 0,
                        "tx_bytes": 0,
                    }
                if section == "rx":
                    interfaces[name]["rx_packets"] = pkts
                    interfaces[name]["rx_bytes"] = byt
                else:
                    interfaces[name]["tx_packets"] = pkts
                    interfaces[name]["tx_bytes"] = byt

    return interfaces


def parse_nat_sessions(output):
    """Parse 'show nat44 ei sessions' to count active sessions."""
    count = 0
    total_rate = 0.0
    if not output:
        return count, total_rate

    for line in output.split("\n"):
        stripped = line.strip()
        if not stripped:
            continue
        # Lines starting with a number or protocol are session entries
        if stripped and stripped[0].isdigit():
            count += 1
        # Look for "established" or "syn" states
        if "established" in stripped.lower():
            count += 1

    return count, total_rate


def parse_pppoe_sessions(output):
    """Parse 'show pppoe client' output for session stats."""
    result = {
        "total_clients": 0,
        "sessions_active": 0,
        "sessions_discovery": 0,
    }
    if not output:
        return result

    for line in output.split("\n"):
        stripped = line.strip()
        if "sw-if-index" not in stripped:
            continue
        result["total_clients"] += 1
        if "PPPOE_CLIENT_SESSION" in stripped:
            result["sessions_active"] += 1
        elif "PPPOE_CLIENT_DISCOVERY" in stripped:
            result["sessions_discovery"] += 1

    return result


def parse_vpp_memory(output):
    """Parse 'show memory' output for VPP heap usage."""
    result = {
        "used": 0,
        "free": 0,
        "total": 0,
        "percent": 0.0,
    }
    if not output:
        return result

    for line in output.split("\n"):
        stripped = line.strip().lower()
        # Look for lines with "used" and "free" keywords
        if "used:" in stripped or "allocated" in stripped:
            parts = stripped.split()
            for i, p in enumerate(parts):
                if p in ("used:", "allocated:", "used", "allocated"):
                    try:
                        val = int(parts[i + 1])
                        result["used"] = val
                    except (ValueError, IndexError):
                        pass
        if "free:" in stripped or "available" in stripped:
            parts = stripped.split()
            for i, p in enumerate(parts):
                if p in ("free:", "available:", "free", "available"):
                    try:
                        val = int(parts[i + 1])
                        result["free"] = val
                    except (ValueError, IndexError):
                        pass

    result["total"] = result["used"] + result["free"]
    if result["total"] > 0:
        result["percent"] = round((result["used"] / result["total"]) * 100, 2)

    return result


def parse_threads(output):
    """Parse 'show threads' for worker thread info."""
    result = {
        "worker_threads": 0,
        "thread_details": [],
    }
    if not output:
        return result

    for line in output.split("\n"):
        stripped = line.strip()
        if not stripped or stripped.startswith("Name") or stripped.startswith("---"):
            continue
        # Thread lines typically have: name, lcore, pid, type
        if "worker" in stripped.lower() or "vpp" in stripped.lower():
            result["worker_threads"] += 1
            parts = stripped.split()
            if len(parts) >= 2:
                result["thread_details"].append({
                    "name": parts[0],
                    "lcore": parts[1] if len(parts) > 1 else "",
                })

    return result


def parse_errors(output):
    """Parse 'show errors' for drop/error counters."""
    result = {
        "total_drops": 0,
        "total_errors": 0,
        "counters": [],
    }
    if not output:
        return result

    for line in output.split("\n"):
        stripped = line.strip()
        if not stripped or stripped.startswith("Count") or stripped.startswith("---"):
            continue
        # Error lines typically: "error-name   counter_value"
        parts = stripped.split()
        if len(parts) >= 2:
            try:
                val = int(parts[-1])
                name = " ".join(parts[:-1])
                result["counters"].append({"name": name, "count": val})
                if "drop" in name.lower():
                    result["total_drops"] += val
                else:
                    result["total_errors"] += val
            except ValueError:
                continue

    return result


def load_previous():
    """Load previous snapshot from temp file."""
    try:
        with open(PREV_FILE, "r") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def save_previous(data):
    """Save current snapshot to temp file."""
    try:
        with open(PREV_FILE, "w") as f:
            json.dump(data, f)
    except OSError:
        pass


def calculate_rate(current, previous, key):
    """Calculate per-second rate for a given metric key."""
    if previous is None:
        return 0.0
    c = current.get(key, 0)
    p = previous.get(key, 0)
    dt = current.get("_timestamp", 0) - previous.get("_timestamp", 0)
    if dt <= 0:
        return 0.0
    return round((c - p) / dt, 2)


def collect_metrics():
    """Collect all VPP performance metrics."""
    now = time.time()
    prev = load_previous()

    # --- Interface throughput ---
    iface_output, _ = run_vppctl(["show", "interface"])
    iface_raw = parse_show_interface(iface_output)

    # Sum all interfaces
    total_rx_packets = sum(v["rx_packets"] for v in iface_raw.values())
    total_tx_packets = sum(v["tx_packets"] for v in iface_raw.values())
    total_rx_bytes = sum(v["rx_bytes"] for v in iface_raw.values())
    total_tx_bytes = sum(v["tx_bytes"] for v in iface_raw.values())

    # Calculate rates
    prev_ifaces = prev.get("interfaces", {}) if prev else {}
    prev_rx_pkts = prev_ifaces.get("_total_rx_packets", 0)
    prev_tx_pkts = prev_ifaces.get("_total_tx_packets", 0)
    prev_rx_bytes = prev_ifaces.get("_total_rx_bytes", 0)
    prev_tx_bytes = prev_ifaces.get("_total_tx_bytes", 0)
    prev_ts = prev.get("_timestamp", 0) if prev else 0
    dt = now - prev_ts if prev_ts > 0 else 0

    rx_pps = round((total_rx_packets - prev_rx_pkts) / dt, 2) if dt > 0 else 0.0
    tx_pps = round((total_tx_packets - prev_tx_pkts) / dt, 2) if dt > 0 else 0.0
    rx_bps = round((total_rx_bytes - prev_rx_bytes) / dt, 2) if dt > 0 else 0.0
    tx_bps = round((total_tx_bytes - prev_tx_bytes) / dt, 2) if dt > 0 else 0.0

    # Per-interface detail
    iface_details = []
    for name, stats in iface_raw.items():
        prev_s = prev_ifaces.get(name, {}) if prev else {}
        p_rx = prev_s.get("rx_packets", 0)
        p_tx = prev_s.get("tx_packets", 0)
        p_rxb = prev_s.get("rx_bytes", 0)
        p_txb = prev_s.get("tx_bytes", 0)
        iface_details.append({
            "name": name,
            "rx_packets": stats["rx_packets"],
            "tx_packets": stats["tx_packets"],
            "rx_bytes": stats["rx_bytes"],
            "tx_bytes": stats["tx_bytes"],
            "rx_pps": round((stats["rx_packets"] - p_rx) / dt, 2) if dt > 0 else 0.0,
            "tx_pps": round((stats["tx_packets"] - p_tx) / dt, 2) if dt > 0 else 0.0,
            "rx_bps": round((stats["rx_bytes"] - p_rxb) / dt, 2) if dt > 0 else 0.0,
            "tx_bps": round((stats["tx_bytes"] - p_txb) / dt, 2) if dt > 0 else 0.0,
        })

    # --- NAT ---
    nat_output, _ = run_vppctl(["show", "nat44", "ei", "sessions"])
    nat_session_count, _ = parse_nat_sessions(nat_output)
    prev_nat = prev.get("nat", {}) if prev else {}
    prev_nat_sessions = prev_nat.get("session_count", 0)
    nat_session_rate = round((nat_session_count - prev_nat_sessions) / dt, 2) if dt > 0 else 0.0

    # --- PPPoE ---
    pppoe_output, _ = run_vppctl(["show", "pppoe", "client"])
    pppoe_stats = parse_pppoe_sessions(pppoe_output)

    # --- Memory ---
    mem_output, _ = run_vppctl(["show", "memory"])
    mem_stats = parse_vpp_memory(mem_output)

    # --- Threads ---
    thread_output, _ = run_vppctl(["show", "threads"])
    thread_stats = parse_threads(thread_output)

    # --- Errors ---
    error_output, _ = run_vppctl(["show", "errors"])
    error_stats = parse_errors(error_output)

    # --- Packet processing rate ---
    packet_rate = {
        "rx_packets_per_sec": rx_pps,
        "tx_packets_per_sec": tx_pps,
        "rx_bytes_per_sec": rx_bps,
        "tx_bytes_per_sec": tx_bps,
    }

    # Build current snapshot for next rate calculation
    current_snapshot = {
        "_timestamp": now,
        "interfaces": {
            "_total_rx_packets": total_rx_packets,
            "_total_tx_packets": total_tx_packets,
            "_total_rx_bytes": total_rx_bytes,
            "_total_tx_bytes": total_tx_bytes,
        },
        "nat": {"session_count": nat_session_count},
    }
    # Store per-interface totals
    for name, stats in iface_raw.items():
        current_snapshot["interfaces"][name] = stats

    save_previous(current_snapshot)

    return {
        "timestamp": now,
        "packet_rate": packet_rate,
        "interfaces": iface_details,
        "nat": {
            "session_count": nat_session_count,
            "session_rate": nat_session_rate,
        },
        "pppoe": pppoe_stats,
        "memory": mem_stats,
        "threads": thread_stats,
        "errors": error_stats,
    }


def main():
    try:
        metrics = collect_metrics()
        print(json.dumps(metrics, indent=2))
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
