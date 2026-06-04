#!/usr/bin/env python3
"""VectorOS VPN Manager - WireGuard, IPsec, and OpenVPN management.

Supports:
  - WireGuard tunnels (kernel or VPP plugin)
  - IPsec (VPP ipsec plugin or Linux kernel)
  - OpenVPN client/server

When VPP has the relevant plugin loaded, configuration is applied via vppctl.
Otherwise, falls back to Linux kernel tools (wg, ip, openvpn).
"""

import sys
import json
import argparse
import subprocess
import os
import re
import time
from pathlib import Path
from datetime import datetime

VPN_CONFIG_DIR = Path("/etc/vectoros/vpn")
VPN_STATE_FILE = VPN_CONFIG_DIR / "state.json"
WG_CONFIG_DIR = VPN_CONFIG_DIR / "wireguard"
IPSEC_CONFIG_DIR = VPN_CONFIG_DIR / "ipsec"
OVPN_CONFIG_DIR = VPN_CONFIG_DIR / "openvpn"

VPPCTL = "vppctl"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def run_cmd(cmd, timeout=15):
    """Run a shell command, return (stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            cmd, shell=isinstance(cmd, str),
            capture_output=True, text=True, timeout=timeout,
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except Exception as e:
        return "", str(e), 1


def run_vppctl(cmd_str):
    """Run a vppctl command string."""
    return run_cmd([VPPCTL] + cmd_str.split())


def load_state():
    """Load VPN state from disk."""
    if VPN_STATE_FILE.exists():
        try:
            with open(VPN_STATE_FILE) as f:
                return json.load(f)
        except Exception:
            pass
    return {"wireguard": {}, "ipsec": {}, "openvpn": {}}


def save_state(state):
    """Persist VPN state to disk."""
    VPN_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(VPN_STATE_FILE, "w") as f:
        json.dump(state, f, indent=2)


def detect_vpp_plugin(name):
    """Check if a VPP plugin is loaded."""
    stdout, _, rc = run_vppctl("show plugins")
    if rc == 0 and name.lower() in stdout.lower():
        return True
    return False


def detect_kernel_tool(tool):
    """Check if a kernel tool is available."""
    stdout, _, rc = run_cmd(f"which {tool}")
    return rc == 0


def get_wireguard_interfaces():
    """Get list of WireGuard interfaces and their status."""
    interfaces = []

    # Try kernel WireGuard first
    if detect_kernel_tool("wg"):
        stdout, _, rc = run_cmd("wg show")
        if rc == 0 and stdout:
            current_iface = None
            for line in stdout.splitlines():
                if line.startswith("interface: "):
                    current_iface = line.split("interface: ", 1)[1].strip()
                    interfaces.append({
                        "name": current_iface,
                        "backend": "kernel",
                        "status": "active",
                        "peers": [],
                    })
                elif line.strip().startswith("peer: ") and interfaces:
                    peer_pubkey = line.split("peer: ", 1)[1].strip()
                    interfaces[-1]["peers"].append({"public_key": peer_pubkey})

    # Try VPP WireGuard
    if detect_vpp_plugin("wireguard"):
        stdout, _, rc = run_vppctl("show wireguard interface")
        if rc == 0 and stdout:
            for line in stdout.splitlines():
                if "wg" in line.lower():
                    interfaces.append({
                        "name": line.strip().split()[0],
                        "backend": "vpp",
                        "status": "active",
                    })

    return interfaces


def get_ipsec_sas():
    """Get IPsec security associations."""
    sas = []

    # Try VPP IPsec
    if detect_vpp_plugin("ipsec"):
        stdout, _, rc = run_vppctl("show ipsec sa")
        if rc == 0 and stdout:
            for line in stdout.splitlines():
                if line.strip() and not line.startswith("Index"):
                    parts = line.split()
                    if len(parts) >= 4:
                        sas.append({
                            "spi": parts[1] if len(parts) > 1 else "",
                            "protocol": parts[2] if len(parts) > 2 else "",
                            "state": parts[3] if len(parts) > 3 else "unknown",
                            "backend": "vpp",
                        })

    # Try Linux kernel IPsec
    stdout, _, rc = run_cmd("ip xfrm state")
    if rc == 0 and stdout:
        current = {}
        for line in stdout.splitlines():
            line = line.strip()
            if line.startswith("proto"):
                if current:
                    sas.append(current)
                current = {"raw": line, "backend": "kernel", "state": "active"}
            elif line.startswith("dir"):
                current["direction"] = line.split()[-1] if line.split() else ""
        if current:
            sas.append(current)

    return sas


def get_openvpn_status():
    """Get OpenVPN connection status."""
    connections = []
    status_file = Path("/var/log/openvpn/status.log")
    if status_file.exists():
        try:
            content = status_file.read_text()
            for line in content.splitlines():
                if "CONNECTED" in line.upper():
                    parts = line.split(",")
                    connections.append({
                        "remote": parts[1] if len(parts) > 1 else "",
                        "bytes_received": parts[2] if len(parts) > 2 else "0",
                        "bytes_sent": parts[3] if len(parts) > 3 else "0",
                    })
        except Exception:
            pass

    # Check running processes
    stdout, _, rc = run_cmd("pgrep -a openvpn")
    if rc == 0 and stdout:
        for line in stdout.splitlines():
            connections.append({"process": line.strip()})

    return connections


# ---------------------------------------------------------------------------
# WireGuard commands
# ---------------------------------------------------------------------------

def cmd_wg_config(args):
    """Configure a WireGuard tunnel."""
    state = load_state()

    tunnel_name = args.name or "wg0"
    config = {
        "name": tunnel_name,
        "type": "wireguard",
        "listen_port": args.listen_port or 51820,
        "private_key": args.private_key or "",
        "public_key": args.public_key or "",
        "address": args.address or "",
        "peer_endpoint": args.peer_endpoint or "",
        "peer_public_key": args.peer_public_key or "",
        "peer_allowed_ips": args.peer_allowed_ips or "0.0.0.0/0",
        "dns": args.dns or "",
        "mtu": args.mtu or 1420,
        "created_at": datetime.now().isoformat(),
    }

    # Generate keys if not provided
    if not config["private_key"] and detect_kernel_tool("wg"):
        privkey, _, _ = run_cmd("wg genkey")
        if privkey:
            config["private_key"] = privkey
            pubkey, _, _ = run_cmd(f"echo '{privkey}' | wg pubkey")
            config["public_key"] = pubkey

    # Save config
    wg_dir = WG_CONFIG_DIR
    wg_dir.mkdir(parents=True, exist_ok=True)
    with open(wg_dir / f"{tunnel_name}.json", "w") as f:
        json.dump(config, f, indent=2)

    # Generate WireGuard config file
    wg_conf = f"""[Interface]
