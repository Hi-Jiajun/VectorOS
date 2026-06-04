#!/usr/bin/env python3
"""VectorOS NAT Manager - Python wrapper for VPP NAT"""

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

def nat_enable(inside_if='lan0', outside_if='pppoe-wan0'):
    """Enable NAT44 EI on interfaces"""
    errors = []

    # Enable NAT plugin
    stdout, stderr, rc = run_vppctl('nat44 ei plugin enable sessions 65536 users 8192')
    if rc != 0 and 'already enabled' not in stderr:
        errors.append(f'NAT enable: {stderr}')

    # Set inside/outside interfaces
    stdout, stderr, rc = run_vppctl(f'set interface nat44 ei in {inside_if} out {outside_if}')
    if rc != 0:
        errors.append(f'Interface config: {stderr}')

    # Add interface address
    stdout, stderr, rc = run_vppctl(f'nat44 ei add interface address {outside_if}')
    if rc != 0:
        errors.append(f'Address config: {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': 'NAT enabled'}

def nat_disable():
    """Disable NAT44 EI"""
    # VPP doesn't have a direct disable command, need to remove interfaces
    stdout, stderr, rc = run_vppctl('nat44 ei plugin disable')
    if rc != 0:
        return {'error': stderr}
    return {'status': 'ok', 'message': 'NAT disabled'}

def nat_show():
    """Show NAT status"""
    result = {'interfaces': [], 'sessions': []}

    # Get NAT interfaces
    stdout, stderr, rc = run_vppctl('show nat44 ei interfaces')
    if rc == 0 and stdout:
        for line in stdout.split('\n'):
            line = line.strip()
            if line and 'NAT44 interfaces:' not in line:
                parts = line.split()
                if len(parts) >= 2:
                    result['interfaces'].append({
                        'name': parts[0],
                        'direction': parts[1]
                    })

    # Get NAT sessions
    stdout, stderr, rc = run_vppctl('show nat44 ei sessions')
    if rc == 0 and stdout:
        for line in stdout.split('\n')[:10]:
            line = line.strip()
            if line:
                result['sessions'].append(line)

    return result

def main():
    parser = argparse.ArgumentParser(description='VectorOS NAT Manager')
    parser.add_argument('action', choices=['enable', 'disable', 'show'])
    parser.add_argument('--inside-if', default='lan0', help='Inside interface name (LAN)')
    parser.add_argument('--outside-if', default='pppoe-wan0', help='Outside interface name (PPPoE)')

    args = parser.parse_args()

    try:
        if args.action == 'enable':
            result = nat_enable(args.inside_if, args.outside_if)
        elif args.action == 'disable':
            result = nat_disable()
        elif args.action == 'show':
            result = nat_show()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
