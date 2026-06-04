#!/usr/bin/env python3
"""VectorOS Interface Manager - VPP interface management"""

import sys
import json
import argparse
import subprocess

def run_vppctl(cmd):
    """Run a vppctl command"""
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

def iface_list():
    """List all interfaces"""
    stdout, stderr, rc = run_vppctl('show interface')
    if rc != 0:
        return {'error': stderr}

    interfaces = []
    for line in stdout.split('\n'):
        if not line.strip() or 'Name' in line or '---' in line:
            continue
        parts = line.split()
        if len(parts) >= 4:
            name = parts[0]
            idx = parts[1]
            state = parts[2]
            mtu = parts[3].split('/')[0] if '/' in parts[3] else parts[3]
            interfaces.append({
                'name': name,
                'sw_if_index': int(idx) if idx.isdigit() else 0,
                'state': state,
                'mtu': int(mtu) if mtu.isdigit() else 0
            })

    return {'interfaces': interfaces}

def iface_set_state(name, state):
    """Set interface state (up/down)"""
    stdout, stderr, rc = run_vppctl(f'set interface state {name} {state}')
    if rc != 0:
        return {'error': stderr}
    return {'status': 'ok', 'message': f'Interface {name} set to {state}'}

def iface_set_ip(name, ip_addr):
    """Set interface IP address"""
    stdout, stderr, rc = run_vppctl(f'set interface ip address {name} {ip_addr}')
    if rc != 0:
        return {'error': stderr}
    return {'status': 'ok', 'message': f'IP {ip_addr} set on {name}'}

def iface_set_mtu(name, mtu):
    """Set interface MTU"""
    stdout, stderr, rc = run_vppctl(f'set interface mtu packet {mtu} {name}')
    if rc != 0:
        return {'error': stderr}
    return {'status': 'ok', 'message': f'MTU {mtu} set on {name}'}

def iface_stats(name):
    """Get interface statistics"""
    stdout, stderr, rc = run_vppctl(f'show interface {name}')
    if rc != 0:
        return {'error': stderr}

    stats = {'name': name, 'counters': {}}
    for line in stdout.split('\n'):
        if 'rx packets' in line.lower():
            stats['counters']['rx_packets'] = line.split()[-1]
        elif 'tx packets' in line.lower():
            stats['counters']['tx_packets'] = line.split()[-1]
        elif 'rx bytes' in line.lower():
            stats['counters']['rx_bytes'] = line.split()[-1]
        elif 'tx bytes' in line.lower():
            stats['counters']['tx_bytes'] = line.split()[-1]

    return stats

def main():
    parser = argparse.ArgumentParser(description='VectorOS Interface Manager')
    parser.add_argument('action', choices=['list', 'up', 'down', 'set-ip', 'set-mtu', 'stats'])
    parser.add_argument('--name', help='Interface name')
    parser.add_argument('--ip', help='IP address (for set-ip)')
    parser.add_argument('--mtu', type=int, help='MTU value (for set-mtu)')

    args = parser.parse_args()

    try:
        if args.action == 'list':
            result = iface_list()
        elif args.action == 'up':
            result = iface_set_state(args.name, 'up')
        elif args.action == 'down':
            result = iface_set_state(args.name, 'down')
        elif args.action == 'set-ip':
            result = iface_set_ip(args.name, args.ip)
        elif args.action == 'set-mtu':
            result = iface_set_mtu(args.name, args.mtu)
        elif args.action == 'stats':
            result = iface_stats(args.name)

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