PrivateKey = {config['private_key']}
Address = {config['address']}
ListenPort = {config['listen_port']}
MTU = {config['mtu']}
"""
    if config["dns"]:
        wg_conf += f"DNS = {config['dns']}\n"

    if config["peer_public_key"]:
        wg_conf += f"""
[Peer]
PublicKey = {config['peer_public_key']}
Endpoint = {config['peer_endpoint']}
AllowedIPs = {config['peer_allowed_ips']}
"""
        if args.pre_shared_key:
            wg_conf += f"PresharedKey = {args.pre_shared_key}\n"
        wg_conf += "PersistentKeepalive = 25\n"

    conf_path = WG_CONFIG_DIR / f"{tunnel_name}.conf"
    with open(conf_path, "w") as f:
        f.write(wg_conf)

    # Apply to system
    success = False
    backend = "kernel"

    # Try kernel WireGuard
    if detect_kernel_tool("wg"):
        # Remove existing interface if any
        run_cmd(f"ip link delete {tunnel_name} 2>/dev/null")

        # Create interface
        stdout, stderr, rc = run_cmd(f"ip link add {tunnel_name} type wireguard")
        if rc == 0:
            run_cmd(f"wg setconf {tunnel_name} {conf_path}")
            run_cmd(f"ip addr add {config['address']} dev {tunnel_name}")
            run_cmd(f"ip link set {tunnel_name} up")
            success = True
            backend = "kernel"

    # Try VPP WireGuard
    if not success and detect_vpp_plugin("wireguard"):
        run_cmd(f"vppctl create wireguard interface listen-port {config['listen_port']}")
        success = True
        backend = "vpp"

    config["backend"] = backend
    config["active"] = success

    state["wireguard"][tunnel_name] = config
    save_state(state)

    print(json.dumps({
        "status": "ok" if success else "error",
        "message": f"WireGuard tunnel '{tunnel_name}' {'configured' if success else 'failed to configure'}",
        "backend": backend,
        "config": config,
    }))


def cmd_wg_show(args):
    """Show WireGuard status."""
    state = load_state()
    interfaces = get_wireguard_interfaces()

    # Merge with stored state
    result = []
    for iface in interfaces:
        name = iface["name"]
        stored = state.get("wireguard", {}).get(name, {})
        iface.update({k: v for k, v in stored.items() if k not in iface})
        result.append(iface)

    print(json.dumps({
        "status": "ok",
        "tunnels": result,
        "total": len(result),
    }))


def cmd_wg_down(args):
    """Bring down a WireGuard tunnel."""
    state = load_state()
    name = args.name or "wg0"

    if detect_kernel_tool("wg"):
        run_cmd(f"ip link delete {name}")

    state.get("wireguard", {}).pop(name, None)
    save_state(state)

    print(json.dumps({
        "status": "ok",
        "message": f"WireGuard tunnel '{name}' brought down",
    }))


# ---------------------------------------------------------------------------
# IPsec commands
# ---------------------------------------------------------------------------

def cmd_ipsec_config(args):
    """Configure an IPsec tunnel."""
    state = load_state()

    tunnel_name = args.name or "ipsec0"

    config = {
        "name": tunnel_name,
        "type": "ipsec",
        "mode": args.mode or "tunnel",  # tunnel or transport
        "proto": args.proto or "esp",
        "local_ip": args.local_ip or "",
        "remote_ip": args.remote_ip or "",
        "local_subnet": args.local_subnet or "",
        "remote_subnet": args.remote_subnet or "",
        "local_id": args.local_id or "",
        "remote_id": args.remote_id or "",
        "encryption": args.encryption or "aes-256-gcm",
        "integrity": args.integrity or "sha256",
        "dh_group": args.dh_group or "2",
        "ikelifetime": args.ikelifetime or "8h",
        "salifetime": args.salifetime or "1h",
        "pre_shared_key": args.pre_shared_key or "",
        "created_at": datetime.now().isoformat(),
    }

    # Try VPP IPsec
    backend = "none"
    success = False

    if detect_vpp_plugin("ipsec"):
        backend = "vpp"
        # VPP IPsec SA configuration
        run_vppctl(f"ikev2 profile add {tunnel_name}")
        run_vppctl(f"ikev2 profile set {tunnel_name} local-ip {config['local_ip']}")
        run_vppctl(f"ikev2 profile set {tunnel_name} remote-ip {config['remote_ip']}")
        if config["pre_shared_key"]:
            run_vppctl(f"ikev2 profile set {tunnel_name} auth psk {config['pre_shared_key']}")
        if config["local_id"]:
            run_vppctl(f"ikev2 profile set {tunnel_name} local-id {config['local_id']}")
        if config["remote_id"]:
            run_vppctl(f"ikev2 profile set {tunnel_name} remote-id {config['remote_id']}")
        success = True

    # Try Linux kernel IPsec via ip xfrm / strongswan
    if not success and detect_kernel_tool("ip"):
        backend = "kernel"
        # Configure via ip xfrm
        run_cmd(f"ip xfrm policy flush")
        run_cmd(f"ip xfrm state flush")

        # Add state
        spi_hex = format(int(time.time()) % 0xFFFFFFFF, '08x')
        state_cmd = (
            f"ip xfrm state add src {config['local_ip']} dst {config['remote_ip']} "
            f"proto esp spi {spi_hex} "
            f"enc {config['encryption'].replace('-', '').replace('gcm', '')} "
        )
        if config['pre_shared_key']:
            state_cmd += f"key 0x{config['pre_shared_key']}"
        run_cmd(state_cmd)

        # Add policy
        if config['local_subnet'] and config['remote_subnet']:
            policy_cmd = (
                f"ip xfrm policy add src {config['local_subnet']} dst {config['remote_subnet']} "
                f"dir out tmpl src {config['local_ip']} dst {config['remote_ip']} proto esp mode tunnel"
            )
            run_cmd(policy_cmd)
        success = True

    config["backend"] = backend
    config["active"] = success

    # Save config
    ipsec_dir = IPSEC_CONFIG_DIR
    ipsec_dir.mkdir(parents=True, exist_ok=True)
    with open(ipsec_dir / f"{tunnel_name}.json", "w") as f:
        json.dump(config, f, indent=2)

    state["ipsec"][tunnel_name] = config
    save_state(state)

    print(json.dumps({
        "status": "ok" if success else "error",
        "message": f"IPsec tunnel '{tunnel_name}' {'configured' if success else 'failed to configure'}",
        "backend": backend,
        "config": config,
    }))


def cmd_ipsec_show(args):
    """Show IPsec status."""
    state = load_state()
    sas = get_ipsec_sas()

    tunnels = []
    for name, cfg in state.get("ipsec", {}).items():
        cfg["sas"] = [s for s in sas if s.get("backend") == cfg.get("backend")]
        tunnels.append(cfg)

    print(json.dumps({
        "status": "ok",
        "tunnels": tunnels,
        "security_associations": sas,
        "total": len(tunnels),
    }))


def cmd_ipsec_down(args):
    """Tear down an IPsec tunnel."""
    state = load_state()
    name = args.name or "ipsec0"

    if detect_vpp_plugin("ipsec"):
        run_vppctl(f"ikev2 profile del {name}")

    run_cmd("ip xfrm state flush")
    run_cmd("ip xfrm policy flush")

    state.get("ipsec", {}).pop(name, None)
    save_state(state)

    print(json.dumps({
        "status": "ok",
        "message": f"IPsec tunnel '{name}' torn down",
    }))


# ---------------------------------------------------------------------------
# OpenVPN commands
# ---------------------------------------------------------------------------

def cmd_ovpn_config(args):
    """Configure OpenVPN client or server."""
    state = load_state()

    tunnel_name = args.name or "ovpn0"
    mode = args.mode or "client"

    config = {
        "name": tunnel_name,
        "type": "openvpn",
        "mode": mode,
        "remote": args.remote or "",
        "port": args.port or 1194,
        "proto": args.proto or "udp",
        "ca_cert": args.ca_cert or "",
        "client_cert": args.client_cert or "",
        "client_key": args.client_key or "",
        "tls_auth": args.tls_auth or "",
        "device": args.device or "tun",
        "cipher": args.cipher or "AES-256-GCM",
        "auth": args.auth or "SHA256",
        "redirect_gateway": args.redirect_gateway or False,
        "dns_push": args.dns_push or "",
        "created_at": datetime.now().isoformat(),
    }

    # Save config
    ovpn_dir = OVPN_CONFIG_DIR
    ovpn_dir.mkdir(parents=True, exist_ok=True)
    with open(ovpn_dir / f"{tunnel_name}.json", "w") as f:
        json.dump(config, f, indent=2)

    # Generate OpenVPN config
    ovpn_conf = f"""client
