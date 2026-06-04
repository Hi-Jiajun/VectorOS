#!/usr/bin/env python3
"""VectorOS Firewall Manager — OPNsense-style

Provides rule groups, aliases (IP/port/network/URL), schedule-based rules,
GeoIP blocking, traffic shaping, and Suricata IDS management.
Persists to /etc/vectoros/firewall-rules.json and applies via vppctl ACL.
"""

import json
import sys
import argparse
import subprocess
from datetime import datetime, time as dtime
from pathlib import Path

RULES_FILE = Path("/etc/vectoros/firewall-rules.json")
VPPCTL = "vppctl"


# ── Helpers ──────────────────────────────────────────────────────────

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
    if RULES_FILE.exists():
        try:
            with open(RULES_FILE) as f:
                data = json.load(f)
            # Ensure all top-level keys exist for backward compat
            for key, default in [
                ("enabled", True),
                ("default_policy", "block"),
                ("rules", []),
                ("groups", []),
                ("aliases", []),
                ("schedules", []),
                ("geoip", {}),
                ("shaper", {}),
                ("ids", {}),
            ]:
                data.setdefault(key, default)
            return data
        except Exception:
            pass
    return {
        "enabled": True,
        "default_policy": "block",
        "rules": [],
        "groups": [],
        "aliases": [],
        "schedules": [],
        "geoip": {},
        "shaper": {},
        "ids": {},
    }


def save_rules(data):
    RULES_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(RULES_FILE, "w") as f:
        json.dump(data, f, indent=2)


# ── Schedule helpers ─────────────────────────────────────────────────

def is_schedule_active(schedule):
    """Check if a schedule is currently active."""
    if not schedule.get("enabled", True):
        return False
    now = datetime.now()
    weekday = now.weekday()  # 0=Monday .. 6=Sunday
    # Convert to 0=Sunday .. 6=Saturday
    day_num = (weekday + 1) % 7
    current_time = now.strftime("%H:%M")

    for tr in schedule.get("time_ranges", []):
        if tr.get("day") == day_num:
            start = tr.get("start", "00:00")
            end = tr.get("end", "23:59")
            if start <= end:
                if start <= current_time <= end:
                    return True
            else:
                # Overnight range
                if current_time >= start or current_time <= end:
                    return True
    return False


# ── Alias resolution ─────────────────────────────────────────────────

def resolve_alias(name, aliases):
    """Resolve an alias to its concrete entries."""
    for a in aliases:
        if a.get("name") == name and a.get("enabled", True):
            result = []
            for entry in a.get("entries", []):
                # Check for nested alias
                nested = False
                for inner in aliases:
                    if inner.get("name") == entry and inner.get("enabled", True):
                        nested = True
                        result.extend(resolve_alias(entry, aliases))
                if not nested:
                    result.append(entry)
            return result
    return []


# ── VPP ACL application ──────────────────────────────────────────────

def apply_rules_to_vpp(data):
    """Apply the full rule set to VPP via vppctl ACL commands."""
    run_vppctl("acl plugin enable")
    results = []

    sorted_rules = sorted(
        [r for r in data.get("rules", []) if r.get("enabled", True)],
        key=lambda r: r.get("order", 0),
    )

    for rule in sorted_rules:
        # Check schedule
        if rule.get("schedule"):
            sched = next(
                (s for s in data.get("schedules", []) if s["name"] == rule["schedule"]),
                None,
            )
            if sched and not is_schedule_active(sched):
                results.append({
                    "rule_id": rule["id"],
                    "skipped": True,
                    "reason": "schedule not active",
                })
                continue

        # Resolve aliases
        src_ips = []
        dst_ips = []
        src_ports = []
        dst_ports = []

        if rule.get("src_alias"):
            src_ips = resolve_alias(rule["src_alias"], data.get("aliases", []))
        if rule.get("dst_alias"):
            dst_ips = resolve_alias(rule["dst_alias"], data.get("aliases", []))
        if rule.get("src_port_alias"):
            src_ports = resolve_alias(rule["src_port_alias"], data.get("aliases", []))
        if rule.get("dst_port_alias"):
            dst_ports = resolve_alias(rule["dst_port_alias"], data.get("aliases", []))

        if not src_ips and rule.get("src_ip"):
            src_ips = [rule["src_ip"]]
        if not dst_ips and rule.get("dst_ip"):
            dst_ips = [rule["dst_ip"]]
        if not src_ports and rule.get("src_port"):
            src_ports = [str(rule["src_port"])]
        if not dst_ports and rule.get("dst_port"):
            dst_ports = [str(rule["dst_port"])]

        action = "permit" if rule.get("action") == "pass" else "deny"

        if not src_ips and not dst_ips and not src_ports and not dst_ports:
            cmd_parts = ["acl", "add", "action", action]
            out, err, rc = run_vppctl(" ".join(cmd_parts))
            results.append({"rule_id": rule["id"], "entry": " ".join(cmd_parts),
                            "stdout": out, "stderr": err, "rc": rc})
            continue

        src_ips = src_ips or [""]
        dst_ips = dst_ips or [""]
        src_ports = src_ports or [""]
        dst_ports = dst_ports or [""]

        for sip in src_ips:
            for dip in dst_ips:
                for sport in src_ports:
                    for dport in dst_ports:
                        cmd_parts = ["acl", "add"]
                        if sip:
                            cmd_parts.append(f"src-ip {sip}")
                        if dip:
                            cmd_parts.append(f"dst-ip {dip}")
                        if sport:
                            cmd_parts.append(f"src-port {sport}")
                        if dport:
                            cmd_parts.append(f"dst-port {dport}")
                        if rule.get("protocol") and rule["protocol"] != "ip":
                            cmd_parts.append(f"proto {rule['protocol']}")
                        cmd_parts.append(f"action {action}")

                        cmd = " ".join(cmd_parts)
                        out, err, rc = run_vppctl(cmd)
                        results.append({"rule_id": rule["id"], "entry": cmd,
                                        "stdout": out, "stderr": err, "rc": rc})

    return results


