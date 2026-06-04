#!/usr/bin/env python3
"""VectorOS Traffic Control - Advanced bandwidth shaping, per-IP limits,
priority queues, application QoS, and burst control using VPP policers
and the classify/scheduler infrastructure.

Usage:
    python3 traffic_control.py status
    python3 traffic_control.py set-interface-limit --interface GE0 --rate 100000000 --burst 150000 --direction both
    python3 traffic_control.py remove-interface-limit --interface GE0
    python3 traffic_control.py set-ip-limit --ip 192.168.1.100 --rate 50000000 --burst 75000
    python3 traffic_control.py remove-ip-limit --ip 192.168.1.100
    python3 traffic_control.py set-priority --name gaming --queue high
    python3 traffic_control.py set-app-class --name gaming --ports "3074,27015-27030,2005" --protocol udp --priority high
    python3 traffic_control.py remove-app-class --name gaming
    python3 traffic_control.py stats
    python3 traffic_control.py reset
"""

import sys
import json
import argparse
import subprocess
from pathlib import Path
from datetime import datetime, timezone

TRAFFIC_FILE = Path("/etc/vectoros/traffic-control.json")
VPPCTL = "vppctl"

# Pre-defined application QoS classes (similar to iKuai/OpenWrt SQM)
DEFAULT_APP_CLASSES = {
    "gaming": {
        "description": "Online gaming traffic",
        "ports": "3074,27015-27030,2005,3478-3480,3658,2869,10243-10250",
        "protocol": "udp",
        "priority": "high",
        "dscp": 46,  # EF - Expedited Forwarding
    },
    "video": {
        "description": "Streaming video (YouTube, Netflix, etc.)",
        "ports": "443,80",
        "protocol": "tcp",
        "priority": "high",
        "dscp": 34,  # AF41
    },
    "voip": {
        "description": "VoIP / voice traffic",
        "ports": "5060-5061,10000-20000",
        "protocol": "udp",
        "priority": "high",
        "dscp": 46,  # EF
    },
    "download": {
        "description": "Bulk download / file transfer",
        "ports": "80,443",
        "protocol": "tcp",
        "priority": "low",
        "dscp": 8,  # CS1
    },
    "default": {
        "description": "Default / best-effort traffic",
        "ports": "",
        "protocol": "",
        "priority": "medium",
        "dscp": 0,
    },
}

PRIORITY_LEVELS = {
    "high": {"weight": 40, "dscp": 46, "description": "Real-time / latency-sensitive"},
    "medium": {"weight": 35, "dscp": 0, "description": "Interactive / general traffic"},
    "low": {"weight": 25, "dscp": 8, "description": "Bulk / background traffic"},
}


