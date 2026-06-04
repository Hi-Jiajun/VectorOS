#!/usr/bin/env python3
"""VectorOS DHCP Manager - dnsmasq wrapper"""

import sys
import json
import argparse
import subprocess
import os

def run_cmd(cmd):
    """Run a command"""
    try:
        result = subprocess.run(
            cmd.split(),
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except Exception as e:
        return '', str(e), 1

def dhcp_enable(interface='lan0', start_ip='192.168.1.100', end_ip='192.168.1.200',
                gateway='192.168.1.1', lease_time=86400):
    """Enable DHCP server using dnsmasq"""
    # Create dnsmasq config
    config = f"""# VectorOS DHCP Configuration
interface={interface}
bind-interfaces
dhcp-range={start_ip},{end_ip},{lease_time}s
dhcp-option=option:router,{gateway}
dhcp-option=option:dns-server,8.8.8.8,1.1.1.1
log-dhcp
"""

    # Write config
    config_path = '/etc/dnsmasq.d/vectoros.conf'
    try:
        with open(config_path, 'w') as f:
            f.write(config)
    except Exception as e:
        return {'error': f'Failed to write config: {e}'}

    # Start dnsmasq
    stdout, stderr, rc = run_cmd('systemctl restart dnsmasq')
    if rc != 0:
        # Try to start dnsmasq directly
        stdout, stderr, rc = run_cmd('dnsmasq --conf-file=/etc/dnsmasq.d/vectoros.conf')
        if rc != 0:
            return {'error': f'Failed to start dnsmasq: {stderr}'}

    return {'status': 'ok', 'message': 'DHCP server enabled'}

def dhcp_disable():
    """Disable DHCP server"""
    stdout, stderr, rc = run_cmd('systemctl stop dnsmasq')
    if rc != 0:
        return {'error': f'Failed to stop dnsmasq: {stderr}'}

    # Remove config
    try:
        os.remove('/etc/dnsmasq.d/vectoros.conf')
    except:
        pass

    return {'status': 'ok', 'message': 'DHCP server disabled'}

def dhcp_show():
    """Show DHCP server status and leases"""
    result = {'status': 'unknown', 'leases': []}

    # Check if dnsmasq is running
    stdout, stderr, rc = run_cmd('systemctl is-active dnsmasq')
    result['status'] = 'active' if rc == 0 else 'inactive'

    # Read leases file
    try:
        with open('/var/lib/misc/dnsmasq.leases', 'r') as f:
            for line in f:
                parts = line.strip().split()
                if len(parts) >= 4:
                    result['leases'].append({
                        'mac': parts[1],
                        'ip': parts[2],
                        'hostname': parts[3],
                        'expires': parts[0]
                    })
    except FileNotFoundError:
        pass

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
            result = dhcp_disable()
        elif args.action == 'show':
            result = dhcp_show()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
