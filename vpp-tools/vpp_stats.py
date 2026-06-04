#!/usr/bin/env python3
"""VectorOS VPP Performance Metrics Collector"""

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
    except Exception:
        return "", 1


def parse_show_interface(output):
    """Parse 'show interface' output into per-interface stats."""
    interfaces = {}
    if not output:
        return interfaces

    current_iface = None
    for line in output.split("\n"):
        stripped = line.strip()
        if not stripped:
            continue

        # Skip header line
        if "Name" in stripped and "Idx" in stripped:
            continue

        # Check if this line starts a new interface
        # Interface lines start with a name (no leading whitespace in original)
        if not line.startswith(" "):
            parts = stripped.split()
            if len(parts) >= 4:
                name = parts[0]
                try:
                    idx = int(parts[1])
                    # This is an interface header line
                    current_iface = name
                    interfaces[current_iface] = {
                        "name": name,
                        "sw_if_index": idx,
                        "state": parts[2],
                        "rx_packets": 0,
                        "tx_packets": 0,
                        "rx_bytes": 0,
                        "tx_bytes": 0,
                        "drops": 0,
                        "errors": 0,
                    }
                except ValueError:
                    pass
            continue

        # Parse counter lines (indented)
        if current_iface and stripped:
            parts = stripped.split()
            if len(parts) >= 2:
                counter_name = parts[0]
                try:
                    value = int(parts[-1])
                    if counter_name == "rx" and len(parts) >= 3:
                        if parts[1] == "packets":
                            interfaces[current_iface]["rx_packets"] = value
                        elif parts[1] == "bytes":
                            interfaces[current_iface]["rx_bytes"] = value
                    elif counter_name == "tx" and len(parts) >= 3:
                        if parts[1] == "packets":
                            interfaces[current_iface]["tx_packets"] = value
                        elif parts[1] == "bytes":
                            interfaces[current_iface]["tx_bytes"] = value
                    elif counter_name == "drops":
                        interfaces[current_iface]["drops"] = value
                    elif counter_name in ("tx-error", "rx-error"):
                        interfaces[current_iface]["errors"] += value
                except ValueError:
                    pass

    return interfaces


def get_nat_sessions():
    """Get NAT session count."""
    output, rc = run_vppctl(["show", "nat44", "ei", "sessions"])
    if rc != 0:
        return 0

    total = 0
    for line in output.split("\n"):
        if "sessions" in line.lower():
            parts = line.split()
            for i, part in enumerate(parts):
                if part == "sessions" and i > 0:
                    try:
                        total += int(parts[i-1])
                    except ValueError:
                        pass
    return total


def get_pppoe_status():
    """Get PPPoE client status."""
    output, rc = run_vppctl(["show", "pppoe", "client"])
    if rc != 0:
        return {"active": 0, "discovery": 0, "total": 0}

    active = 0
    discovery = 0
    total = 0

    for line in output.split("\n"):
        if "sw-if-index" in line:
            total += 1
            if "SESSION" in line:
                active += 1
            elif "DISCOVERY" in line:
                discovery += 1

    return {"active": active, "discovery": discovery, "total": total}


def get_memory():
    """Get VPP memory usage."""
    output, rc = run_vppctl(["show", "memory"])
    if rc != 0:
        return {"total_mb": 0, "used_mb": 0, "free_mb": 0, "percent": 0}

    for line in output.split("\n"):
        if "total:" in line and "used:" in line:
            parts = line.split()
            try:
                total_idx = parts.index("total:") + 1
                used_idx = parts.index("used:") + 1
                free_idx = parts.index("free:") + 1

                total = float(parts[total_idx].rstrip("Mm"))
                used = float(parts[used_idx].rstrip("Mm"))
                free = float(parts[free_idx].rstrip("Mm"))

                return {
                    "total_mb": total,
                    "used_mb": used,
                    "free_mb": free,
                    "percent": (used / total * 100) if total > 0 else 0
                }
            except (ValueError, IndexError):
                pass

    return {"total_mb": 0, "used_mb": 0, "free_mb": 0, "percent": 0}


