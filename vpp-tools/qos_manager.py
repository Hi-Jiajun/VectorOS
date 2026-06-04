#!/usr/bin/env python3
"""VectorOS QoS Manager - Traffic policing, rate limiting and DSCP marking using VPP."""

import sys
import json
import argparse
import subprocess
from pathlib import Path

QOS_FILE = Path("/etc/vectoros/qos-config.json")
VPPCTL = "vppctl"


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
    except Exception as e:
        return "", str(e), 1


def load_qos_config():
    """Load saved QoS configuration from disk."""
    if QOS_FILE.exists():
        try:
            with open(QOS_FILE) as f:
                return json.load(f)
        except Exception:
            pass
    return {
        "policers": {},
        "rate_limits": {},
        "dscp_marks": [],
    }


def save_qos_config(data):
    """Persist QoS configuration to disk."""
    QOS_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(QOS_FILE, "w") as f:
        json.dump(data, f, indent=2)


# ── Policer management ──────────────────────────────────────────────

def cmd_create_policer(args):
    """Create a policer in VPP."""
    data = load_qos_config()

    name = args.name
    rate = args.rate
    burst = args.burst
    policer_type = args.type  # "single_rate_two_color", "single_rate_three_color", "trtcm"

    # Build vppctl command: policer add <name> <rate> <burst> type <type>
    cmd_parts = ["policer", "add", name, str(rate), str(burst), "type", policer_type]

    stdout, stderr, rc = run_vppctl(" ".join(cmd_parts))

    if rc != 0:
        print(json.dumps({
            "error": f"Failed to create policer: {stderr}",
            "command": " ".join(cmd_parts),
        }))
        sys.exit(1)

    # Store in config
    data["policers"][name] = {
        "rate": rate,
        "burst": burst,
        "type": policer_type,
        "interfaces": [],
    }
    save_qos_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Policer '{name}' created",
        "policer": data["policers"][name],
    }))


def cmd_delete_policer(args):
    """Delete a policer from VPP."""
    data = load_qos_config()
    name = args.name

    if name not in data.get("policers", {}):
        print(json.dumps({"error": f"Policer '{name}' not found"}))
        sys.exit(1)

    # Remove from all interfaces first
    for iface in data["policers"][name].get("interfaces", []):
        run_vppctl(f"set interface policer {iface} 0")

    # Delete the policer
    stdout, stderr, rc = run_vppctl(f"policer del {name}")

    if rc != 0:
        print(json.dumps({
            "error": f"Failed to delete policer: {stderr}",
        }))
        sys.exit(1)

    del data["policers"][name]
    save_qos_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Policer '{name}' deleted",
    }))


def cmd_show_policers(args):
    """Show all policers and their statistics."""
    data = load_qos_config()

    # Get live policer info from VPP
    stdout, stderr, rc = run_vppctl("show policer")
    vpp_policer_output = stdout if rc == 0 else "N/A"

    # Also try to get detailed stats
    stdout2, _, rc2 = run_vppctl("show policer verbose")
    vpp_policer_verbose = stdout2 if rc2 == 0 else "N/A"

    result = {
        "status": "ok",
        "policers": data.get("policers", {}),
        "vpp_policer_output": vpp_policer_output,
        "vpp_policer_verbose": vpp_policer_verbose,
        "total": len(data.get("policers", {})),
    }

    print(json.dumps(result))


# ── Interface rate limiting ─────────────────────────────────────────

def cmd_set_interface_limit(args):
    """Set rate limit on an interface using a policer."""
    data = load_qos_config()
    iface = args.interface
    rate = args.rate
    burst = args.burst
    direction = args.direction  # "input", "output", or "both"

    # Create a named policer for this interface
    policer_name = f"{iface}-limit"

    # Delete existing interface policer if any
    run_vppctl(f"set interface policer {iface} 0")

    # Delete existing policer with this name
    run_vppctl(f"policer del {policer_name}")

    # Create new policer
    cmd_parts = ["policer", "add", policer_name, str(rate), str(burst), "type", "single_rate_two_color"]
    stdout, stderr, rc = run_vppctl(" ".join(cmd_parts))

    if rc != 0:
        print(json.dumps({
            "error": f"Failed to create rate limit policer: {stderr}",
        }))
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
        print(json.dumps({
            "error": f"Failed to apply policer to interface: {stderr}",
            "command": apply_cmd,
        }))
        sys.exit(1)

    # Store in config
    data["rate_limits"][iface] = {
        "rate": rate,
        "burst": burst,
        "direction": direction,
        "policer_name": policer_name,
    }
    save_qos_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Rate limit set on {iface}: {rate} bps, burst {burst}, direction {direction}",
    }))