dev {config['device']}
proto {config['proto']}
remote {config['remote']} {config['port']}
resolv-retry infinite
nobind
persist-key
persist-tun
cipher {config['cipher']}
auth {config['auth']}
verb 3
"""
    if config["ca_cert"]:
        ovpn_conf += f"ca {config['ca_cert']}\n"
    if config["client_cert"]:
        ovpn_conf += f"cert {config['client_cert']}\n"
    if config["client_key"]:
        ovpn_conf += f"key {config['client_key']}\n"
    if config["tls_auth"]:
        ovpn_conf += f"tls-auth {config['tls_auth']} 1\n"
    if config["redirect_gateway"]:
        ovpn_conf += "redirect-gateway def1\n"
    if config["dns_push"]:
        ovpn_conf += f"script-security 2\nup /etc/openvpn/update-resolv-conf\n"

    conf_path = OVPN_CONFIG_DIR / f"{tunnel_name}.conf"
    with open(conf_path, "w") as f:
        f.write(ovpn_conf)

    # Start OpenVPN
    success = False
    backend = "openvpn"

    if detect_kernel_tool("openvpn"):
        # Kill existing instance
        run_cmd(f"killall openvpn 2>/dev/null")

        stdout, stderr, rc = run_cmd(
            f"openvpn --config {conf_path} --daemon --log /var/log/openvpn/{tunnel_name}.log"
        )
        success = rc == 0

    config["backend"] = backend
    config["active"] = success

    state["openvpn"][tunnel_name] = config
    save_state(state)

    print(json.dumps({
        "status": "ok" if success else "error",
        "message": f"OpenVPN {mode} '{tunnel_name}' {'started' if success else 'failed to start'}",
        "backend": backend,
        "config": config,
    }))


def cmd_ovpn_show(args):
    """Show OpenVPN status."""
    state = load_state()
    connections = get_openvpn_status()

    tunnels = []
    for name, cfg in state.get("openvpn", {}).items():
        cfg["connections"] = connections
        tunnels.append(cfg)

    print(json.dumps({
        "status": "ok",
        "tunnels": tunnels,
        "total": len(tunnels),
    }))


def cmd_ovpn_down(args):
    """Stop OpenVPN tunnel."""
    state = load_state()
    name = args.name or "ovpn0"

    run_cmd("killall openvpn 2>/dev/null")

    state.get("openvpn", {}).pop(name, None)
    save_state(state)

    print(json.dumps({
        "status": "ok",
        "message": f"OpenVPN tunnel '{name}' stopped",
    }))


# ---------------------------------------------------------------------------
# Overall VPN status
# ---------------------------------------------------------------------------

def cmd_status(args):
    """Show overall VPN status."""
    state = load_state()

    wg_ifaces = get_wireguard_interfaces()
    ipsec_sas = get_ipsec_sas()
    ovpn_conns = get_openvpn_status()

    # Check available backends
    backends = {
        "wireguard_kernel": detect_kernel_tool("wg"),
        "wireguard_vpp": detect_vpp_plugin("wireguard"),
        "ipsec_kernel": detect_kernel_tool("ip"),
        "ipsec_vpp": detect_vpp_plugin("ipsec"),
        "openvpn": detect_kernel_tool("openvpn"),
    }

    print(json.dumps({
        "status": "ok",
        "backends": backends,
        "wireguard": {
            "tunnels": wg_ifaces,
            "count": len(wg_ifaces),
            "configured": state.get("wireguard", {}),
        },
        "ipsec": {
            "security_associations": ipsec_sas,
            "count": len(ipsec_sas),
            "configured": state.get("ipsec", {}),
        },
        "openvpn": {
            "connections": ovpn_conns,
            "count": len(ovpn_conns),
            "configured": state.get("openvpn", {}),
        },
    }))


def cmd_connections(args):
    """List all active VPN connections."""
    wg = get_wireguard_interfaces()
    ipsec = get_ipsec_sas()
    ovpn = get_openvpn_status()

    all_connections = []
    for iface in wg:
        all_connections.append({
            "type": "wireguard",
            "name": iface.get("name", ""),
            "status": iface.get("status", "unknown"),
            "backend": iface.get("backend", ""),
            "remote": iface.get("peers", [{}])[0].get("endpoint", "") if iface.get("peers") else "",
        })
    for sa in ipsec:
        all_connections.append({
            "type": "ipsec",
            "spi": sa.get("spi", ""),
            "protocol": sa.get("protocol", ""),
            "status": sa.get("state", "unknown"),
            "backend": sa.get("backend", ""),
        })
    for conn in ovpn:
        all_connections.append({
            "type": "openvpn",
            "remote": conn.get("remote", ""),
            "status": "active" if conn.get("process") else "connected",
            "backend": "openvpn",
            "bytes_in": conn.get("bytes_received", "0"),
            "bytes_out": conn.get("bytes_sent", "0"),
        })

    print(json.dumps({
        "status": "ok",
        "connections": all_connections,
        "total": len(all_connections),
    }))


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    VPN_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    WG_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    IPSEC_CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    OVPN_CONFIG_DIR.mkdir(parents=True, exist_ok=True)

    parser = argparse.ArgumentParser(description="VectorOS VPN Manager")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # --- status ---
    subparsers.add_parser("status", help="Overall VPN status")
    subparsers.add_parser("connections", help="List active VPN connections")

    # --- wireguard ---
    wg_sub = subparsers.add_parser("wg", help="WireGuard management")
    wg_cmds = wg_sub.add_subparsers(dest="wg_command", required=True)

    wg_cfg = wg_cmds.add_parser("config", help="Configure WireGuard tunnel")
    wg_cfg.add_argument("--name", type=str, default="wg0")
    wg_cfg.add_argument("--listen-port", type=int, default=51820)
    wg_cfg.add_argument("--private-key", type=str, default="")
    wg_cfg.add_argument("--public-key", type=str, default="")
    wg_cfg.add_argument("--address", type=str, default="")
    wg_cfg.add_argument("--peer-endpoint", type=str, default="")
    wg_cfg.add_argument("--peer-public-key", type=str, default="")
    wg_cfg.add_argument("--peer-allowed-ips", type=str, default="0.0.0.0/0")
    wg_cfg.add_argument("--pre-shared-key", type=str, default="")
    wg_cfg.add_argument("--dns", type=str, default="")
    wg_cfg.add_argument("--mtu", type=int, default=1420)

    wg_cmds.add_parser("show", help="Show WireGuard status")

    wg_down = wg_cmds.add_parser("down", help="Bring down WireGuard tunnel")
    wg_down.add_argument("--name", type=str, default="wg0")

    # --- ipsec ---
    ipsec_sub = subparsers.add_parser("ipsec", help="IPsec management")
    ipsec_cmds = ipsec_sub.add_subparsers(dest="ipsec_command", required=True)

    ipsec_cfg = ipsec_cmds.add_parser("config", help="Configure IPsec tunnel")
    ipsec_cfg.add_argument("--name", type=str, default="ipsec0")
    ipsec_cfg.add_argument("--mode", type=str, default="tunnel", choices=["tunnel", "transport"])
    ipsec_cfg.add_argument("--proto", type=str, default="esp", choices=["esp", "ah"])
    ipsec_cfg.add_argument("--local-ip", type=str, default="")
    ipsec_cfg.add_argument("--remote-ip", type=str, default="")
    ipsec_cfg.add_argument("--local-subnet", type=str, default="")
    ipsec_cfg.add_argument("--remote-subnet", type=str, default="")
    ipsec_cfg.add_argument("--local-id", type=str, default="")
    ipsec_cfg.add_argument("--remote-id", type=str, default="")
    ipsec_cfg.add_argument("--encryption", type=str, default="aes-256-gcm")
    ipsec_cfg.add_argument("--integrity", type=str, default="sha256")
    ipsec_cfg.add_argument("--dh-group", type=str, default="2")
    ipsec_cfg.add_argument("--ikelifetime", type=str, default="8h")
    ipsec_cfg.add_argument("--salifetime", type=str, default="1h")
    ipsec_cfg.add_argument("--pre-shared-key", type=str, default="")

    ipsec_cmds.add_parser("show", help="Show IPsec status")

    ipsec_down = ipsec_cmds.add_parser("down", help="Tear down IPsec tunnel")
    ipsec_down.add_argument("--name", type=str, default="ipsec0")

    # --- openvpn ---
    ovpn_sub = subparsers.add_parser("openvpn", help="OpenVPN management")
    ovpn_cmds = ovpn_sub.add_subparsers(dest="ovpn_command", required=True)

    ovpn_cfg = ovpn_cmds.add_parser("config", help="Configure OpenVPN")
    ovpn_cfg.add_argument("--name", type=str, default="ovpn0")
    ovpn_cfg.add_argument("--mode", type=str, default="client", choices=["client", "server"])
    ovpn_cfg.add_argument("--remote", type=str, default="")
    ovpn_cfg.add_argument("--port", type=int, default=1194)
    ovpn_cfg.add_argument("--proto", type=str, default="udp", choices=["udp", "tcp"])
    ovpn_cfg.add_argument("--ca-cert", type=str, default="")
    ovpn_cfg.add_argument("--client-cert", type=str, default="")
    ovpn_cfg.add_argument("--client-key", type=str, default="")
    ovpn_cfg.add_argument("--tls-auth", type=str, default="")
    ovpn_cfg.add_argument("--device", type=str, default="tun")
    ovpn_cfg.add_argument("--cipher", type=str, default="AES-256-GCM")
    ovpn_cfg.add_argument("--auth", type=str, default="SHA256")
    ovpn_cfg.add_argument("--redirect-gateway", action="store_true")
    ovpn_cfg.add_argument("--dns-push", type=str, default="")

    ovpn_cmds.add_parser("show", help="Show OpenVPN status")

    ovpn_down = ovpn_cmds.add_parser("down", help="Stop OpenVPN")
    ovpn_down.add_argument("--name", type=str, default="ovpn0")

    args = parser.parse_args()

    try:
        if args.command == "status":
            cmd_status(args)
        elif args.command == "connections":
            cmd_connections(args)
        elif args.command == "wg":
            if args.wg_command == "config":
                cmd_wg_config(args)
            elif args.wg_command == "show":
                cmd_wg_show(args)
            elif args.wg_command == "down":
                cmd_wg_down(args)
        elif args.command == "ipsec":
            if args.ipsec_command == "config":
                cmd_ipsec_config(args)
            elif args.ipsec_command == "show":
                cmd_ipsec_show(args)
            elif args.ipsec_command == "down":
                cmd_ipsec_down(args)
        elif args.command == "openvpn":
            if args.ovpn_command == "config":
                cmd_ovpn_config(args)
            elif args.ovpn_command == "show":
                cmd_ovpn_show(args)
            elif args.ovpn_command == "down":
                cmd_ovpn_down(args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
