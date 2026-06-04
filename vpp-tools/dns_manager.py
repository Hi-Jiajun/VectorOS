#!/usr/bin/env python3
"""VectorOS DNS Manager - dnsmasq DNS forwarding"""

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

def is_dnsmasq_running():
    """Check if dnsmasq is running"""
    stdout, stderr, rc = run_cmd('pgrep dnsmasq')
    return rc == 0

def dns_enable(upstream='8.8.8.8,1.1.1.1', interface='veth-lan0', cache_size=1000):
    """Enable DNS forwarding using dnsmasq"""
    # Read existing DHCP config if any
    dhcp_config = ''
    config_path = '/etc/vectoros-dhcp.conf'
    try:
        with open(config_path, 'r') as f:
            dhcp_config = f.read()
    except FileNotFoundError:
        pass

    # Create combined config
    upstream_lines = '\n'.join([f'server={s.strip()}' for s in upstream.split(',')])
    config = f"""# VectorOS DNS Configuration
{upstream_lines}
cache-size={cache_size}
listen-address=127.0.0.1,192.168.1.1
interface={interface}
bind-dynamic
no-resolv
no-poll
log-queries
"""

    # If DHCP is configured, merge configs
    if 'dhcp-range' in dhcp_config:
        # Extract DHCP lines (skip interface line, use our own)
        dhcp_lines = [l for l in dhcp_config.split('\n()
                      if 'dhcp' in l.lower() and not l.strip().startswith('#')]
        if dhcp_lines:
            config += '\n' + '\n'.join(dhcp_lines) + '\n'

    try:
        with open(config_path, 'w') as f:
            f.write(config)
    except Exception as e:
        return {'error': f'Failed to write config: {e}'}

    # Restart dnsmasq
    run_cmd('pkill -9 dnsmasq')
    import time
    time.sleep(0.5)

    try:
        subprocess.Popen(['dnsmasq', f'--conf-file={config_path}'])
        time.sleep(1)

        if is_dnsmasq_running():
            return {'status': 'ok', 'message': 'DNS forwarding enabled'}
        else:
            return {'error': 'Failed to start dnsmasq'}
    except Exception as e:
        return {'error': f'Failed to start dnsmasq: {e}'}

def dns_disable():
    """Disable DNS forwarding"""
    run_cmd('pkill -9 dnsmasq')

    try:
        os.remove('/etc/vectoros-dns.conf')
    except:
        pass

    return {'status': 'ok', 'message': 'DNS forwarding disabled'}

def dns_show():
    """Show DNS status"""
    result = {
        'status': 'inactive',
        'upstream': ['8.8.8.8', '1.1.1.1'],
        'cache_size': 1000,
        'interface': 'veth-lan0'
    }

    if is_dnsmasq_running():
        result['status'] = 'active'

    # Read config to get actual values
    try:
        with open('/etc/vectoros-dhcp.conf', 'r') as f:
            for line in f:
                if line.startswith('server='):
                    result['upstream'] = [s.strip() for s in line.split('=', 1)[1].split(',')]
                elif line.startswith('cache-size='):
                    result['cache_size'] = int(line.split('=')[1].strip())
    except:
        pass

    return result

def main():
    parser = argparse.ArgumentParser(description='VectorOS DNS Manager')
    parser.add_argument('action', choices=['enable', 'disable', 'show'])
    parser.add_argument('--upstream', default='8.8.8.8,1.1.1.1', help='Upstream DNS servers')
    parser.add_argument('--interface', default='veth-lan0', help='Interface name')
    parser.add_argument('--cache-size', type=int, default=1000, help='DNS cache size')

    args = parser.parse_args()

    try:
        if args.action == 'enable':
            result = dns_enable(args.upstream, args.interface, args.cache_size)
        elif args.action == 'disable':
            result = dns_disable()
        elif args.action == 'show':
            result = dns_show()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
