#!/usr/bin/env python3
"""VectorOS VPP Tunnel Manager - Direct VPP GRE/VXLAN/GENEVE tunnel management"""

import sys
import json
import subprocess

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

def create_gre_tunnel(src, dst, instance=None):
    """Create GRE tunnel"""
    cmd = f'create gre tunnel src {src} dst {dst}'
    if instance is not None:
        cmd += f' instance {instance}'

    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to create GRE tunnel'}

    # Parse interface name from output
    iface_name = stdout.strip() if stdout else 'gre0'
    return {
        'status': 'ok',
        'interface': iface_name,
        'message': f'GRE tunnel {iface_name} created'
    }

def create_vxlan_tunnel(src, dst, vni, instance=None):
    """Create VXLAN tunnel"""
    cmd = f'create vxlan tunnel src {src} dst {dst} vni {vni}'
    if instance is not None:
        cmd += f' instance {instance}'

    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to create VXLAN tunnel'}

    iface_name = stdout.strip() if stdout else 'vxlan0'
    return {
        'status': 'ok',
        'interface': iface_name,
        'message': f'VXLAN tunnel {iface_name} created'
    }

def create_geneve_tunnel(src, dst, vni):
    """Create GENEVE tunnel"""
    cmd = f'create geneve tunnel local {src} remote {dst} vni {vni}'
    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to create GENEVE tunnel'}

    iface_name = stdout.strip() if stdout else 'geneve0'
    return {
        'status': 'ok',
        'interface': iface_name,
        'message': f'GENEVE tunnel {iface_name} created'
    }

def delete_tunnel(iface_name):
    """Delete tunnel interface"""
    stdout, stderr, rc = run_vppctl(f'create interface {iface_name} del')
    if rc != 0:
        return {'error': stderr or 'Failed to delete tunnel'}
    return {'status': 'ok', 'message': f'Tunnel {iface_name} deleted'}

def show_gre_tunnels():
    """Show GRE tunnels"""
    stdout, stderr, rc = run_vppctl('show gre tunnel')
    if rc != 0:
        return {'error': stderr or 'Failed to show GRE tunnels'}

    # Check if no tunnels
    if 'No' in stdout and 'configured' in stdout:
        return {'tunnels': [], 'message': 'No GRE tunnels configured'}

    tunnels = []
    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('Name') and not line.startswith('---'):
            parts = line.split()
            if len(parts) >= 3:
                tunnels.append({
                    'name': parts[0],
                    'src': parts[1],
                    'dst': parts[2],
                    'type': 'gre'
                })

    return {'tunnels': tunnels}

def show_vxlan_tunnels():
    """Show VXLAN tunnels"""
    stdout, stderr, rc = run_vppctl('show vxlan tunnel')
    if rc != 0:
        return {'error': stderr or 'Failed to show VXLAN tunnels'}

    # Check if no tunnels
    if 'No' in stdout and 'configured' in stdout:
        return {'tunnels': [], 'message': 'No VXLAN tunnels configured'}

    tunnels = []
    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('Name') and not line.startswith('---'):
            parts = line.split()
            if len(parts) >= 3:
                tunnels.append({
                    'name': parts[0],
                    'src': parts[1],
                    'dst': parts[2],
                    'vni': parts[3] if len(parts) > 3 else '0'
                })

    return {'tunnels': tunnels}

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS VPP Tunnel Manager')
    parser.add_argument('action', choices=['gre-create', 'vxlan-create', 'geneve-create', 'delete', 'gre-show', 'vxlan-show'])
    parser.add_argument('--src', help='Source IP')
    parser.add_argument('--dst', help='Destination IP')
    parser.add_argument('--vni', type=int, help='VNI')
    parser.add_argument('--instance', type=int, help='Instance ID')
    parser.add_argument('--name', help='Interface name')

    args = parser.parse_args()

    try:
        if args.action == 'gre-create':
            result = create_gre_tunnel(args.src, args.dst, args.instance)
        elif args.action == 'vxlan-create':
            result = create_vxlan_tunnel(args.src, args.dst, args.vni, args.instance)
        elif args.action == 'geneve-create':
            result = create_geneve_tunnel(args.src, args.dst, args.vni)
        elif args.action == 'delete':
            result = delete_tunnel(args.name)
        elif args.action == 'gre-show':
            result = show_gre_tunnels()
        elif args.action == 'vxlan-show':
            result = show_vxlan_tunnels()

        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
