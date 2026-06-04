#!/usr/bin/env python3
"""VectorOS DHCP Manager - dnsmasq wrapper"""

import sys
import json
import argparse
import subprocess
import os
import signal

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

def is_dnsmasq_running():
    """Check if dnsmasq is running"""
    stdout, stderr, rc = run_cmd('pgrep dnsmasq')
    return rc == 0

def dhcp_enable(interface='veth-lan0', start_ip='192.168.1.100', end_ip='192.168.1.200',
                gateway='192.168.1.1', lease_time=86400):
    """Enable DHCP server using dnsmasq"""
    # Kill existing dnsmasq
    run_cmd('pkill -9 dnsmasq')

    # Create config file
    config = f"""interface={interface}
bind-dynamic
dhcp-range={start_ip},{end_ip},{lease_time}s
dhcp-option=option:router,{gateway}
dhcp-option=option:dns-server,8.8.8.8,1.1.1.1
log-dhcp
"""

    config_path = '/etc/vectoros-dhcp.conf'
    try:
        with open(config_path, 'w') as f:
            f.write(config)
    except Exception as e:
        return {'error': f'Failed to write config: {e}'}

    # Start dnsmasq
    try:
        subprocess.Popen(['dnsmasq', f'--conf-file={config_path}'])
        import time
        time.sleep(1)

        if is_dnsmasq_running():
            return {'status': 'ok', 'message': 'DHCP server enabled'}
        else:
            return {'error': 'Failed to start dnsmasq'}
    except Exception as e:
        return {'error': f'Failed to start dnsmasq: {e}'}

def dhcp_disable():
    """Disable DHCP server"""
    run_cmd('pkill -9 dnsmasq')

    # Remove config
    try:
        os.remove('/etc/vectoros-dhcp.conf')
    except:
        pass

    return {'status': 'ok', 'message': 'DHCP server disabled'}

def dhcp_show():
    """Show DHCP server status and leases"""
    result = {'status': 'inactive', 'leases': []}

    # Check if dnsmasq is running
    if is_dnsmasq_running():
        result['status'] = 'active'

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
    parser.add_argument('--interface', default='veth-lan0', help='Interface name')
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
