#!/usr/bin/env python3
"""VectorOS Firewall Manager - Basic firewall rules using VPP ACL plugin."""

import sys
import json
import argparse
import subprocess
from pathlib import Path

RULES_FILE = Path("/etc/vectoros/firewall-rules.json")
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


def load_rules():
    """Load saved rules from disk."""
    if RULES_FILE.exists():
        try:
            with open(RULES_FILE) as f:
                return json.load(f)
        except Exception:
            pass
    return {"enabled": True, "rules": []}


def save_rules(data):
    """Persist rules to disk."""
    RULES_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(RULES_FILE, "w") as f:
        json.dump(data, f, indent=2)


def build_acl_config(rules):
    """Build vppctl ACL configuration commands from rule list."""
    cmds = []
    # We use a single ACL (index 0) and rebuild it each time.
    # First collect per-action permit/deny lists.
    permit_ips = []
    deny_ips = []
    permit_ports = []
    deny_ports = []

    for rule in rules:
        if not rule.get("enabled", True):
            continue
        action = rule.get("action", "deny")
        src_ip = rule.get("src_ip")
        dst_ip = rule.get("dst_ip")
        src_port = rule.get("src_port")
        dst_port = rule.get("dst_port")
        protocol = rule.get("protocol", "ip")

        entry = {}
        if src_ip:
            entry["src_ip"] = src_ip
        if dst_ip:
            entry["dst_ip"] = dst_ip
        if src_port:
            entry["src_port"] = int(src_port)
        if dst_port:
            entry["dst_port"] = int(dst_port)
        if protocol and protocol != "ip":
            entry["proto"] = protocol

        if action == "permit":
            permit_ports.append(entry)
        else:
            deny_ports.append(entry)

    return permit_ports, deny_ports


def apply_rules_to_vpp(rules):
    """Apply the full rule set to VPP via vppctl."""
    # Reset ACL
    stdout, stderr, rc = run_vppctl("acl plugin enable")
    # Ignore error if already enabled

    # Delete existing ACL entries
    # VPP ACL commands use: acl add/del per-interface
    # We take a simpler approach: log what would be applied
    # and store the ruleset. The actual VPP ACL application
    # depends on the specific VPP ACL plugin version.

    permit_entries, deny_entries = build_acl_config(rules)

    results = []
    for entry in deny_entries:
        desc_parts = []
        if "src_ip" in entry:
            desc_parts.append(f"src {entry['src_ip']}")
        if "dst_ip" in entry:
            desc_parts.append(f"dst {entry['dst_ip']}")
        if "dst_port" in entry:
            desc_parts.append(f"dport {entry['dst_port']}")
        if "proto" in entry:
            desc_parts.append(f"proto {entry['proto']}")

        # Apply deny rule via vppctl acl
        cmd_parts = ["acl add"]
        if "src_ip" in entry:
            cmd_parts.append(f"src-ip {entry['src_ip']}")
        if "dst_ip" in entry:
            cmd_parts.append(f"dst-ip {entry['dst_ip']}")
        if "dst_port" in entry:
            cmd_parts.append(f"dst-port {entry['dst_port']}")
        cmd_parts.append("action deny")

        out, err, rc = run_vppctl(" ".join(cmd_parts))
        results.append({"entry": " ".join(cmd_parts), "stdout": out, "stderr": err, "rc": rc})

    for entry in permit_entries:
        cmd_parts = ["acl add"]
        if "src_ip" in entry:
            cmd_parts.append(f"src-ip {entry['src_ip']}")
        if "dst_ip" in entry:
            cmd_parts.append(f"dst-ip {entry['dst_ip']}")
        if "dst_port" in entry:
            cmd_parts.append(f"dst-port {entry['dst_port']}")
        cmd_parts.append("action permit")

        out, err, rc = run_vppctl(" ".join(cmd_parts))
        results.append({"entry": " ".join(cmd_parts), "stdout": out, "stderr": err, "rc": rc})

    return results


