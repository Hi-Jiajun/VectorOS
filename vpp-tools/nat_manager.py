#!/usr/bin/env python3
"""VectorOS NAT Manager - Python wrapper for VPP NAT API"""

import sys
import json
import argparse

sys.path.insert(0, '/usr/lib/python3/dist-packages')
from vpp_papi import VPPApiClient

def connect():
    api = VPPApiClient()
    api.connect('vectoros-nat')
    return api

def nat_enable(api, inside_if, outside_if):
    """Enable NAT44 EI on interfaces"""
    try:
        # Enable NAT44 EI plugin
        result = api.api.nat44_ei_plugin_enable_disable(
            enable=True,
            sessions=65536,
            users=8192
        )
        print(f'NAT enable result: {result}', file=sys.stderr)

        # Set inside interface (LAN)
        result = api.api.nat44_ei_interface_add_del_feature(
            sw_if_index=inside_if,
            is_add=True,
            is_inside=True
        )
        print(f'Inside interface result: {result}', file=sys.stderr)

        # Set outside interface (PPPoE)
        result = api.api.nat44_ei_interface_add_del_feature(
            sw_if_index=outside_if,
            is_add=True,
            is_inside=False
        )
        print(f'Outside interface result: {result}', file=sys.stderr)

        # Add interface address for NAT
        result = api.api.nat44_ei_add_del_interface_addr(
            sw_if_index=outside_if,
            is_add=True
        )
        print(f'Interface addr result: {result}', file=sys.stderr)

        return {'status': 'ok', 'message': 'NAT enabled'}
    except Exception as e:
        return {'error': str(e)}

def nat_disable(api):
    """Disable NAT44 EI"""
    try:
        result = api.api.nat44_ei_plugin_enable_disable(enable=False)
        return {'status': 'ok', 'message': 'NAT disabled'}
    except Exception as e:
        return {'error': str(e)}

def nat_show(api):
    """Show NAT status"""
    try:
        interfaces = []
        try:
            result = api.api.nat44_ei_interface_dump()
            for iface in result:
                interfaces.append({
                    'sw_if_index': iface.sw_if_index,
                    'is_inside': iface.is_inside,
                })
        except Exception as e:
            print(f'Interface dump error: {e}', file=sys.stderr)

        sessions = []
        try:
            result = api.api.nat44_ei_user_session_dump()
            for session in result[:10]:
                sessions.append({
                    'src': str(session.in_src_address),
                    'dst': str(session.in_dst_address),
                    'outside_src': str(session.out_src_address),
                    'outside_dst': str(session.out_dst_address),
                })
        except Exception as e:
            print(f'Session dump error: {e}', file=sys.stderr)

        return {
            'interfaces': interfaces,
            'sessions': sessions,
            'session_count': len(sessions)
        }
    except Exception as e:
        return {'error': str(e)}

def main():
    parser = argparse.ArgumentParser(description='VectorOS NAT Manager')
    parser.add_argument('action', choices=['enable', 'disable', 'show'])
    parser.add_argument('--inside-if', type=int, default=2, help='Inside interface index (LAN)')
    parser.add_argument('--outside-if', type=int, default=4, help='Outside interface index (PPPoE)')

    args = parser.parse_args()

    try:
        api = connect()

        if args.action == 'enable':
            result = nat_enable(api, args.inside_if, args.outside_if)
        elif args.action == 'disable':
            result = nat_disable(api)
        elif args.action == 'show':
            result = nat_show(api)

        api.disconnect()
        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