def get_threads():
    """Get VPP thread info."""
    output, rc = run_vppctl(["show", "threads"])
    if rc != 0:
        return {"count": 0, "threads": []}

    threads = []
    for line in output.split("\n"):
        parts = line.split()
        if len(parts) >= 3:
            try:
                thread_id = int(parts[0])
                name = parts[1]
                threads.append({"id": thread_id, "name": name})
            except ValueError:
                pass

    return {"count": len(threads), "threads": threads}


def get_errors():
    """Get VPP error counters."""
    output, rc = run_vppctl(["show", "errors"])
    if rc != 0:
        return {"total": 0, "counters": []}

    counters = []
    total = 0
    for line in output.split("\n"):
        parts = line.split()
        if len(parts) >= 3:
            try:
                count = int(parts[0])
                if count > 0:
                    counters.append({
                        "count": count,
                        "node": parts[1] if len(parts) > 1 else "",
                        "reason": " ".join(parts[2:]) if len(parts) > 2 else ""
                    })
                    total += count
            except ValueError:
                pass

    return {"total": total, "counters": counters[:20]}  # Top 20


def calculate_rates(current, previous, elapsed):
    """Calculate per-second rates."""
    if not previous or elapsed <= 0:
        return {}

    rates = {}
    for iface_name, curr_stats in current.get("interfaces", {}).items():
        prev_stats = previous.get("interfaces", {}).get(iface_name, {})
        if prev_stats:
            rates[iface_name] = {
                "rx_pps": (curr_stats.get("rx_packets", 0) - prev_stats.get("rx_packets", 0)) / elapsed,
                "tx_pps": (curr_stats.get("tx_packets", 0) - prev_stats.get("tx_packets", 0)) / elapsed,
                "rx_bps": (curr_stats.get("rx_bytes", 0) - prev_stats.get("rx_bytes", 0)) * 8 / elapsed,
                "tx_bps": (curr_stats.get("tx_bytes", 0) - prev_stats.get("tx_bytes", 0)) * 8 / elapsed,
            }

    return rates


def main():
    now = time.time()

    # Load previous snapshot
    previous = None
    if os.path.exists(PREV_FILE):
        try:
            with open(PREV_FILE) as f:
                prev_data = json.load(f)
                previous = prev_data.get("data")
                prev_time = prev_data.get("timestamp", 0)
        except Exception:
            pass

    # Collect current metrics
    iface_output, _ = run_vppctl(["show", "interface"])
    interfaces = parse_show_interface(iface_output)

    nat_sessions = get_nat_sessions()
    pppoe = get_pppoe_status()
    memory = get_memory()
    threads = get_threads()
    errors = get_errors()

    # Calculate rates
    elapsed = now - (prev_time if previous else now)
    rates = calculate_rates({"interfaces": interfaces}, previous, elapsed)

    # Build result
    result = {
        "timestamp": now,
        "packet_rate": {
            "rx_packets_per_sec": sum(r.get("rx_pps", 0) for r in rates.values()),
            "tx_packets_per_sec": sum(r.get("tx_pps", 0) for r in rates.values()),
            "rx_bytes_per_sec": sum(r.get("rx_bps", 0) for r in rates.values()),
            "tx_bytes_per_sec": sum(r.get("tx_bps", 0) for r in rates.values()),
        },
        "interfaces": [
            {
                "name": name,
                "rx_packets": stats.get("rx_packets", 0),
                "tx_packets": stats.get("tx_packets", 0),
                "rx_bytes": stats.get("rx_bytes", 0),
                "tx_bytes": stats.get("tx_bytes", 0),
                "drops": stats.get("drops", 0),
                "errors": stats.get("errors", 0),
                "rx_pps": rates.get(name, {}).get("rx_pps", 0),
                "tx_pps": rates.get(name, {}).get("tx_pps", 0),
                "rx_bps": rates.get(name, {}).get("rx_bps", 0),
                "tx_bps": rates.get(name, {}).get("tx_bps", 0),
            }
            for name, stats in interfaces.items()
        ],
        "nat": {
            "sessions": nat_sessions,
        },
        "pppoe": pppoe,
        "memory": memory,
        "threads": threads,
        "errors": errors,
    }

    # Save current snapshot
    try:
        with open(PREV_FILE, "w") as f:
            json.dump({"timestamp": now, "data": {"interfaces": interfaces}}, f)
    except Exception:
        pass

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