def cmd_add_rule(args):
    """Add a firewall rule."""
    data = load_rules()
    rules = data.get("rules", [])

    new_rule = {
        "id": max((r.get("id", 0) for r in rules), default=0) + 1,
        "action": args.action,
        "protocol": args.protocol or "ip",
        "enabled": True,
    }

    if args.src_ip:
        new_rule["src_ip"] = args.src_ip
    if args.dst_ip:
        new_rule["dst_ip"] = args.dst_ip
    if args.src_port:
        new_rule["src_port"] = int(args.src_port)
    if args.dst_port:
        new_rule["dst_port"] = int(args.dst_port)

    if args.description:
        new_rule["description"] = args.description

    rules.append(new_rule)
    data["rules"] = rules
    save_rules(data)

    # Apply to VPP if firewall is enabled
    if data.get("enabled", True):
        apply_rules_to_vpp(rules)

    print(json.dumps({"status": "ok", "rule": new_rule, "total_rules": len(rules)}))


def cmd_del_rule(args):
    """Delete a firewall rule by ID."""
    data = load_rules()
    rules = data.get("rules", [])
    original_count = len(rules)

    rules = [r for r in rules if r.get("id") != args.id]

    if len(rules) == original_count:
        print(json.dumps({"error": f"Rule with id {args.id} not found"}))
        sys.exit(1)

    data["rules"] = rules
    save_rules(data)

    if data.get("enabled", True):
        apply_rules_to_vpp(rules)

    print(json.dumps({"status": "ok", "message": f"Rule {args.id} deleted", "total_rules": len(rules)}))


def cmd_show(args):
    """Show current firewall rules and status."""
    data = load_rules()
    rules = data.get("rules", [])

    # Get VPP ACL status
    stdout, stderr, rc = run_vppctl("show acl")
    vpp_acl_status = stdout if rc == 0 else "N/A"

    result = {
        "status": "ok",
        "enabled": data.get("enabled", True),
        "rules": rules,
        "total_rules": len(rules),
        "active_rules": len([r for r in rules if r.get("enabled", True)]),
        "vpp_acl_status": vpp_acl_status,
    }

    print(json.dumps(result))


def cmd_enable(args):
    """Enable the firewall and apply rules."""
    data = load_rules()
    data["enabled"] = True
    save_rules(data)

    # Enable VPP ACL plugin
    run_vppctl("acl plugin enable")

    # Apply all active rules
    apply_rules_to_vpp(data.get("rules", []))

    print(json.dumps({"status": "ok", "message": "Firewall enabled"}))


def cmd_disable(args):
    """Disable the firewall."""
    data = load_rules()
    data["enabled"] = False
    save_rules(data)

    # Disable VPP ACL plugin
    run_vppctl("acl plugin disable")

    print(json.dumps({"status": "ok", "message": "Firewall disabled"}))


def main():
    parser = argparse.ArgumentParser(description="VectorOS Firewall Manager")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # add-rule
    add_parser = subparsers.add_parser("add-rule", help="Add a firewall rule")
    add_parser.add_argument("--action", choices=["permit", "deny"], required=True, help="Rule action")
    add_parser.add_argument("--src-ip", type=str, default=None, help="Source IP (CIDR notation)")
    add_parser.add_argument("--dst-ip", type=str, default=None, help="Destination IP (CIDR notation)")
    add_parser.add_argument("--src-port", type=int, default=None, help="Source port")
    add_parser.add_argument("--dst-port", type=int, default=None, help="Destination port")
    add_parser.add_argument("--protocol", type=str, default="ip", help="Protocol: ip, tcp, udp, icmp")
    add_parser.add_argument("--description", type=str, default=None, help="Rule description")

    # del-rule
    del_parser = subparsers.add_parser("del-rule", help="Delete a firewall rule")
    del_parser.add_argument("--id", type=int, required=True, help="Rule ID to delete")

    # show
    subparsers.add_parser("show", help="Show firewall rules and status")

    # enable
    subparsers.add_parser("enable", help="Enable firewall")

    # disable
    subparsers.add_parser("disable", help="Disable firewall")

    args = parser.parse_args()

    try:
        if args.command == "add-rule":
            cmd_add_rule(args)
        elif args.command == "del-rule":
            cmd_del_rule(args)
        elif args.command == "show":
            cmd_show(args)
        elif args.command == "enable":
            cmd_enable(args)
        elif args.command == "disable":
            cmd_disable(args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
