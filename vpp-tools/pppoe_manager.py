#!/usr/bin/env python3
"""VectorOS PPPoE Manager - Python wrapper for VPP API"""

import sys
import json
import argparse

sys.path.insert(0, '/usr/lib/python3/dist-packages')
from vpp_papi import VPPApiClient

def connect():
    api = VPPApiClient()
    api.connect('vectoros-pppoe')
    return api

def get_interfaces(api):
    result = api.api.sw_interface_dump()
    interfaces = []
    for iface in result:
        interfaces.append({
            'sw_if_index': iface.sw_if_index,
            'name': iface.interface_name,
            'admin_up': bool(iface.flags & 1),
            'link_up': bool(iface.flags & 2),
            'mtu': iface.mtu[0] if iface.mtu else 0,
        })
    return interfaces

def pppoe_create(api, sw_if_index, username, password, mtu=1492, mru=1492,
                 use_peer_dns=True, add_default_route4=True, add_default_route6=True):
    # Create PPPoE client
    result = api.api.pppoeclient_add_del(
        is_add=True,
        sw_if_index=sw_if_index,
        host_uniq=1,
        configured_ac_name='',
        service_name='',
        custom_ifname='pppoe-wan0'
    )
    if result.retval != 0:
        return {'error': f'Failed to create PPPoE client: {result.retval}'}

    pppox_sw_if_index = result.pppox_sw_if_index

    # Set options
    result = api.api.pppoeclient_set_options(
        pppoeclient_index=0,
        username=username,
        password=password,
        use_peer_dns=use_peer_dns,
        add_default_route4=add_default_route4,
        add_default_route6=add_default_route6,
        mtu=mtu,
        mru=mru,
        timeout=10,
        set_use_peer_dns=True,
        set_add_default_route4=True,
        set_add_default_route6=True,
        configured_ac_name='',
        service_name='',
        clear_ac_name=False,
        clear_service_name=False
    )
    if result.retval != 0:
        return {'error': f'Failed to set PPPoE options: {result.retval}'}

    # Start session (OPEN action)
    result = api.api.pppoeclient_session_action(
        pppoeclient_index=0,
        action=3  # OPEN
    )
    if result.retval != 0:
        return {'error': f'Failed to start PPPoE session: {result.retval}'}

    return {
        'status': 'ok',
        'pppox_sw_if_index': pppox_sw_if_index,
        'message': 'PPPoE client created and session started'
    }

def pppoe_dump(api):
    result = api.api.pppoeclient_dump(sw_if_index=0xFFFFFFFF)  # dump all
    clients = []
    for client in result:
        clients.append({
            'sw_if_index': client.sw_if_index,
            'pppox_sw_if_index': client.pppox_sw_if_index,
            'session_id': client.session_id,
            'client_state': client.client_state,
            'ipv4_local': str(client.ipv4_local),
            'ipv4_peer': str(client.ipv4_peer),
            'auth_user': client.auth_user,
            'mtu': client.mtu,
            'mru': client.mru,
            'use_peer_dns': client.use_peer_dns,
            'add_default_route4': client.add_default_route4,
            'add_default_route6': client.add_default_route6,
            'session_uptime_seconds': client.session_uptime_seconds,
            'total_reconnects': client.total_reconnects,
        })
    return clients

def main():
    parser = argparse.ArgumentParser(description='VectorOS PPPoE Manager')
    parser.add_argument('action', choices=['interfaces', 'create', 'dump', 'status'])
    parser.add_argument('--sw-if-index', type=int, default=1)
    parser.add_argument('--username', default='')
    parser.add_argument('--password', default='')
    parser.add_argument('--mtu', type=int, default=1492)
    parser.add_argument('--mru', type=int, default=1492)
    parser.add_argument('--use-peer-dns', action='store_true', default=True)
    parser.add_argument('--add-default-route4', action='store_true', default=True)
    parser.add_argument('--add-default-route6', action='store_true', default=True)

    args = parser.parse_args()

    try:
        api = connect()

        if args.action == 'interfaces':
            result = get_interfaces(api)
        elif args.action == 'create':
            result = pppoe_create(
                api, args.sw_if_index, args.username, args.password,
                args.mtu, args.mru, args.use_peer_dns,
                args.add_default_route4, args.add_default_route6
            )
        elif args.action == 'dump':
            result = pppoe_dump(api)
        elif args.action == 'status':
            clients = pppoe_dump(api)
            result = {
                'pppoe_clients': clients,
                'interfaces': get_interfaces(api)
            }

        api.disconnect()
        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