# ── CLI commands ──────────────────────────────────────────────────────

def cmd_add_rule(args):
    data = load_rules()
    rules = data.get("rules", [])
    next_id = max((r.get("id", 0) for r in rules), default=0) + 1
    next_order = max((r.get("order", 0) for r in rules), default=0) + 1

    new_rule = {
        "id": next_id,
        "action": args.action,
        "enabled": True,
        "direction": getattr(args, "direction", None) or "both",
        "protocol": args.protocol or "ip",
        "order": next_order,
        "geoip_countries": [],
        "match_group_geoip": False,
    }
    for field in ["src_ip", "dst_ip", "src_port", "dst_port", "src_alias",
                  "dst_alias", "src_port_alias", "dst_port_alias", "group",
                  "schedule", "log", "description", "dscp", "log_prefix"]:
        val = getattr(args, field.replace("-", "_"), None)
        if val is not None:
            new_rule[field] = val

    if hasattr(args, "geoip_countries") and args.geoip_countries:
        new_rule["geoip_countries"] = args.geoip_countries.split(",")

    rules.append(new_rule)
    data["rules"] = rules
    save_rules(data)

    if data.get("enabled", True):
        apply_rules_to_vpp(data)

    print(json.dumps({"status": "ok", "rule": new_rule, "total_rules": len(rules)}))


def cmd_del_rule(args):
    data = load_rules()
    rules = data.get("rules", [])
    original_count = len(rules)
    rules = [r for r in rules if r.get("id") != args.id]

    if len(rules) == original_count:
        print(json.dumps({"error": f"Rule with id {args.id} not found"}))
        sys.exit(1)

    # Remove from groups too
    for group in data.get("groups", []):
        group["rules"] = [r for r in group.get("rules", []) if r != args.id]

    data["rules"] = rules
    save_rules(data)

    if data.get("enabled", True):
        apply_rules_to_vpp(data)

    print(json.dumps({"status": "ok", "message": f"Rule {args.id} deleted",
                       "total_rules": len(rules)}))


def cmd_show(args):
    data = load_rules()
    rules = data.get("rules", [])
    active = len([r for r in rules if r.get("enabled", True)])

    # Evaluate schedule status
    for rule in rules:
        if rule.get("schedule"):
            sched = next(
                (s for s in data.get("schedules", []) if s["name"] == rule["schedule"]),
                None,
            )
            rule["schedule_active"] = is_schedule_active(sched) if sched else False
        else:
            rule["schedule_active"] = True

    stdout, stderr, rc = run_vppctl("show acl")
    vpp_acl_status = stdout if rc == 0 else "N/A"

    print(json.dumps({
        "status": "ok",
        "enabled": data.get("enabled", True),
        "default_policy": data.get("default_policy", "block"),
        "rules": rules,
        "groups": data.get("groups", []),
        "aliases": data.get("aliases", []),
        "schedules": data.get("schedules", []),
        "geoip": data.get("geoip", {}),
        "shaper": data.get("shaper", {}),
        "ids": data.get("ids", {}),
        "total_rules": len(rules),
        "active_rules": active,
        "vpp_acl_status": vpp_acl_status,
    }))


def cmd_enable(args):
    data = load_rules()
    data["enabled"] = True
    save_rules(data)
    run_vppctl("acl plugin enable")
    apply_rules_to_vpp(data)
    print(json.dumps({"status": "ok", "message": "Firewall enabled"}))


