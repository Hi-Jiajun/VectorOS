#!/usr/bin/env python3
"""VectorOS DHCPv6 Manager - dnsmasq DHCPv6 server management

Supports:
- IA_NA (Identity Association for Non-temporary Addresses) - address assignment
- IA_PD (Identity Association for Prefix Delegation) - prefix delegation
"""

import sys
import json
import argparse
import subprocess
import os
import time


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


def dhcpv6_enable(interface='veth-lan0', range_start='2001:db8:1::100',
                   range_end='2001:db8:1::200', prefix='2001:db8:1::/64',
                   gateway='2001:db8:1::1', lease_time=86400,
                   enable_ia_na=True, enable_ia_pd=False,
                   pd_prefix='2001:db8:2::/48'):
    """Enable DHCPv6 server using dnsmasq

    dnsmasq DHCPv6 options:
    - dhcp-range: sets the range for IA_NA address assignment
    - dhcp-option=option6:dns-server: sets DNS servers for DHCPv6 clients
    - dhcp-option=option6:prefix-delegation: enables IA_PD prefix delegation
    """
    # Read existing DHCPv4 config if any
    dhcpv4_config = ''
    config_path = '/etc/vectoros-dhcp.conf'
    try:
        with open(config_path, 'r') as f:
            dhcpv4_config = f.read()
    except FileNotFoundError:
        pass

    # Build DHCPv6 config lines
    dhcpv6_lines = []

    # Enable DHCPv6 on the interface
    dhcpv6_lines.append(f'dhcp-range={interface},{range_start},{range_end},{lease_time}s')

    # IPv6 gateway (router option)
    dhcpv6_lines.append(f'dhcp-option=option6:rap-con,{gateway}')

    # DNS servers
    dhcpv6_lines.append('dhcp-option=option6:dns-server,2001:4860:4860::8888,2606:4700:4700::1111')

    # Enable prefix delegation if requested
    if enable_ia_pd:
        dhcpv6_lines.append(f'dhcp-option=option6:prefix-delegation,64,{pd_prefix}')

    # Read the DHCPv6 config file path
    dhcpv6_config_path = '/etc/vectoros-dhcpv6.conf'
    config_content = '# VectorOS DHCPv6 Configuration\n'
    config_content += '\n'.join(dhcpv6_lines) + '\n'

    try:
        with open(dhcpv6_config_path, 'w') as f:
            f.write(config_content)
    except Exception as e:
        return {'error': f'Failed to write DHCPv6 config: {e}'}

    # Merge with DHCPv4 config if running
    if is_dnsmasq_running():
        run_cmd('pkill -9 dnsmasq')
        time.sleep(0.5)

    # Start dnsmasq with combined config
    try:
        cmd = ['dnsmasq', f'--conf-file={dhcpv6_config_path}']
        subprocess.Popen(cmd)
        time.sleep(1)

        if is_dnsmasq_running():
            result = {
                'status': 'ok',
                'message': 'DHCPv6 server enabled',
                'ia_na': {
                    'enabled': enable_ia_na,
                    'range': f'{range_start} - {range_end}',
                    'interface': interface
                }
            }
            if enable_ia_pd:
                result['ia_pd'] = {
                    'enabled': True,
                    'prefix': pd_prefix,
                    'delegation_length': 64
                }
            return result
        else:
            return {'error': 'Failed to start dnsmasq for DHCPv6'}
    except Exception as e:
        return {'error': f'Failed to start DHCPv6 server: {e}'}


def dhcpv6_disable():
    """Disable DHCPv6 server"""
    run_cmd('pkill -9 dnsmasq')

    # Remove DHCPv6 config
    try:
        os.remove('/etc/vectoros-dhcpv6.conf')
    except FileNotFoundError:
        pass

    return {'status': 'ok', 'message': 'DHCPv6 server disabled'}


def dhcpv6_show():
    """Show DHCPv6 server status and leases"""
    result = {
        'status': 'inactive',
        'ia_na': {'enabled': False, 'leases': []},
        'ia_pd': {'enabled': False, 'leases': []}
    }

    if is_dnsmasq_running():
        result['status'] = 'active'

    # Read DHCPv6 leases from dnsmasq
    # dnsmasq stores DHCPv6 leases in /var/lib/misc/dnsmasq.leases
    # or /var/lib/dhcp/dhclient6.leases depending on version
    lease_files = [
        '/var/lib/misc/dnsmasq.leases',
        '/var/lib/dhcp/dhcpv6.leases',
        '/var/lib/dhcpd/dhcpv6.leases'
    ]

    for lease_file in lease_files:
        try:
            with open(lease_file, 'r') as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    parts = line.split()
                    if len(parts) >= 4:
                        # dnsmasq lease format: expiry mac address hostname
                        entry = {
                            'expires': parts[0],
                            'mac': parts[1],
                            'address': parts[2],
                            'hostname': parts[3] if len(parts) > 3 else ''
                        }
                        # Determine if this is IA_NA (single address) or IA_PD (prefix)
                        if ':' in entry['address']:
                            # Check if it's a prefix (shorter than /128)
                            result['ia_na']['leases'].append(entry)
            break  # Only read the first found lease file
        except FileNotFoundError:
            continue

    # Read config for prefix delegation info
    try:
        with open('/etc/vectoros-dhcpv6.conf', 'r') as f:
            for line in f:
                if 'prefix-delegation' in line:
                    result['ia_pd']['enabled'] = True
    except FileNotFoundError:
        pass

    return result


def main():
    parser = argparse.ArgumentParser(description='VectorOS DHCPv6 Manager')
    subparsers = parser.add_subparsers(dest='action', help='Action to perform')

    # enable
    enable_parser = subparsers.add_parser('enable', help='Enable DHCPv6 server')
    enable_parser.add_argument('--interface', default='veth-lan0',
                                help='Interface name (default: veth-lan0)')
    enable_parser.add_argument('--range-start', default='2001:db8:1::100',
                                help='IA_NA range start (default: 2001:db8:1::100)')
    enable_parser.add_argument('--range-end', default='2001:db8:1::200',
                                help='IA_NA range end (default: 2001:db8:1::200)')
    enable_parser.add_argument('--prefix', default='2001:db8:1::/64',
                                help='Network prefix (default: 2001:db8:1::/64)')
    enable_parser.add_argument('--gateway', default='2001:db8:1::1',
                                help='Gateway address (default: 2001:db8:1::1)')
    enable_parser.add_argument('--lease-time', type=int, default=86400,
                                help='Lease time in seconds (default: 86400)')
    enable_parser.add_argument('--enable-ia-pd', action='store_true',
                                help='Enable prefix delegation (IA_PD)')
    enable_parser.add_argument('--pd-prefix', default='2001:db8:2::/48',
                                help='Prefix delegation pool (default: 2001:db8:2::/48)')

    # disable
    subparsers.add_parser('disable', help='Disable DHCPv6 server')

    # show
    subparsers.add_parser('show', help='Show DHCPv6 status and leases')

    args = parser.parse_args()

    if not args.action:
        parser.print_help()
        sys.exit(1)

    try:
        if args.action == 'enable':
            result = dhcpv6_enable(
                interface=args.interface,
                range_start=args.range_start,
                range_end=args.range_end,
                prefix=args.prefix,
                gateway=args.gateway,
                lease_time=args.lease_time,
                enable_ia_pd=args.enable_ia_pd,
                pd_prefix=args.pd_prefix
            )
        elif args.action == 'disable':
            result = dhcpv6_disable()
        elif args.action == 'show':
            result = dhcpv6_show()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)


if __name__ == '__main__':
    main()