def cmd_remove_interface_limit(args):
    """Remove rate limit from an interface."""
    data = load_qos_config()
    iface = args.interface

    if iface not in data.get("rate_limits", {}):
        print(json.dumps({"error": f"No rate limit configured on {iface}"}))
        sys.exit(1)

    policer_name = data["rate_limits"][iface].get("policer_name", f"{iface}-limit")

    # Remove policer from interface
    run_vppctl(f"set interface policer {iface} 0")

    # Delete the policer
    run_vppctl(f"policer del {policer_name}")

    del data["rate_limits"][iface]
    save_qos_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"Rate limit removed from {iface}",
    }))


# ── DSCP marking ───────────────────────────────────────────────────

def cmd_set_dscp_mark(args):
    """Set DSCP marking for traffic class."""
    data = load_qos_config()

    entry = {
        "protocol": args.protocol or "ip",
        "src_ip": args.src_ip,
        "dst_ip": args.dst_ip,
        "src_port": args.src_port,
        "dst_port": args.dst_port,
        "dscp": args.dscp,
        "description": args.description or "",
    }

    data.setdefault("dscp_marks", []).append(entry)
    save_qos_config(data)

    print(json.dumps({
        "status": "ok",
        "message": f"DSCP mark rule added: DSCP={args.dscp}",
        "rule": entry,
    }))


def cmd_show_dscp_marks(args):
    """Show DSCP marking rules."""
    data = load_qos_config()
    marks = data.get("dscp_marks", [])

    print(json.dumps({
        "status": "ok",
        "dscp_marks": marks,
        "total": len(marks),
    }))


# ── Full status ─────────────────────────────────────────────────────

def cmd_show_status(args):
    """Show full QoS status."""
    data = load_qos_config()

    # Get VPP policer info
    stdout, stderr, rc = run_vppctl("show policer")
    vpp_policer_output = stdout if rc == 0 else "N/A"

    result = {
        "status": "ok",
        "policers": data.get("policers", {}),
        "rate_limits": data.get("rate_limits", {}),
        "dscp_marks": data.get("dscp_marks", []),
        "vpp_policer_output": vpp_policer_output,
        "total_policers": len(data.get("policers", {})),
        "total_rate_limits": len(data.get("rate_limits", {})),
        "total_dscp_marks": len(data.get("dscp_marks", [])),
    }

    print(json.dumps(result))


def main():
    parser = argparse.ArgumentParser(description="VectorOS QoS Manager")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # create-policer
    p = subparsers.add_parser("create-policer", help="Create a policer")
    p.add_argument("--name", required=True, help="Policer name")
    p.add_argument("--rate", type=int, required=True, help="Rate in bits/sec")
    p.add_argument("--burst", type=int, required=True, help="Burst size in bytes")
    p.add_argument("--type", default="single_rate_two_color",
                   choices=["single_rate_two_color", "single_rate_three_color", "trtcm"],
                   help="Policer type")

    # delete-policer
    p = subparsers.add_parser("delete-policer", help="Delete a policer")
    p.add_argument("--name", required=True, help="Policer name")

    # show-policers
    subparsers.add_parser("show-policers", help="Show all policers and stats")

    # set-interface-limit
    p = subparsers.add_parser("set-interface-limit", help="Set rate limit on interface")
    p.add_argument("--interface", required=True, help="Interface name")
    p.add_argument("--rate", type=int, required=True, help="Rate in bits/sec")
    p.add_argument("--burst", type=int, required=True, help="Burst size in bytes")
    p.add_argument("--direction", default="both", choices=["input", "output", "both"],
                   help="Direction to apply limit")

    # remove-interface-limit
    p = subparsers.add_parser("remove-interface-limit", help="Remove rate limit from interface")
    p.add_argument("--interface", required=True, help="Interface name")

    # set-dscp-mark
    p = subparsers.add_parser("set-dscp-mark", help="Set DSCP marking rule")
    p.add_argument("--dscp", type=int, required=True, help="DSCP value (0-63)")
    p.add_argument("--protocol", default=None, help="Protocol (tcp, udp, ip)")
    p.add_argument("--src-ip", default=None, help="Source IP")
    p.add_argument("--dst-ip", default=None, help="Destination IP")
    p.add_argument("--src-port", type=int, default=None, help="Source port")
    p.add_argument("--dst-port", type=int, default=None, help="Destination port")
    p.add_argument("--description", default=None, help="Rule description")

    # show-dscp-marks
    subparsers.add_parser("show-dscp-marks", help="Show DSCP marking rules")

    # status
    subparsers.add_parser("status", help="Show full QoS status")

    args = parser.parse_args()

    try:
        if args.command == "create-policer":
            cmd_create_policer(args)
        elif args.command == "delete-policer":
            cmd_delete_policer(args)
        elif args.command == "show-policers":
            cmd_show_policers(args)
        elif args.command == "set-interface-limit":
            cmd_set_interface_limit(args)
        elif args.command == "remove-interface-limit":
            cmd_remove_interface_limit(args)
        elif args.command == "set-dscp-mark":
            cmd_set_dscp_mark(args)
        elif args.command == "show-dscp-marks":
            cmd_show_dscp_marks(args)
        elif args.command == "status":
            cmd_show_status(args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
