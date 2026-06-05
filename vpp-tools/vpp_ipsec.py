#!/usr/bin/env python3
"""VectorOS VPP IPSec Manager - Direct VPP IKEv2/IPSec plugin management"""

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

def create_profile(name):
    """Create IKEv2 profile"""
    stdout, stderr, rc = run_vppctl(f'ikev2 profile add {name}')
    if rc != 0:
        return {'error': stderr or 'Failed to create profile'}
    return {'status': 'ok', 'message': f'Profile {name} created'}

def delete_profile(name):
    """Delete IKEv2 profile"""
    stdout, stderr, rc = run_vppctl(f'ikev2 profile del {name}')
    if rc != 0:
        return {'error': stderr or 'Failed to delete profile'}
    return {'status': 'ok', 'message': f'Profile {name} deleted'}

def set_auth(name, auth_type='shared-key-mic', data=''):
    """Set authentication for IKEv2 profile"""
    stdout, stderr, rc = run_vppctl(f'ikev2 profile set {name} auth {auth_type} string {data}')
    if rc != 0:
        return {'error': stderr or 'Failed to set auth'}
    return {'status': 'ok', 'message': f'Auth set for profile {name}'}

def set_id(name, side, id_type, data):
    """Set identity for IKEv2 profile"""
    stdout, stderr, rc = run_vppctl(f'ikev2 profile set {name} id {side} {id_type} {data}')
    if rc != 0:
        return {'error': stderr or 'Failed to set id'}
    return {'status': 'ok', 'message': f'ID set for profile {name}'}

def set_tunnel(name, interface):
    """Set tunnel interface for IKEv2 profile"""
    stdout, stderr, rc = run_vppctl(f'ikev2 profile set {name} tunnel {interface}')
    if rc != 0:
        return {'error': stderr or 'Failed to set tunnel'}
    return {'status': 'ok', 'message': f'Tunnel set for profile {name}'}

def initiate(name):
    """Initiate IKEv2 connection"""
    stdout, stderr, rc = run_vppctl(f'ikev2 initiate sa {name}')
    if rc != 0:
        return {'error': stderr or 'Failed to initiate'}
    return {'status': 'ok', 'message': f'Connection initiated for profile {name}'}

def show_sa():
    """Show IKEv2 Security Associations"""
    stdout, stderr, rc = run_vppctl('show ikev2 sa')
    if rc != 0:
        return {'error': stderr or 'Failed to show SAs'}

    # Parse SA output
    sas = []
    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('ISPI') and not line.startswith('---'):
            parts = line.split()
            if len(parts) >= 4:
                sas.append({
                    'ispi': parts[0],
                    'rspi': parts[1],
                    'state': parts[2],
                    'profile': parts[3] if len(parts) > 3 else ''
                })

    return {'sas': sas}

def show_profiles():
    """Show IKEv2 profiles"""
    stdout, stderr, rc = run_vppctl('show ikev2 profile')
    if rc != 0:
        return {'error': stderr or 'Failed to show profiles'}

    profiles = []
    current_profile = None

    for line in stdout.split('\n'):
        line = line.strip()
        if line.startswith('profile'):
            if current_profile:
                profiles.append(current_profile)
            parts = line.split()
            current_profile = {
                'name': parts[1] if len(parts) > 1 else '',
                'config': []
            }
        elif current_profile and line:
            current_profile['config'].append(line)

    if current_profile:
        profiles.append(current_profile)

    return {'profiles': profiles}

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS VPP IPSec Manager')
    parser.add_argument('action', choices=['create', 'delete', 'auth', 'id', 'tunnel', 'initiate', 'sa', 'profiles'])
    parser.add_argument('--name', help='Profile name')
    parser.add_argument('--auth-type', default='shared-key-mic')
    parser.add_argument('--data', help='Auth data')
    parser.add_argument('--side', choices=['local', 'remote'])
    parser.add_argument('--id-type', default='fqdn')
    parser.add_argument('--interface', help='Tunnel interface')

    args = parser.parse_args()

    try:
        if args.action == 'create':
            result = create_profile(args.name)
        elif args.action == 'delete':
            result = delete_profile(args.name)
        elif args.action == 'auth':
            result = set_auth(args.name, args.auth_type, args.data)
        elif args.action == 'id':
            result = set_id(args.name, args.side, args.id_type, args.data)
        elif args.action == 'tunnel':
            result = set_tunnel(args.name, args.interface)
        elif args.action == 'initiate':
            result = initiate(args.name)
        elif args.action == 'sa':
            result = show_sa()
        elif args.action == 'profiles':
            result = show_profiles()

        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