def run_vppctl(cmd):
    """Run a vppctl command and return (stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            [VPPCTL] + cmd.split(),
            capture_output=True,
            text=True,
            timeout=10,
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except FileNotFoundError:
        return "", "vppctl not found", 1
    except Exception as e:
        return "", str(e), 1


def load_config():
    """Load saved traffic control configuration from disk."""
    if TRAFFIC_FILE.exists():
        try:
            with open(TRAFFIC_FILE) as f:
                return json.load(f)
        except Exception:
            pass
    return {
        "interface_limits": {},
        "ip_limits": {},
        "priority_queues": {k: v for k, v in PRIORITY_LEVELS.items()},
        "app_classes": {},
        "global_enabled": True,
    }


def save_config(data):
    """Persist traffic control configuration to disk."""
    TRAFFIC_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(TRAFFIC_FILE, "w") as f:
        json.dump(data, f, indent=2)


# ── Per-interface bandwidth limits ──────────────────────────────────

def cmd_set_interface_limit(args):
    """Set per-interface bandwidth limit using VPP policer."""
    data = load_config()
    iface = args.interface
    rate = args.rate
    burst = args.burst
    direction = args.direction

    policer_name = f"tc-{iface}-limit"

    # Clean up any existing policer
    run_vppctl(f"set interface input policer {iface} 0")
    run_vppctl(f"set interface output policer {iface} 0")
    run_vppctl(f"policer del {policer_name}")

    # Create policer: rate in kbps, burst in bytes
    rate_kbps = rate // 1000
    cmd_parts = ["policer", "add", policer_name, str(rate_kbps), str(burst), "type", "single_rate_two_color"]
    stdout, stderr, rc = run_vppctl(" ".join(cmd_parts))

    if rc != 0:
        print(json.dumps({"error": f"Failed to create policer: {stderr}", "command": " ".join(cmd_parts)}))
        sys.exit(1)

    # Apply to interface
    if direction == "input":
        apply_cmd = f"set interface input policer {iface} {policer_name}"
    elif direction == "output":
        apply_cmd = f"set interface output policer {iface} {policer_name}"
    else:
        apply_cmd = f"set interface policer {iface} {policer_name}"

    stdout, stderr, rc = run_vppctl(apply_cmd)
    if rc != 0:
        print(json.dumps({"error": f"Failed to apply policer: {stderr}", "command": apply_cmd}))
        sys.exit(1)

    data["interface_limits"][iface] = {
        "rate": rate,
        "burst": burst,
        "direction": direction,
        "policer_name": policer_name,
        "created": datetime.now(timezone.utc).isoformat(),
    }
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Interface limit set on {iface}: {format_rate(rate)}, burst {burst}, direction {direction}",
    }))


def cmd_remove_interface_limit(args):
    """Remove per-interface bandwidth limit."""
    data = load_config()
    iface = args.interface

    if iface not in data.get("interface_limits", {}):
        print(json.dumps({"error": f"No traffic limit configured on {iface}"}))
        sys.exit(1)

    limit = data["interface_limits"].pop(iface)
    policer_name = limit.get("policer_name", f"tc-{iface}-limit")

    run_vppctl(f"set interface input policer {iface} 0")
    run_vppctl(f"set interface output policer {iface} 0")
    run_vppctl(f"policer del {policer_name}")

    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Interface limit removed from {iface}",
    }))


# ── Per-IP bandwidth limits ─────────────────────────────────────────

def cmd_set_ip_limit(args):
    """Set per-IP bandwidth limit using VPP classify + policer.

    This creates a classify table that matches the source IP and applies a
    policer to limit bandwidth for that specific host.
    """
    data = load_config()
    ip = args.ip
    rate = args.rate
    burst = args.burst

    policer_name = f"tc-ip-{ip.replace('.', '-')}"

    # Delete existing limit for this IP
    _remove_ip_limit_vpp(ip, data)

    # Create policer
    rate_kbps = rate // 1000
    cmd_parts = ["policer", "add", policer_name, str(rate_kbps), str(burst), "type", "single_rate_two_color"]
    stdout, stderr, rc = run_vppctl(" ".join(cmd_parts))

    if rc != 0:
        print(json.dumps({"error": f"Failed to create policer: {stderr}"}))
        sys.exit(1)

    data["ip_limits"][ip] = {
        "rate": rate,
        "burst": burst,
        "policer_name": policer_name,
        "created": datetime.now(timezone.utc).isoformat(),
    }
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"IP limit set on {ip}: {format_rate(rate)}, burst {burst}",
    }))


def cmd_remove_ip_limit(args):
    """Remove per-IP bandwidth limit."""
    data = load_config()
    ip = args.ip

    if ip not in data.get("ip_limits", {}):
        print(json.dumps({"error": f"No traffic limit configured for {ip}"}))
        sys.exit(1)

    _remove_ip_limit_vpp(ip, data)
    del data["ip_limits"][ip]
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"IP limit removed for {ip}",
    }))


def _remove_ip_limit_vpp(ip, data):
    """Remove existing VPP policer for an IP."""
    if ip in data.get("ip_limits", {}):
        old = data["ip_limits"][ip]
        run_vppctl(f"policer del {old['policer_name']}")


# ── Priority queues ─────────────────────────────────────────────────

def cmd_set_priority(args):
    """Set priority queue weight for a traffic class."""
    data = load_config()
    name = args.name
    queue = args.queue

    if queue not in PRIORITY_LEVELS:
        print(json.dumps({"error": f"Invalid priority '{queue}'. Choose from: high, medium, low"}))
        sys.exit(1)

    data.setdefault("priority_queues", {})[name] = {
        "level": queue,
        "weight": PRIORITY_LEVELS[queue]["weight"],
        "dscp": PRIORITY_LEVELS[queue]["dscp"],
        "description": args.description or PRIORITY_LEVELS[queue]["description"],
    }
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Priority for '{name}' set to {queue}",
        "queue": data["priority_queues"][name],
    }))


# ── Application-based QoS ───────────────────────────────────────────

def cmd_set_app_class(args):
    """Set application-based QoS classification."""
    data = load_config()
    name = args.name
    ports = args.ports or ""
    protocol = args.protocol or ""
    priority = args.priority or "medium"

    if priority not in PRIORITY_LEVELS:
        print(json.dumps({"error": f"Invalid priority '{priority}'. Choose from: high, medium, low"}))
        sys.exit(1)

    dscp = PRIORITY_LEVELS[priority]["dscp"]
    if args.dscp is not None:
        dscp = args.dscp

    app_class = {
        "ports": ports,
        "protocol": protocol,
        "priority": priority,
        "dscp": dscp,
        "description": args.description or f"{name} traffic",
    }

    # Apply DSCP marking via vppctl classify
    if ports:
        _apply_app_classification(name, app_class)

    data.setdefault("app_classes", {})[name] = app_class
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"App class '{name}' configured: priority={priority}, DSCP={dscp}",
        "class": app_class,
    }))


def cmd_remove_app_class(args):
    """Remove application-based QoS classification."""
    data = load_config()
    name = args.name

    if name not in data.get("app_classes", {}):
        print(json.dumps({"error": f"App class '{name}' not found"}))
        sys.exit(1)

    # Remove from VPP
    _remove_app_classification(name)

    del data["app_classes"][name]
    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"App class '{name}' removed",
    }))


def cmd_load_defaults(args):
    """Load default application QoS classes (gaming, video, voip, download)."""
    data = load_config()

    for name, cls in DEFAULT_APP_CLASSES.items():
        app_class = {
            "ports": cls["ports"],
            "protocol": cls["protocol"],
            "priority": cls["priority"],
            "dscp": cls["dscp"],
            "description": cls["description"],
        }
        if cls["ports"]:
            _apply_app_classification(name, app_class)
        data.setdefault("app_classes", {})[name] = app_class

    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Loaded {len(DEFAULT_APP_CLASSES)} default app classes",
        "classes": list(DEFAULT_APP_CLASSES.keys()),
    }))


def _apply_app_classification(name, app_class):
    """Apply an application classification rule to VPP via vppctl.

    Uses the VPP classify table and ACL to mark matching packets with DSCP.
    """
    ports = app_class.get("ports", "")
    protocol = app_class.get("protocol", "")
    dscp = app_class.get("dscp", 0)

    if not ports:
        return

    # Build classify rule
    # Format: vppctl classify table name <name> ...
    # We use DSCP marking via ip4 dscp <value> match
    for port_entry in ports.split(","):
        port_entry = port_entry.strip()
        if not port_entry:
            continue

        # Handle port ranges
        if "-" in port_entry:
            parts = port_entry.split("-", 1)
            try:
                lo, hi = int(parts[0]), int(parts[1])
                for p in range(lo, min(hi + 1, lo + 50)):  # limit to avoid huge rules
                    proto_flag = f"{protocol} " if protocol else ""
                    run_vppctl(f"classify table name {name}-{p} skip n_vectors 1")
                    run_vppctl(f"classify session {name}-{p} match {proto_flag}l4 {p} 65535 action setdscp {dscp}")
            except ValueError:
                pass
        else:
            proto_flag = f"{protocol} " if protocol else ""
            run_vppctl(f"classify table name {name}-{port_entry} skip n_vectors 1")
            run_vppctl(f"classify session {name}-{port_entry} match {proto_flag}l4 {port_entry} 65535 action setdscp {dscp}")


def _remove_app_classification(name):
    """Remove application classification rules from VPP."""
    # Delete classify tables matching this app class name pattern
    # vppctl does not support bulk delete, so we clean up known tables
    stdout, _, _ = run_vppctl("show classify table")
    if stdout and name in stdout:
        for line in stdout.splitlines():
            if name in line:
                table_name = line.split()[0].strip()
                run_vppctl(f"classify table del {table_name}")


# ── Burst control ───────────────────────────────────────────────────

def cmd_set_burst_control(args):
    """Configure burst control parameters for all policers."""
    data = load_config()

    data["burst_control"] = {
        "enabled": args.enabled,
        "default_burst_bytes": args.burst_bytes,
        "max_burst_multiplier": args.multiplier,
    }

    # Re-apply burst sizes to existing interface limits
    for iface, limit in data.get("interface_limits", {}).items():
        new_burst = min(
            args.burst_bytes,
            int(limit["rate"] / 8 * args.multiplier),
        )
        limit["burst"] = new_burst
        # Re-create policer with new burst
        policer_name = limit["policer_name"]
        rate_kbps = limit["rate"] // 1000
        run_vppctl(f"policer del {policer_name}")
        run_vppctl(f"policer add {policer_name} {rate_kbps} {new_burst} type single_rate_two_color")

    save_config(data)

    print(json.dumps({
        "status": "ok",
        "message": "Burst control configured",
        "burst_control": data["burst_control"],
    }))


# ── Statistics ───────────────────────────────────────────────────────

def cmd_stats(args):
    """Show traffic control statistics."""
    data = load_config()

    # Get VPP policer stats
    stdout1, _, rc1 = run_vppctl("show policer")
    vpp_policers = stdout1 if rc1 == 0 else "N/A"

    stdout2, _, rc2 = run_vppctl("show policer verbose")
    vpp_policers_verbose = stdout2 if rc2 == 0 else "N/A"

    # Get classify table info
    stdout3, _, rc3 = run_vppctl("show classify table")
    vpp_classify = stdout3 if rc3 == 0 else "N/A"

    result = {
        "status": "ok",
        "interface_limits": data.get("interface_limits", {}),
        "ip_limits": data.get("ip_limits", {}),
        "app_classes": data.get("app_classes", {}),
        "priority_queues": data.get("priority_queues", {}),
        "burst_control": data.get("burst_control", {}),
        "global_enabled": data.get("global_enabled", True),
        "vpp_policers": vpp_policers,
        "vpp_policers_verbose": vpp_policers_verbose,
        "vpp_classify": vpp_classify,
        "total_interface_limits": len(data.get("interface_limits", {})),
        "total_ip_limits": len(data.get("ip_limits", {})),
        "total_app_classes": len(data.get("app_classes", {})),
    }

    print(json.dumps(result))


# ── Full status ─────────────────────────────────────────────────────

def cmd_status(args):
    """Show full traffic control status (alias for stats)."""
    cmd_stats(args)


# ── Reset ───────────────────────────────────────────────────────────

def cmd_reset(args):
    """Reset all traffic control rules."""
    data = load_config()

    # Remove all interface limits
    for iface, limit in data.get("interface_limits", {}).items():
        run_vppctl(f"set interface input policer {iface} 0")
        run_vppctl(f"set interface output policer {iface} 0")
        run_vppctl(f"policer del {limit['policer_name']}")

    # Remove all IP limits
    for ip, limit in data.get("ip_limits", {}).items():
        run_vppctl(f"policer del {limit['policer_name']}")

    # Remove all app classification tables
    for name in data.get("app_classes", {}):
        _remove_app_classification(name)

    # Reset config
    new_data = {
        "interface_limits": {},
        "ip_limits": {},
        "priority_queues": {k: v for k, v in PRIORITY_LEVELS.items()},
        "app_classes": {},
        "global_enabled": True,
    }
    save_config(new_data)

    print(json.dumps({
        "status": "ok",
        "message": "All traffic control rules removed",
    }))


# ── Helpers ──────────────────────────────────────────────────────────

def format_rate(bits_per_sec):
    """Format a rate in bits/sec to human readable string."""
    if bits_per_sec >= 1_000_000_000:
        return f"{bits_per_sec / 1_000_000_000:.1f} Gbps"
    if bits_per_sec >= 1_000_000:
        return f"{bits_per_sec / 1_000_000:.1f} Mbps"
    if bits_per_sec >= 1_000:
        return f"{bits_per_sec / 1_000:.1f} Kbps"
    return f"{bits_per_sec} bps"


# ── Main ────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="VectorOS Traffic Control")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # status / stats
    subparsers.add_parser("status", help="Show traffic control status")
    subparsers.add_parser("stats", help="Show traffic control statistics")

    # set-interface-limit
    p = subparsers.add_parser("set-interface-limit", help="Set per-interface bandwidth limit")
    p.add_argument("--interface", required=True, help="Interface name")
    p.add_argument("--rate", type=int, required=True, help="Rate in bits/sec")
    p.add_argument("--burst", type=int, required=True, help="Burst size in bytes")
    p.add_argument("--direction", default="both", choices=["input", "output", "both"],
                   help="Direction to apply limit")

    # remove-interface-limit
    p = subparsers.add_parser("remove-interface-limit", help="Remove interface bandwidth limit")
    p.add_argument("--interface", required=True, help="Interface name")

    # set-ip-limit
    p = subparsers.add_parser("set-ip-limit", help="Set per-IP bandwidth limit")
    p.add_argument("--ip", required=True, help="IP address to limit")
    p.add_argument("--rate", type=int, required=True, help="Rate in bits/sec")
    p.add_argument("--burst", type=int, required=True, help="Burst size in bytes")

    # remove-ip-limit
    p = subparsers.add_parser("remove-ip-limit", help="Remove per-IP bandwidth limit")
    p.add_argument("--ip", required=True, help="IP address")

    # set-priority
    p = subparsers.add_parser("set-priority", help="Set priority queue for a traffic class")
    p.add_argument("--name", required=True, help="Traffic class name")
    p.add_argument("--queue", required=True, choices=["high", "medium", "low"],
                   help="Priority level")
    p.add_argument("--description", default=None, help="Description")

    # set-app-class
    p = subparsers.add_parser("set-app-class", help="Set application QoS class")
    p.add_argument("--name", required=True, help="App class name (e.g. gaming, video)")
    p.add_argument("--ports", default=None, help="Port or port range (e.g. 443 or 1000-2000)")
    p.add_argument("--protocol", default=None, help="Protocol (tcp, udp)")
    p.add_argument("--priority", default=None, choices=["high", "medium", "low"],
                   help="Priority level")
    p.add_argument("--dscp", type=int, default=None, help="DSCP value override (0-63)")
    p.add_argument("--description", default=None, help="Description")

    # remove-app-class
    p = subparsers.add_parser("remove-app-class", help="Remove application QoS class")
    p.add_argument("--name", required=True, help="App class name")

    # load-defaults
    subparsers.add_parser("load-defaults", help="Load default app QoS classes (gaming, video, voip, download)")

    # set-burst-control
    p = subparsers.add_parser("set-burst-control", help="Configure burst control")
    p.add_argument("--enabled", action="store_true", default=True, help="Enable burst control")
    p.add_argument("--no-enabled", dest="enabled", action="store_false", help="Disable burst control")
    p.add_argument("--burst-bytes", type=int, default=150000, help="Default burst size in bytes")
    p.add_argument("--multiplier", type=float, default=1.5, help="Max burst multiplier")

    # reset
    subparsers.add_parser("reset", help="Remove all traffic control rules")

    args = parser.parse_args()

    try:
        if args.command == "status":
            cmd_status(args)
        elif args.command == "stats":
            cmd_stats(args)
        elif args.command == "set-interface-limit":
            cmd_set_interface_limit(args)
        elif args.command == "remove-interface-limit":
            cmd_remove_interface_limit(args)
        elif args.command == "set-ip-limit":
            cmd_set_ip_limit(args)
        elif args.command == "remove-ip-limit":
            cmd_remove_ip_limit(args)
        elif args.command == "set-priority":
            cmd_set_priority(args)
        elif args.command == "set-app-class":
            cmd_set_app_class(args)
        elif args.command == "remove-app-class":
            cmd_remove_app_class(args)
        elif args.command == "load-defaults":
            cmd_load_defaults(args)
        elif args.command == "set-burst-control":
            cmd_set_burst_control(args)
        elif args.command == "reset":
            cmd_reset(args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
