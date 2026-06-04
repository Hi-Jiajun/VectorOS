#!/usr/bin/env python3
"""VectorOS VyOS-style Configuration CLI

Provides hierarchical configuration with set/delete commands,
commit/rollback system, config diff, and tab completion.

Usage:
    config_cli.py set interfaces eth0 address 192.168.1.1/24
    config_cli.py delete interfaces eth0 address
    config_cli.py commit
    config_cli.py rollback <version>
    config_cli.py show [path]
    config_cli.py diff [v1] [v2]
    config_cli.py history
    config_cli.py save-template <name>
    config_cli.py load-template <name>
    config_cli.py list-templates
"""

import sys
import os
import json
import copy
import hashlib
import argparse
import time
from pathlib import Path
from datetime import datetime
from typing import Any, Optional

CONFIG_DIR = os.environ.get("VECTOROS_CONFIG_DIR", "/etc/vectoros")
CONFIG_FILE = os.path.join(CONFIG_DIR, "config.json")
STAGING_FILE = os.path.join(CONFIG_DIR, ".config_staging.json")
HISTORY_DIR = os.path.join(CONFIG_DIR, "config_history")
TEMPLATE_DIR = os.path.join(CONFIG_DIR, "config_templates")

DEFAULT_CONFIG = {
    "interfaces": {
        "eth0": {
            "state": "up",
            "mtu": 1500,
            "address": []
        },
        "eth1": {
            "state": "up",
            "mtu": 1500,
            "address": ["192.168.1.1/24"]
        }
    },
    "pppoe": {
        "enabled": False,
        "username": "",
        "password": "",
        "interface": "eth0",
        "mtu": 1492,
        "mru": 1492,
        "use_peer_dns": True,
        "add_default_route4": True,
        "add_default_route6": True
    },
    "dhcp": {
        "enabled": False,
        "interface": "eth1",
        "start_ip": "192.168.1.100",
        "end_ip": "192.168.1.200",
        "gateway": "192.168.1.1",
        "lease_time": 86400
    },
    "dns": {
        "enabled": False,
        "upstream": ["8.8.8.8", "1.1.1.1"],
        "cache_size": 1000
    },
    "nat": {
        "enabled": False,
        "inside_if": "eth1",
        "outside_if": "eth0"
    },
    "firewall": {
        "enabled": False,
        "rules": []
    },
    "ipv6": {
        "enabled": False,
        "lan_prefix": "2001:db8:1::/64",
        "lan_address": "2001:db8:1::1/64",
        "wan_prefix": "2001:db8:2::/64",
        "upstream_dns": ["2001:4860:4860::8888", "2606:4700:4700::1111"]
    },
    "dhcpv6": {
        "enabled": False,
        "interface": "eth1",
        "range_start": "2001:db8:1::100",
        "range_end": "2001:db8:1::200",
        "gateway": "2001:db8:1::1",
        "lease_time": 86400
    },
    "vpn": {
        "wireguard": {},
        "ipsec": {},
        "openvpn": {}
    },
    "qos": {
        "enabled": False,
        "policers": {}
    },
    "traffic": {
        "enabled": False,
        "limits": {},
        "priorities": {},
        "app_classes": {}
    },
    "frr": {
        "enabled": False,
        "bgp": {},
        "ospf": {}
    }
}


def ensure_dirs():
    """Create required directories."""
    os.makedirs(CONFIG_DIR, exist_ok=True)
    os.makedirs(HISTORY_DIR, exist_ok=True)
    os.makedirs(TEMPLATE_DIR, exist_ok=True)