def cmd_disable(args):
    data = load_rules()
    data["enabled"] = False
    save_rules(data)
    run_vppctl("acl plugin disable")
    print(json.dumps({"status": "ok", "message": "Firewall disabled"}))


def cmd_add_group(args):
    data = load_rules()
    if any(g["name"] == args.name for g in data.get("groups", [])):
        print(json.dumps({"error": f"Group '{args.name}' already exists"}))
        sys.exit(1)

    group = {
        "name": args.name,
        "description": getattr(args, "description", None),
        "enabled": True,
        "rules": [],
        "interfaces": getattr(args, "interfaces", None) or [],
    }
    data.setdefault("groups", []).append(group)
    save_rules(data)
    print(json.dumps({"status": "ok", "group": group}))


def cmd_del_group(args):
    data = load_rules()
    original = len(data.get("groups", []))
    data["groups"] = [g for g in data.get("groups", []) if g["name"] != args.name]
    if len(data["groups"]) == original:
        print(json.dumps({"error": f"Group '{args.name}' not found"}))
        sys.exit(1)
    save_rules(data)
    print(json.dumps({"status": "ok", "message": f"Group '{args.name}' deleted"}))


def cmd_add_alias(args):
    data = load_rules()
    if any(a["name"] == args.name for a in data.get("aliases", [])):
        print(json.dumps({"error": f"Alias '{args.name}' already exists"}))
        sys.exit(1)

    entries = []
    if args.entries:
        entries = [e.strip() for e in args.entries.split(",")]

    alias = {
        "name": args.name,
        "type": args.type,
        "description": getattr(args, "description", None),
        "enabled": True,
        "entries": entries,
        "cached_entries": [],
        "last_fetched": None,
        "refresh_interval": getattr(args, "refresh_interval", 0) or 0,
    }
    data.setdefault("aliases", []).append(alias)
    save_rules(data)
    print(json.dumps({"status": "ok", "alias": alias}))


def cmd_del_alias(args):
    data = load_rules()
    original = len(data.get("aliases", []))
    data["aliases"] = [a for a in data.get("aliases", []) if a["name"] != args.name]
    if len(data["aliases"]) == original:
        print(json.dumps({"error": f"Alias '{args.name}' not found"}))
        sys.exit(1)

    # Remove alias references from rules
    for rule in data.get("rules", []):
        for field in ["src_alias", "dst_alias", "src_port_alias", "dst_port_alias"]:
            if rule.get(field) == args.name:
                rule[field] = None

    save_rules(data)
    print(json.dumps({"status": "ok", "message": f"Alias '{args.name}' deleted"}))


def cmd_refresh_alias(args):
    data = load_rules()
    alias = next(
        (a for a in data.get("aliases", []) if a["name"] == args.name and a["type"] == "url"),
        None,
    )
    if not alias:
        print(json.dumps({"error": f"URL alias '{args.name}' not found"}))
        sys.exit(1)

    import urllib.request
    all_entries = []
    for url in alias.get("entries", []):
        try:
            req = urllib.request.urlopen(url, timeout=10)
            body = req.read().decode("utf-8", errors="ignore")
            for line in body.splitlines():
                line = line.strip()
                if line and not line.startswith("#"):
                    all_entries.append(line)
        except Exception as e:
            print(f"Warning: Failed to fetch {url}: {e}", file=sys.stderr)

    alias["cached_entries"] = all_entries
    alias["last_fetched"] = datetime.now().isoformat()
    save_rules(data)
    print(json.dumps({"status": "ok", "alias": alias}))


def cmd_add_schedule(args):
    data = load_rules()
    if any(s["name"] == args.name for s in data.get("schedules", [])):
        print(json.dumps({"error": f"Schedule '{args.name}' already exists"}))
        sys.exit(1)

    time_ranges = []
    if args.time_ranges:
        for tr_str in args.time_ranges.split(";"):
            parts = tr_str.strip().split(",")
            if len(parts) == 3:
                time_ranges.append({
                    "day": int(parts[0]),
                    "start": parts[1],
                    "end": parts[2],
                })

    schedule = {
        "name": args.name,
        "description": getattr(args, "description", None),
        "enabled": True,
        "time_ranges": time_ranges,
    }
    data.setdefault("schedules", []).append(schedule)
    save_rules(data)
    print(json.dumps({"status": "ok", "schedule": schedule}))


def cmd_del_schedule(args):
    data = load_rules()
    original = len(data.get("schedules", []))
    data["schedules"] = [s for s in data.get("schedules", []) if s["name"] != args.name]
    if len(data["schedules"]) == original:
        print(json.dumps({"error": f"Schedule '{args.name}' not found"}))
        sys.exit(1)

    for rule in data.get("rules", []):
        if rule.get("schedule") == args.name:
            rule["schedule"] = None

    save_rules(data)
    print(json.dumps({"status": "ok", "message": f"Schedule '{args.name}' deleted"}))


