#!/usr/bin/env python3
"""VectorOS VPP WireGuard Manager - Direct VPP WireGuard plugin management"""

import sys
import json
import subprocess
import base64
import os

def run_vppctl(cmd):
    """Run vppctl command"""
    try:
        result = subprocess.run(
            ['vppctl'] + cmd.split(),
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except Exception as e:
        return '', str(e), 1

def generate_keypair():
    """Generate WireGuard keypair"""
    try:
        # Generate private key
        result = subprocess.run(['wg', 'genkey'], capture_output=True, text=True)
        if result.returncode != 0:
            return {'error': 'Failed to generate private key'}
        private_key = result.stdout.strip()

        # Generate public key from private key
        result = subprocess.run(['wg', 'pubkey'], input=private_key, capture_output=True, text=True)
        if result.returncode != 0:
            return {'error': 'Failed to generate public key'}
        public_key = result.stdout.strip()

        return {
            'private_key': private_key,
            'public_key': public_key
        }
    except FileNotFoundError:
        return {'error': 'wg tool not installed'}

def create_interface(listen_port, private_key, src_ip=None):
    """Create WireGuard interface in VPP"""
    cmd = f'wireguard create listen-port {listen_port} private-key {private_key}'
    if src_ip:
        cmd += f' src {src_ip}'

    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to create WireGuard interface'}

    # Parse interface name from output
    iface_name = stdout.strip() if stdout else 'wg0'
    return {
        'status': 'ok',
        'interface': iface_name,
        'message': f'WireGuard interface {iface_name} created'
    }

def delete_interface(iface_name):
    """Delete WireGuard interface"""
    stdout, stderr, rc = run_vppctl(f'wireguard {iface_name} delete')
    if rc != 0:
        return {'error': stderr or 'Failed to delete WireGuard interface'}
    return {'status': 'ok', 'message': f'WireGuard interface {iface_name} deleted'}

def add_peer(iface_name, public_key, endpoint=None, port=None, allowed_ips=None, preshared_key=None):
    """Add peer to WireGuard interface"""
    cmd = f'wireguard peer add {iface_name} public-key {public_key}'

    if endpoint and port:
        cmd += f' endpoint {endpoint} port {port}'

    if allowed_ips:
        for ip in allowed_ips:
            cmd += f' allowed-ip {ip}'

    if preshared_key:
        cmd += f' preshared-key {preshared_key}'

    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to add peer'}

    return {'status': 'ok', 'message': 'Peer added successfully'}

def remove_peer(iface_name, public_key):
    """Remove peer from WireGuard interface"""
    stdout, stderr, rc = run_vppctl(f'wireguard peer remove {iface_name} public-key {public_key}')
    if rc != 0:
        return {'error': stderr or 'Failed to remove peer'}
    return {'status': 'ok', 'message': 'Peer removed successfully'}

def show_interfaces():
    """Show WireGuard interfaces"""
    stdout, stderr, rc = run_vppctl('show wireguard interface')
    if rc != 0:
        return {'error': stderr or 'Failed to show interfaces'}

    interfaces = []
    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('Interface') and not line.startswith('---'):
            parts = line.split()
            if len(parts) >= 2:
                interfaces.append({
                    'name': parts[0],
                    'index': parts[1] if len(parts) > 1 else '',
                    'state': parts[2] if len(parts) > 2 else 'unknown'
                })

    return {'interfaces': interfaces}

def show_peers():
    """Show WireGuard peers"""
    stdout, stderr, rc = run_vppctl('show wireguard peer')
    if rc != 0:
        return {'error': stderr or 'Failed to show peers'}

    peers = []
    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('Peer') and not line.startswith('---'):
            parts = line.split()
            if len(parts) >= 2:
                peers.append({
                    'interface': parts[0] if len(parts) > 0 else '',
                    'public_key': parts[1] if len(parts) > 1 else '',
                    'endpoint': parts[2] if len(parts) > 2 and parts[2] != '(none)' else None,
                    'allowed_ips': parts[3:] if len(parts) > 3 else []
                })

    return {'peers': peers}

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS VPP WireGuard Manager')
    parser.add_argument('action', choices=['create', 'delete', 'peer-add', 'peer-remove', 'show', 'peers', 'genkey'])
    parser.add_argument('--interface', help='WireGuard interface name')
    parser.add_argument('--listen-port', type=int, default=51820)
    parser.add_argument('--private-key', help='Private key')
    parser.add_argument('--src-ip', help='Source IP address')
    parser.add_argument('--public-key', help='Peer public key')
    parser.add_argument('--endpoint', help='Peer endpoint')
    parser.add_argument('--port', type=int, help='Peer port')
    parser.add_argument('--allowed-ips', nargs='+', help='Allowed IPs')
    parser.add_argument('--preshared-key', help='Preshared key')

    args = parser.parse_args()

    try:
        if args.action == 'genkey':
            result = generate_keypair()
        elif args.action == 'create':
            result = create_interface(args.listen_port, args.private_key, args.src_ip)
        elif args.action == 'delete':
            result = delete_interface(args.interface)
        elif args.action == 'peer-add':
            result = add_peer(args.interface, args.public_key, args.endpoint, args.port, args.allowed_ips, args.preshared_key)
        elif args.action == 'peer-remove':
            result = remove_peer(args.interface, args.public_key)
        elif args.action == 'show':
            result = show_interfaces()
        elif args.action == 'peers':
            result = show_peers()

        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