def load_config() -> dict:
    """Load committed configuration."""
    try:
        with open(CONFIG_FILE, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return copy.deepcopy(DEFAULT_CONFIG)


def save_config(config: dict):
    """Save committed configuration."""
    ensure_dirs()
    with open(CONFIG_FILE, 'w') as f:
        json.dump(config, f, indent=2)


def load_staging() -> Optional[dict]:
    """Load staged (uncommitted) configuration."""
    try:
        with open(STAGING_FILE, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def save_staging(config: dict):
    """Save staged configuration."""
    ensure_dirs()
    with open(STAGING_FILE, 'w') as f:
        json.dump(config, f, indent=2)


def clear_staging():
    """Remove staging file."""
    try:
        os.remove(STAGING_FILE)
    except FileNotFoundError:
        pass


def config_hash(config: dict) -> str:
    """Generate a short hash for a config version."""
    content = json.dumps(config, sort_keys=True, indent=2)
    return hashlib.sha256(content.encode()).hexdigest()[:12]


def get_active_config() -> dict:
    """Get the active (staged or committed) config."""
    staging = load_staging()
    if staging is not None:
        return staging
    return load_config()


# ── Tree operations ──────────────────────────────────────────────

def get_nested(config: dict, path: list) -> Any:
    """Navigate into a nested dict by path keys."""
    current = config
    for key in path:
        if isinstance(current, dict) and key in current:
            current = current[key]
        else:
            return None
    return current


def set_nested(config: dict, path: list, value: Any) -> dict:
    """Set a value at a nested path, creating intermediate dicts as needed."""
    config = copy.deepcopy(config)
    current = config
    for key in path[:-1]:
        if key not in current or not isinstance(current[key], dict):
            current[key] = {}
        current = current[key]

    last_key = path[-1]
    # Handle list appending (if current is a list and value is not a list)
    if isinstance(current.get(last_key), list) and not isinstance(value, list):
        if value not in current[last_key]:
            current[last_key].append(value)
    else:
        # Try to parse common types
        current[last_key] = _parse_value(value)

    return config


def delete_nested(config: dict, path: list) -> dict:
    """Delete a key at a nested path."""
    config = copy.deepcopy(config)
    current = config
    for key in path[:-1]:
        if key not in current or not isinstance(current[key], dict):
            return config  # Path doesn't exist
        current = current[key]

    last_key = path[-1]
    if last_key in current:
        if isinstance(current[last_key], list):
            # If last arg is a list item value, remove that item
            # Otherwise remove the whole list
            current.pop(last_key)
        else:
            current.pop(last_key)

    return config


def _parse_value(value: str) -> Any:
    """Parse a string value into appropriate Python type."""
    if not isinstance(value, str):
        return value  # Already parsed (e.g., bool, int)
    if value.lower() in ('true', 'yes', 'on'):
        return True
    if value.lower() in ('false', 'no', 'off'):
        return False
    try:
        return int(value)
    except ValueError:
        pass
    try:
        return float(value)
    except ValueError:
        pass
    return value


# ── Diff ─────────────────────────────────────────────────────────

def compute_diff(old: dict, new: dict, path: str = "") -> list:
    """Compute a structured diff between two configs."""
    changes = []

    all_keys = set(list(old.keys()) + list(new.keys()))

    for key in sorted(all_keys):
        full_path = f"{path} {key}".strip()

        old_val = old.get(key)
        new_val = new.get(key)

        if key not in old:
            changes.append({"op": "set", "path": full_path, "value": new_val})
        elif key not in new:
            changes.append({"op": "delete", "path": full_path, "value": old_val})
        elif isinstance(old_val, dict) and isinstance(new_val, dict):
            changes.extend(compute_diff(old_val, new_val, full_path))
        elif old_val != new_val:
            changes.append({"op": "update", "path": full_path, "old": old_val, "new": new_val})

    return changes


def format_diff(diff: list) -> str:
    """Format diff for display."""
    lines = []
    for change in diff:
        if change["op"] == "set":
            lines.append(f"+ {change['path']} = {json.dumps(change['value'])}")
        elif change["op"] == "delete":
            lines.append(f"- {change['path']}")
        elif change["op"] == "update":
            lines.append(f"~ {change['path']}: {json.dumps(change['old'])} -> {json.dumps(change['new'])}")
    return "\n".join(lines) if lines else "(no differences)"


# ── Config history ───────────────────────────────────────────────

def save_history(config: dict, message: str = ""):
    """Save a config snapshot to history."""
    ensure_dirs()
    version = config_hash(config)
    timestamp = datetime.now().isoformat()

    snapshot = {
        "version": version,
        "timestamp": timestamp,
        "message": message,
        "config": config
    }

    filepath = os.path.join(HISTORY_DIR, f"{version}.json")
    with open(filepath, 'w') as f:
        json.dump(snapshot, f, indent=2)

    # Update index
    index_path = os.path.join(HISTORY_DIR, "index.json")
    try:
        with open(index_path, 'r') as f:
            index = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        index = []

    index.append({
        "version": version,
        "timestamp": timestamp,
        "message": message
    })

    # Keep last 100 entries
    if len(index) > 100:
        index = index[-100:]

    with open(index_path, 'w') as f:
        json.dump(index, f, indent=2)

    return version


def load_history_version(version: str) -> Optional[dict]:
    """Load a specific config version from history."""
    filepath = os.path.join(HISTORY_DIR, f"{version}.json")
    try:
        with open(filepath, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def list_history() -> list:
    """List all config history entries."""
    index_path = os.path.join(HISTORY_DIR, "index.json")
    try:
        with open(index_path, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return []


# ── Templates ────────────────────────────────────────────────────

def save_template(name: str, config: dict, description: str = ""):
    """Save a config as a named template."""
    ensure_dirs()
    template = {
        "name": name,
        "description": description,
        "created": datetime.now().isoformat(),
        "config": config
    }
    filepath = os.path.join(TEMPLATE_DIR, f"{name}.json")
    with open(filepath, 'w') as f:
        json.dump(template, f, indent=2)


def load_template(name: str) -> Optional[dict]:
    """Load a template by name."""
    filepath = os.path.join(TEMPLATE_DIR, f"{name}.json")
    try:
        with open(filepath, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def list_templates() -> list:
    """List all saved templates."""
    ensure_dirs()
    templates = []
    for f in Path(TEMPLATE_DIR).glob("*.json"):
        try:
            with open(f, 'r') as fh:
                data = json.load(fh)
                templates.append({
                    "name": data.get("name", f.stem),
                    "description": data.get("description", ""),
                    "created": data.get("created", "")
                })
        except (json.JSONDecodeError, KeyError):
            continue
    return templates


def apply_template(name: str, variables: dict = None) -> Optional[dict]:
    """Apply a template, optionally substituting variables."""
    template = load_template(name)
    if template is None:
        return None

    config = copy.deepcopy(template["config"])

    if variables:
        config_str = json.dumps(config)
        for key, value in variables.items():
            config_str = config_str.replace(f"{{{key}}}", str(value))
        config = json.loads(config_str)

    return config


# ── Show / display ───────────────────────────────────────────────

def format_tree(config: dict, prefix: str = "", is_last: bool = True, path: str = "") -> str:
    """Format config as a tree for display."""
    lines = []
    items = list(config.items()) if isinstance(config, dict) else []

    if not items:
        if isinstance(config, list):
            return ", ".join(str(x) for x in config) if config else "(empty)"
        return str(config) if config is not None else "(none)"

    for i, (key, value) in enumerate(items):
        is_last_item = (i == len(items) - 1)
        connector = "└── " if is_last_item else "├── "
        current_path = f"{path} {key}".strip()

        if isinstance(value, dict):
            lines.append(f"{prefix}{connector}{key}")
            extension = "    " if is_last_item else "│   "
            lines.append(format_tree(value, prefix + extension, is_last_item, current_path))
        elif isinstance(value, list):
            lines.append(f"{prefix}{connector}{key}")
            extension = "    " if is_last_item else "│   "
            if value:
                for j, item in enumerate(value):
                    item_last = (j == len(value) - 1)
                    item_connector = "└── " if item_last else "├── "
                    lines.append(f"{prefix}{extension}{item_connector}{item}")
            else:
                lines.append(f"{prefix}{extension}└── (empty)")
        else:
            lines.append(f"{prefix}{connector}{key} = {json.dumps(value)}")

    return "\n".join(lines)


def show_config(path: str = "", config: dict = None) -> str:
    """Show config, optionally at a specific path."""
    if config is None:
        config = get_active_config()

    if path:
        keys = [k for k in path.split() if k]
        node = get_nested(config, keys)
        if node is None:
            return f"Path not found: {path}"
        if isinstance(node, dict):
            return format_tree(node, path=f" ".join(keys))
        elif isinstance(node, list):
            return ", ".join(str(x) for x in node) if node else "(empty)"
        else:
            return str(node)

    return format_tree(config)


# ── Tab completion data ──────────────────────────────────────────

COMPLETIONS = {
    "root": ["interfaces", "pppoe", "dhcp", "dns", "nat", "firewall",
             "ipv6", "dhcpv6", "vpn", "qos", "traffic", "frr"],
    "interfaces": {"_help": "Network interface configuration",
                   "_children": ["eth0", "eth1", "lo"]},
    "pppoe": {"_help": "PPPoE client configuration",
              "enabled": {"_help": "Enable/disable PPPoE", "_values": ["true", "false"]},
              "username": {"_help": "PPPoE username"},
              "password": {"_help": "PPPoE password"},
              "interface": {"_help": "Underlying interface"},
              "mtu": {"_help": "MTU value (default: 1492)"},
              "mru": {"_help": "MRU value (default: 1492)"},
              "use_peer_dns": {"_help": "Use peer DNS", "_values": ["true", "false"]},
              "add_default_route4": {"_help": "Add IPv4 default route", "_values": ["true", "false"]},
              "add_default_route6": {"_help": "Add IPv6 default route", "_values": ["true", "false"]}},
    "dhcp": {"_help": "DHCP server configuration",
             "enabled": {"_values": ["true", "false"]},
             "interface": {"_help": "DHCP listening interface"},
             "start_ip": {"_help": "DHCP range start"},
             "end_ip": {"_help": "DHCP range end"},
             "gateway": {"_help": "Default gateway"},
             "lease_time": {"_help": "Lease time in seconds"}},
    "dns": {"_help": "DNS forwarding configuration",
            "enabled": {"_values": ["true", "false"]},
            "upstream": {"_help": "Upstream DNS servers"},
            "cache_size": {"_help": "DNS cache size"}},
}


# ── Commands ─────────────────────────────────────────────────────

def cmd_set(args):
    """Set a configuration value at a path."""
    path_keys = args.path
    value = args.value

    if not path_keys:
        return {"error": "Path is required. Example: set interfaces eth0 address 192.168.1.1/24"}

    staging = load_staging()
    if staging is None:
        staging = load_config()

    try:
        if value is not None:
            new_config = set_nested(staging, path_keys, value)
        else:
            # No value means set with empty/true
            new_config = set_nested(staging, path_keys, True)

        save_staging(new_config)
        path_str = " ".join(path_keys)
        val_display = value if value else "enabled"
        return {
            "status": "staged",
            "message": f"Set {path_str} = {val_display}",
            "note": "Changes staged. Use 'commit' to apply."
        }
    except Exception as e:
        return {"error": f"Failed to set: {e}"}


def cmd_delete(args):
    """Delete a configuration value at a path."""
    path_keys = args.path

    if not path_keys:
        return {"error": "Path is required. Example: delete interfaces eth0 address"}

    staging = load_staging()
    if staging is None:
        staging = load_config()

    try:
        new_config = delete_nested(staging, path_keys)
        save_staging(new_config)
        path_str = " ".join(path_keys)
        return {
            "status": "staged",
            "message": f"Deleted {path_str}",
            "note": "Changes staged. Use 'commit' to apply."
        }
    except Exception as e:
        return {"error": f"Failed to delete: {e}"}


def cmd_commit(args):
    """Commit staged configuration changes."""
    staging = load_staging()
    if staging is None:
        return {"status": "ok", "message": "No changes to commit"}

    committed = load_config()
    diff = compute_diff(committed, staging)

    if not diff:
        clear_staging()
        return {"status": "ok", "message": "No differences to commit"}

    message = args.message if hasattr(args, 'message') and args.message else f"Committed {len(diff)} change(s)"

    # Save old version to history
    save_history(committed, "pre-commit snapshot")

    # Apply
    save_config(staging)
    new_version = save_history(staging, message)
    clear_staging()

    return {
        "status": "ok",
        "message": message,
        "version": new_version,
        "changes": len(diff),
        "diff": format_diff(diff)
    }


def cmd_rollback(args):
    """Rollback to a previous config version."""
    version = args.version

    if not version:
        return {"error": "Version hash is required. Use 'history' to see available versions."}

    snapshot = load_history_version(version)
    if snapshot is None:
        return {"error": f"Version {version} not found in history"}

    # Save current config to history before rollback
    current = load_config()
    save_history(current, f"pre-rollback to {version}")

    # Apply rollback
    save_config(snapshot["config"])
    new_version = save_history(snapshot["config"], f"Rolled back to {version}")

    return {
        "status": "ok",
        "message": f"Rolled back to version {version}",
        "new_version": new_version,
        "rolled_back_from": config_hash(current),
        "rolled_back_to": version
    }


def cmd_show(args):
    """Show configuration tree."""
    path = " ".join(args.path) if args.path else ""
    output = show_config(path)
    return {"config": output}


def cmd_diff(args):
    """Show diff between committed and staged config, or between two versions."""
    committed = load_config()
    staging = load_staging()

    if args.v1 and args.v2:
        snap1 = load_history_version(args.v1)
        snap2 = load_history_version(args.v2)
        if not snap1:
            return {"error": f"Version {args.v1} not found"}
        if not snap2:
            return {"error": f"Version {args.v2} not found"}
        diff = compute_diff(snap1["config"], snap2["config"])
    elif staging is not None:
        diff = compute_diff(committed, staging)
    else:
        return {"status": "ok", "message": "No staged changes to diff", "diff": ""}

    return {
        "status": "ok",
        "diff": format_diff(diff),
        "changes": len(diff)
    }


def cmd_history(args):
    """Show config change history."""
    entries = list_history()
    limit = args.limit if hasattr(args, 'limit') and args.limit else 20
    recent = entries[-limit:]

    lines = []
    for entry in reversed(recent):
        lines.append(f"{entry['version']}  {entry['timestamp']}  {entry.get('message', '')}")

    return {
        "history": recent,
        "formatted": "\n".join(lines) if lines else "(no history)"
    }


def cmd_save_template(args):
    """Save current config as a template."""
    config = load_config()
    save_template(args.name, config, args.description or "")
    return {
        "status": "ok",
        "message": f"Template '{args.name}' saved"
    }


def cmd_load_template(args):
    """Load a template into staging."""
    template = load_template(args.name)
    if template is None:
        return {"error": f"Template '{args.name}' not found"}

    save_staging(template["config"])
    return {
        "status": "ok",
        "message": f"Template '{args.name}' loaded to staging",
        "note": "Use 'commit' to apply"
    }


def cmd_apply_template(args):
    """Apply a template with variable substitution."""
    variables = {}
    if args.vars:
        for v in args.vars:
            if '=' in v:
                k, val = v.split('=', 1)
                variables[k] = val

    config = apply_template(args.name, variables)
    if config is None:
        return {"error": f"Template '{args.name}' not found"}

    save_staging(config)
    return {
        "status": "ok",
        "message": f"Template '{args.name}' applied to staging" + (f" with {len(variables)} variables" if variables else ""),
        "note": "Use 'commit' to apply"
    }


def cmd_list_templates(args):
    """List all saved templates."""
    templates = list_templates()
    return {"templates": templates}


def cmd_discard(args):
    """Discard staged (uncommitted) changes."""
    clear_staging()
    return {"status": "ok", "message": "Staged changes discarded"}


def cmd_complete(args):
    """Provide tab completion suggestions."""
    path = args.path or []
    level = len(path)

    if level == 0:
        completions = list(COMPLETIONS.keys())
    elif level == 1:
        section = path[0]
        if section in COMPLETIONS and isinstance(COMPLETIONS[section], dict):
            completions = [k for k in COMPLETIONS[section].keys() if not k.startswith('_')]
        else:
            completions = []
    else:
        completions = []

    return {"completions": completions, "path": path}


# ── Main CLI ─────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description='VectorOS VyOS-style Configuration CLI')
    subparsers = parser.add_subparsers(dest='command', help='Available commands')

    # set
    p_set = subparsers.add_parser('set', help='Set a configuration value')
    p_set.add_argument('path', nargs='+', help='Config path (e.g., interfaces eth0 address)')
    p_set.add_argument('-v', '--value', help='Value to set')

    # delete
    p_del = subparsers.add_parser('delete', help='Delete a configuration value')
    p_del.add_argument('path', nargs='+', help='Config path to delete')

    # commit
    p_commit = subparsers.add_parser('commit', help='Commit staged changes')
    p_commit.add_argument('-m', '--message', help='Commit message')

    # rollback
    p_rollback = subparsers.add_parser('rollback', help='Rollback to a version')
    p_rollback.add_argument('version', help='Version hash to rollback to')

    # show
    p_show = subparsers.add_parser('show', help='Show configuration')
    p_show.add_argument('path', nargs='*', help='Path to show (empty = full config)')

    # diff
    p_diff = subparsers.add_parser('diff', help='Show config differences')
    p_diff.add_argument('v1', nargs='?', help='First version (optional)')
    p_diff.add_argument('v2', nargs='?', help='Second version (optional)')

    # history
    p_hist = subparsers.add_parser('history', help='Show config history')
    p_hist.add_argument('-n', '--limit', type=int, default=20, help='Number of entries')

    # save-template
    p_st = subparsers.add_parser('save-template', help='Save config as template')
    p_st.add_argument('name', help='Template name')
    p_st.add_argument('-d', '--description', help='Template description')

    # load-template
    p_lt = subparsers.add_parser('load-template', help='Load template to staging')
    p_lt.add_argument('name', help='Template name')

    # apply-template
    p_at = subparsers.add_parser('apply-template', help='Apply template with variables')
    p_at.add_argument('name', help='Template name')
    p_at.add_argument('vars', nargs='*', help='Variables as key=value')

    # list-templates
    subparsers.add_parser('list-templates', help='List saved templates')

    # discard
    subparsers.add_parser('discard', help='Discard staged changes')

    # complete (for tab completion)
    p_complete = subparsers.add_parser('complete', help='Tab completion')
    p_complete.add_argument('path', nargs='*', help='Current path')

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        sys.exit(0)

    commands = {
        'set': cmd_set,
        'delete': cmd_delete,
        'commit': cmd_commit,
        'rollback': cmd_rollback,
        'show': cmd_show,
        'diff': cmd_diff,
        'history': cmd_history,
        'save-template': cmd_save_template,
        'load-template': cmd_load_template,
        'apply-template': cmd_apply_template,
        'list-templates': cmd_list_templates,
        'discard': cmd_discard,
        'complete': cmd_complete,
    }

    handler = commands.get(args.command)
    if handler:
        result = handler(args)
        print(json.dumps(result, indent=2))
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == '__main__':
    main()