def cmd_ids_config(args):
    data = load_rules()
    ids = data.setdefault("ids", {})
    ids["enabled"] = args.enabled
    ids.setdefault("interfaces", [])
    if args.interfaces:
        ids["interfaces"] = args.interfaces.split(",")
    ids.setdefault("rule_categories", {})
    ids.setdefault("alerts", [])
    ids.setdefault("stats", {"packets_inspected": 0, "alerts_total": 0,
                             "alerts_blocked": 0, "uptime_seconds": 0, "rules_loaded": 0})
    save_rules(data)

    if args.enabled:
        subprocess.Popen(
            ["suricata", "-c", "/etc/suricata/suricata.yaml", "-D"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    else:
        subprocess.run(["killall", "suricata"], capture_output=True)

    print(json.dumps({"status": "ok", "ids": ids}))


def main():
    parser = argparse.ArgumentParser(description="VectorOS Firewall Manager (OPNsense-style)")
    sub = parser.add_subparsers(dest="command", required=True)

    # ── Rules ────────────────────────────────────────────────────────
    add_p = sub.add_parser("add-rule", help="Add a firewall rule")
    add_p.add_argument("--action", choices=["pass", "block", "reject", "permit", "deny"],
                        required=True)
    add_p.add_argument("--direction", default="both", choices=["in", "out", "both"])
    add_p.add_argument("--src-ip")
    add_p.add_argument("--dst-ip")
    add_p.add_argument("--src-port")
    add_p.add_argument("--dst-port")
    add_p.add_argument("--src-alias")
    add_p.add_argument("--dst-alias")
    add_p.add_argument("--src-port-alias")
    add_p.add_argument("--dst-port-alias")
    add_p.add_argument("--protocol", default="ip")
    add_p.add_argument("--group")
    add_p.add_argument("--schedule")
    add_p.add_argument("--log", action="store_true")
    add_p.add_argument("--description")
    add_p.add_argument("--dscp")
    add_p.add_argument("--log-prefix")
    add_p.add_argument("--geoip-countries")

    del_p = sub.add_parser("del-rule", help="Delete a firewall rule")
    del_p.add_argument("--id", type=int, required=True)

    sub.add_parser("show", help="Show all firewall state")
    sub.add_parser("enable", help="Enable firewall")
    sub.add_parser("disable", help="Disable firewall")

    # ── Groups ───────────────────────────────────────────────────────
    grp_add = sub.add_parser("add-group", help="Add a rule group")
    grp_add.add_argument("--name", required=True)
    grp_add.add_argument("--description")
    grp_add.add_argument("--interfaces")

    grp_del = sub.add_parser("del-group", help="Delete a rule group")
    grp_del.add_argument("--name", required=True)

    # ── Aliases ──────────────────────────────────────────────────────
    al_add = sub.add_parser("add-alias", help="Add an alias")
    al_add.add_argument("--name", required=True)
    al_add.add_argument("--type", required=True,
                         choices=["host", "network", "port", "url"])
    al_add.add_argument("--description")
    al_add.add_argument("--entries", help="Comma-separated entries")
    al_add.add_argument("--refresh-interval", type=int, default=0)

    al_del = sub.add_parser("del-alias", help="Delete an alias")
    al_del.add_argument("--name", required=True)

    al_refresh = sub.add_parser("refresh-alias", help="Refresh URL alias")
    al_refresh.add_argument("--name", required=True)

    # ── Schedules ────────────────────────────────────────────────────
    sch_add = sub.add_parser("add-schedule", help="Add a schedule")
    sch_add.add_argument("--name", required=True)
    sch_add.add_argument("--description")
    sch_add.add_argument("--time-ranges",
                          help="Semicolon-separated: day,start,end;...")

    sch_del = sub.add_parser("del-schedule", help="Delete a schedule")
    sch_del.add_argument("--name", required=True)

    # ── IDS ──────────────────────────────────────────────────────────
    ids_p = sub.add_parser("ids-config", help="Configure IDS/Suricata")
    ids_p.add_argument("--enabled", action="store_true")
    ids_p.add_argument("--interfaces", help="Comma-separated interfaces")

    args = parser.parse_args()

    try:
        cmd_map = {
            "add-rule": cmd_add_rule,
            "del-rule": cmd_del_rule,
            "show": cmd_show,
            "enable": cmd_enable,
            "disable": cmd_disable,
            "add-group": cmd_add_group,
            "del-group": cmd_del_group,
            "add-alias": cmd_add_alias,
            "del-alias": cmd_del_alias,
            "refresh-alias": cmd_refresh_alias,
            "add-schedule": cmd_add_schedule,
            "del-schedule": cmd_del_schedule,
            "ids-config": cmd_ids_config,
        }
        cmd_map[args.command](args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
