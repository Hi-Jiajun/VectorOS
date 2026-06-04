#!/usr/bin/env python3
"""VectorOS DHCP Manager - Python wrapper for VPP DHCP API"""

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

def dhcp_enable(interface='lan0', start_ip='192.168.1.100', end_ip='192.168.1.200',
                gateway='192.168.1.1', lease_time=86400):
    """Enable DHCP server on interface"""
    errors = []

    # Set interface IP address if not set
    stdout, stderr, rc = run_vppctl(f'set interface ip address {interface} {gateway}/24')
    if rc != 0 and 'already' not in stderr.lower():
        errors.append(f'IP address: {stderr}')

    # Enable DHCP server
    stdout, stderr, rc = run_vppctl(f'dhcp server add-del {interface} start {start_ip} end {end_ip} gateway {gateway} lease {lease_time}')
    if rc != 0:
        errors.append(f'DHCP server: {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': 'DHCP server enabled'}

def dhcp_disable(interface='lan0'):
    """Disable DHCP server on interface"""
    stdout, stderr, rc = run_vppctl(f'dhcp server add-del {interface} del')
    if rc != 0:
        return {'error': stderr}
    return {'status': 'ok', 'message': 'DHCP server disabled'}

def dhcp_show():
    """Show DHCP server status and leases"""
    result = {'servers': [], 'leases': []}

    # Get DHCP servers
    stdout, stderr, rc = run_vppctl('show dhcp server')
    if rc == 0 and stdout:
        result['servers'] = stdout.split('\n')

    # Get DHCP leases
    stdout, stderr, rc = run_vppctl('show dhcp lease')
    if rc == 0 and stdout:
        result['leases'] = stdout.split('\n')

    return result

def main():
    parser = argparse.ArgumentParser(description='VectorOS DHCP Manager')
    parser.add_argument('action', choices=['enable', 'disable', 'show'])
    parser.add_argument('--interface', default='lan0', help='Interface name')
    parser.add_argument('--start-ip', default='192.168.1.100', help='DHCP range start')
    parser.add_argument('--end-ip', default='192.168.1.200', help='DHCP range end')
    parser.add_argument('--gateway', default='192.168.1.1', help='Gateway IP')
    parser.add_argument('--lease-time', type=int, default=86400, help='Lease time in seconds')

    args = parser.parse_args()

    try:
        if args.action == 'enable':
            result = dhcp_enable(args.interface, args.start_ip, args.end_ip,
                               args.gateway, args.lease_time)
        elif args.action == 'disable':
            result = dhcp_disable(args.interface)
        elif args.action == 'show':
            result = dhcp_show()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
