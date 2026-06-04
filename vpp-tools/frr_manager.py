#!/usr/bin/env python3
"""VectorOS FRRouting Manager - Manages FRRouting via vtysh and FPM socket"""

import sys
import json
import argparse
import subprocess
import socket
import struct
import os
import signal


def run_vtysh(cmd):
    """Run a vtysh command and return the output"""
    try:
        result = subprocess.run(
            ['vtysh', '-c', cmd],
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except FileNotFoundError:
        return '', 'vtysh not found - is FRRouting installed?', 1
    except Exception as e:
        return '', str(e), 1


def is_frr_running():
    """Check if FRRouting daemons are running"""
    stdout, stderr, rc = run_vtysh('show version')
    if rc == 0 and stdout:
        return True
    return False


def get_daemon_status():
    """Get status of individual FRR daemons"""
    daemons = {}
    stdout, stderr, rc = run_vtysh('show daemons')
    if rc == 0 and stdout:
        for line in stdout.split('\n'):
            line = line.strip()
            if line and not line.startswith('Daemon'):
                parts = line.split()
                if parts:
                    daemon_name = parts[0]
                    # Detect 'running' or 'active' in the line
                    is_active = any(
                        kw in line.lower()
                        for kw in ['running', 'active', '*']
                    )
                    daemons[daemon_name] = is_active
    return daemons


def frr_enable(as_number=None):
    """Enable FRRouting - start required daemons"""
    errors = []

    # Enable zebra (always needed)
    stdout, stderr, rc = run_vtysh('conf\nzebra on\nexit')
    if rc != 0:
        errors.append(f'zebra enable: {stderr}')

    # Enable OSPF
    stdout, stderr, rc = run_vtysh('conf\nospf on\nexit')
    if rc != 0:
        errors.append(f'ospf enable: {stderr}')

    # Enable BGP if AS number is provided
    if as_number:
        as_str = str(as_number)
        stdout, stderr, rc = run_vtysh(f'conf\nrouter bgp {as_str}\nexit')
        if rc != 0:
            errors.append(f'bgp enable (AS {as_str}): {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': 'FRRouting enabled'}


def frr_disable():
    """Disable FRRouting daemons"""
    errors = []

    # Disable BGP
    stdout, stderr, rc = run_vtysh('conf\nno router bgp\nexit')
    if rc != 0 and 'no such' not in stderr.lower():
        errors.append(f'bgp disable: {stderr}')

    # Disable OSPF
    stdout, stderr, rc = run_vtysh('conf\nno ospf on\nexit')
    if rc != 0:
        errors.append(f'ospf disable: {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': 'FRRouting daemons disabled'}


def frr_show():
    """Show FRRouting status and configuration"""
    result = {
        'running': is_frr_running(),
        'daemons': get_daemon_status(),
        'bgp': {},
        'ospf': {},
        'version': ''
    }

    # Get version
    stdout, stderr, rc = run_vtysh('show version')
    if rc == 0 and stdout:
        for line in stdout.split('\n'):
            if 'FRRouting' in line or 'frr' in line.lower():
                result['version'] = line.strip()
                break
        if not result['version'] and stdout:
            result['version'] = stdout.split('\n')[0].strip()

    # Get BGP summary
    stdout, stderr, rc = run_vtysh('show bgp summary')
    if rc == 0 and stdout:
        result['bgp']['summary'] = stdout

    # Get OSPF neighbor info
    stdout, stderr, rc = run_vtysh('show ip ospf neighbor')
    if rc == 0 and stdout:
        result['ospf']['neighbors'] = stdout

    # Get OSPF routing table
    stdout, stderr, rc = run_vtysh('show ip ospf route')
    if rc == 0 and stdout:
        result['ospf']['routes'] = stdout

    return result


def frr_show_routes():
    """Show all routes from FRR"""
    result = {'routes': []}

    stdout, stderr, rc = run_vtysh('show ip route')
    if rc == 0 and stdout:
        for line in stdout.split('\n'):
            line = line.strip()
            if line and not line.startswith('Codes') and not line.startswith('K') \
                    and 'Gateway of last resort' not in line:
                parts = line.split()
                if len(parts) >= 2:
                    # Parse route line: Codes prefix via gateway ...
                    route_entry = {
                        'raw': line,
                        'protocol': '',
                        'prefix': '',
                        'nexthop': '',
                        'interface': '',
                    }
                    # Try to identify protocol code
                    if parts[0] in ('C', 'connected'):
                        route_entry['protocol'] = 'connected'
                    elif parts[0] in ('S', 'static'):
                        route_entry['protocol'] = 'static'
                    elif parts[0] in ('O', 'ospf'):
                        route_entry['protocol'] = 'ospf'
                    elif parts[0] in ('B', 'bgp'):
                        route_entry['protocol'] = 'bgp'
                    elif parts[0] in ('K', 'kernel'):
                        route_entry['protocol'] = 'kernel'

                    result['routes'].append(route_entry)

    return result


def frr_add_route(prefix, nexthop=None, interface=None, distance=None):
    """Add a static route via FRR"""
    if not nexthop and not interface:
        return {'error': 'Either nexthop or interface must be specified'}

    cmd_parts = ['conf', 'ip route']
    if distance:
        cmd_parts.append(str(distance))
    cmd_parts.append(prefix)
    if nexthop:
        cmd_parts.append(nexthop)
    if interface:
        cmd_parts.append(interface)
    cmd_parts.append('exit')

    cmd = ' '.join(cmd_parts)
    stdout, stderr, rc = run_vtysh(cmd)

    if rc != 0:
        return {'error': f'Failed to add route: {stderr}'}
    return {'status': 'ok', 'message': f'Route {prefix} added'}


def frr_del_route(prefix, nexthop=None, interface=None, distance=None):
    """Delete a static route via FRR"""
    cmd_parts = ['conf', 'no', 'ip route']
    if distance:
        cmd_parts.append(str(distance))
    cmd_parts.append(prefix)
    if nexthop:
        cmd_parts.append(nexthop)
    if interface:
        cmd_parts.append(interface)
    cmd_parts.append('exit')

    cmd = ' '.join(cmd_parts)
    stdout, stderr, rc = run_vtysh(cmd)

    if rc != 0:
        return {'error': f'Failed to delete route: {stderr}'}
    return {'status': 'ok', 'message': f'Route {prefix} deleted'}


def frr_configure_bgp(as_number, neighbor_ip=None, neighbor_as=None, networks=None):
    """Configure BGP"""
    errors = []
    as_str = str(as_number)

    # Start BGP process
    stdout, stderr, rc = run_vtysh(f'conf\nrouter bgp {as_str}\nexit')
    if rc != 0:
        return {'error': f'Failed to start BGP AS {as_str}: {stderr}'}

    # Add neighbor
    if neighbor_ip and neighbor_as:
        neighbor_as_str = str(neighbor_as)
        stdout, stderr, rc = run_vtysh(
            f'conf\nrouter bgp {as_str}\nneighbor {neighbor_ip} remote-as {neighbor_as_str}\nexit'
        )
        if rc != 0:
            errors.append(f'neighbor config: {stderr}')

    # Add networks
    if networks:
        for network in networks:
            stdout, stderr, rc = run_vtysh(
                f'conf\nrouter bgp {as_str}\nnetwork {network}\nexit'
            )
            if rc != 0:
                errors.append(f'network {network}: {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': f'BGP AS {as_str} configured'}


def frr_configure_ospf(process_id=1, networks=None, redistribute=None):
    """Configure OSPF"""
    errors = []
    pid_str = str(process_id)

    # Enable OSPF
    stdout, stderr, rc = run_vtysh(f'conf\nrouter ospf {pid_str}\nexit')
    if rc != 0:
        return {'error': f'Failed to enable OSPF: {stderr}'}

    # Add networks
    if networks:
        for net in networks:
            area = net.get('area', '0')
            prefix = net.get('prefix', net.get('network', ''))
            if prefix:
                stdout, stderr, rc = run_vtysh(
                    f'conf\nrouter ospf {pid_str}\nnetwork {prefix} area {area}\nexit'
                )
                if rc != 0:
                    errors.append(f'network {prefix}: {stderr}')

    # Redistribute connected routes
    if redistribute:
        for rtype in redistribute:
            if rtype == 'connected':
                stdout, stderr, rc = run_vtysh(
                    f'conf\nrouter ospf {pid_str}\nredistribute connected\nexit'
                )
                if rc != 0:
                    errors.append(f'redistribute connected: {stderr}')
            elif rtype == 'static':
                stdout, stderr, rc = run_vtysh(
                    f'conf\nrouter ospf {pid_str}\nredistribute static\nexit'
                )
                if rc != 0:
                    errors.append(f'redistribute static: {stderr}')

    if errors:
        return {'error': '; '.join(errors)}
    return {'status': 'ok', 'message': f'OSPF process {pid_str} configured'}


def main():
    parser = argparse.ArgumentParser(description='VectorOS FRRouting Manager')
    parser.add_argument('action', choices=[
        'enable', 'disable', 'show', 'routes',
        'add-route', 'del-route',
        'configure-bgp', 'configure-ospf'
    ])
    parser.add_argument('--as-number', type=int, help='BGP AS number')
    parser.add_argument('--prefix', help='Route prefix (e.g. 10.0.0.0/8)')
    parser.add_argument('--nexthop', help='Next hop IP address')
    parser.add_argument('--interface', help='Outgoing interface')
    parser.add_argument('--distance', type=int, help='Route distance/preference')
    parser.add_argument('--neighbor-ip', help='BGP neighbor IP')
    parser.add_argument('--neighbor-as', type=int, help='BGP neighbor AS number')
    parser.add_argument('--network', action='append', help='Network to advertise (BGP)')
    parser.add_argument('--ospf-process', type=int, default=1, help='OSPF process ID')
    parser.add_argument('--ospf-network', action='append',
                        help='OSPF network in format prefix:area (e.g. 192.168.1.0/24:0)')
    parser.add_argument('--redistribute', action='append',
                        help='Routes to redistribute (connected, static)')

    args = parser.parse_args()

    try:
        if args.action == 'enable':
            result = frr_enable(args.as_number)
        elif args.action == 'disable':
            result = frr_disable()
        elif args.action == 'show':
            result = frr_show()
        elif args.action == 'routes':
            result = frr_show_routes()
        elif args.action == 'add-route':
            if not args.prefix:
                result = {'error': '--prefix is required'}
            else:
                result = frr_add_route(args.prefix, args.nexthop, args.interface, args.distance)
        elif args.action == 'del-route':
            if not args.prefix:
                result = {'error': '--prefix is required'}
            else:
                result = frr_del_route(args.prefix, args.nexthop, args.interface, args.distance)
        elif args.action == 'configure-bgp':
            if not args.as_number:
                result = {'error': '--as-number is required'}
            else:
                networks = args.network or []
                result = frr_configure_bgp(
                    args.as_number, args.neighbor_ip,
                    args.neighbor_as, networks
                )
        elif args.action == 'configure-ospf':
            ospf_networks = []
            if args.ospf_network:
                for net_str in args.ospf_network:
                    parts = net_str.split(':')
                    if len(parts) == 2:
                        ospf_networks.append({'prefix': parts[0], 'area': parts[1]})
                    else:
                        ospf_networks.append({'prefix': net_str, 'area': '0'})
            result = frr_configure_ospf(
                args.ospf_process, ospf_networks, args.redistribute
            )

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)


if __name__ == '__main__':
    main()
