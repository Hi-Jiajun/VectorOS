#!/usr/bin/env python3
"""VectorOS IPv6 Manager - IPv6 address, NDP, and routing management via VPP"""

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


def ipv6_set_addr(interface, address):
    """Set an IPv6 address on an interface

    Uses VPP CLI: set interface ip6 address <iface> <addr>/<prefix>
    """
    # Validate that address contains a prefix (e.g. 2001:db8::1/64)
    if '/' not in address:
        return {'error': 'IPv6 address must include prefix length (e.g. 2001:db8::1/64)'}

    stdout, stderr, rc = run_vppctl(f'set interface ip6 address {interface} {address}')
    if rc != 0:
        return {'error': stderr or 'Failed to set IPv6 address'}

    return {'status': 'ok', 'message': f'IPv6 address {address} set on {interface}'}


def ipv6_del_addr(interface, address):
    """Remove an IPv6 address from an interface"""
    if '/' not in address:
        return {'error': 'IPv6 address must include prefix length'}

    stdout, stderr, rc = run_vppctl(f'set interface ip6 address {interface} {address}')
    if rc != 0:
        # Try the delete command
        stdout, stderr, rc = run_vppctl(f'del interface ip6 address {interface} {address}')
        if rc != 0:
            return {'error': stderr or 'Failed to remove IPv6 address'}

    return {'status': 'ok', 'message': f'IPv6 address {address} removed from {interface}'}


def ipv6_show():
    """Show IPv6 addresses on all interfaces"""
    result = {'interfaces': []}

    stdout, stderr, rc = run_vppctl('show interface addr')
    if rc != 0:
        return {'error': stderr}

    current_iface = None
    for line in stdout.split('\n'):
        line = line.strip()
        if not line:
            continue
        # Interface lines don't start with spaces and contain state info
        if not line.startswith(' ') and ('up' in line.lower() or 'down' in line.lower()):
            parts = line.split()
            if parts:
                current_iface = {
                    'name': parts[0],
                    'ipv6_addresses': []
                }
                result['interfaces'].append(current_iface)
        elif current_iface and ':' in line and '/' in line:
            # This looks like an IPv6 address line
            addr = line.split()[0] if line.split() else line
            if ':' in addr:
                current_iface['ipv6_addresses'].append(addr)

    return result


def ipv6_show_ndp():
    """Show IPv6 neighbor discovery table"""
    result = {'neighbors': []}

    stdout, stderr, rc = run_vppctl('show ip6 neighbors')
    if rc != 0:
        return {'error': stderr}

    for line in stdout.split('\n'):
        line = line.strip()
        if not line or 'IP6' in line or '---' in line or 'IPv6' in line:
            continue
        parts = line.split()
        if len(parts) >= 4 and ':' in parts[0]:
            result['neighbors'].append({
                'ipv6': parts[0],
                'mac': parts[1] if len(parts) > 1 else '',
                'interface': parts[2] if len(parts) > 2 else '',
                'flags': parts[3] if len(parts) > 3 else ''
            })

    return result


def ipv6_enable_ndp(interface):
    """Enable IPv6 neighbor discovery on an interface"""
    # Ensure IPv6 is enabled on the interface first
    stdout, stderr, rc = run_vppctl(f'set interface ip6 enable {interface}')
    if rc != 0 and 'already' not in (stderr or ''):
        return {'error': stderr or 'Failed to enable IPv6 on interface'}

    return {'status': 'ok', 'message': f'IPv6 NDP enabled on {interface}'}


def ipv6_show_routes():
    """Show IPv6 routing table (FIB)"""
    result = {'routes': []}

    stdout, stderr, rc = run_vppctl('show ip6 fib')
    if rc != 0:
        return {'error': stderr}

    for line in stdout.split('\n'):
        line = line.strip()
        if not line or 'IP6' in line or '---' in line:
            continue
        parts = line.split()
        if len(parts) >= 2 and ':' in parts[0]:
            result['routes'].append({
                'destination': parts[0],
                'next_hop': parts[1] if len(parts) > 1 else '',
                'details': ' '.join(parts[2:]) if len(parts) > 2 else ''
            })

    return result


def ipv6_add_route(destination, next_hop):
    """Add a static IPv6 route"""
    stdout, stderr, rc = run_vppctl(f'ip route add {destination} via {next_hop}')
    if rc != 0:
        return {'error': stderr or 'Failed to add IPv6 route'}
    return {'status': 'ok', 'message': f'Route {destination} via {next_hop} added'}


def ipv6_del_route(destination, next_hop):
    """Delete a static IPv6 route"""
    stdout, stderr, rc = run_vppctl(f'ip route del {destination} via {next_hop}')
    if rc != 0:
        return {'error': stderr or 'Failed to delete IPv6 route'}
    return {'status': 'ok', 'message': f'Route {destination} via {next_hop} removed'}


def main():
    parser = argparse.ArgumentParser(description='VectorOS IPv6 Manager')
    subparsers = parser.add_subparsers(dest='action', help='Action to perform')

    # set-addr
    set_addr = subparsers.add_parser('set-addr', help='Set IPv6 address on interface')
    set_addr.add_argument('--interface', required=True, help='Interface name')
    set_addr.add_argument('--address', required=True, help='IPv6 address with prefix (e.g. 2001:db8::1/64)')

    # del-addr
    del_addr = subparsers.add_parser('del-addr', help='Remove IPv6 address from interface')
    del_addr.add_argument('--interface', required=True, help='Interface name')
    del_addr.add_argument('--address', required=True, help='IPv6 address with prefix')

    # show
    subparsers.add_parser('show', help='Show IPv6 addresses on all interfaces')

    # enable-ndp
    enable_ndp = subparsers.add_parser('enable-ndp', help='Enable IPv6 NDP on interface')
    enable_ndp.add_argument('--interface', required=True, help='Interface name')

    # show-ndp
    subparsers.add_parser('show-ndp', help='Show IPv6 neighbor discovery table')

    # show-routes
    subparsers.add_parser('show-routes', help='Show IPv6 routing table')

    # add-route
    add_route = subparsers.add_parser('add-route', help='Add static IPv6 route')
    add_route.add_argument('--destination', required=True, help='Destination prefix')
    add_route.add_argument('--next-hop', required=True, help='Next hop address')

    # del-route
    del_route = subparsers.add_parser('del-route', help='Delete static IPv6 route')
    del_route.add_argument('--destination', required=True, help='Destination prefix')
    del_route.add_argument('--next-hop', required=True, help='Next hop address')

    args = parser.parse_args()

    if not args.action:
        parser.print_help()
        sys.exit(1)

    try:
        if args.action == 'set-addr':
            result = ipv6_set_addr(args.interface, args.address)
        elif args.action == 'del-addr':
            result = ipv6_del_addr(args.interface, args.address)
        elif args.action == 'show':
            result = ipv6_show()
        elif args.action == 'enable-ndp':
            result = ipv6_enable_ndp(args.interface)
        elif args.action == 'show-ndp':
            result = ipv6_show_ndp()
        elif args.action == 'show-routes':
            result = ipv6_show_routes()
        elif args.action == 'add-route':
            result = ipv6_add_route(args.destination, args.next_hop)
        elif args.action == 'del-route':
            result = ipv6_del_route(args.destination, args.next_hop)

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)


if __name__ == '__main__':
    main()
