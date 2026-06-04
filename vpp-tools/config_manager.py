#!/usr/bin/env python3
"""VectorOS Configuration Manager"""

import sys
import json
import argparse
import os

CONFIG_FILE = '/etc/vectoros/config.json'

DEFAULT_CONFIG = {
    'pppoe': {
        'enabled': False,
        'username': '',
        'password': '',
        'interface': 'enp1s0',
        'mtu': 1492,
        'mru': 1492,
        'use_peer_dns': True,
        'add_default_route4': True,
        'add_default_route6': True
    },
    'dhcp': {
        'enabled': False,
        'interface': 'veth-lan0',
        'start_ip': '192.168.1.100',
        'end_ip': '192.168.1.200',
        'gateway': '192.168.1.1',
        'lease_time': 86400
    },
    'dns': {
        'enabled': False,
        'upstream': ['8.8.8.8', '1.1.1.1'],
        'cache_size': 1000
    },
    'nat': {
        'enabled': False,
        'inside_if': 'lan0',
        'outside_if': 'pppoe-wan0'
    },
    'interfaces': {
        'wan0': {'state': 'up', 'mtu': 1500},
        'lan0': {'state': 'up', 'mtu': 1500, 'ip': '192.168.1.1/24'},
        'lan1': {'state': 'up', 'mtu': 1500}
    }
}

def load_config():
    """Load configuration from file"""
    try:
        with open(CONFIG_FILE, 'r') as f:
            return json.load(f)
    except FileNotFoundError:
        return DEFAULT_CONFIG.copy()
    except Exception as e:
        return {'error': str(e)}

def save_config(config):
    """Save configuration to file"""
    try:
        os.makedirs(os.path.dirname(CONFIG_FILE), exist_ok=True)
        with open(CONFIG_FILE, 'w') as f:
            json.dump(config, f, indent=2)
        return {'status': 'ok', 'message': 'Configuration saved'}
    except Exception as e:
        return {'error': str(e)}

def update_config(section, key, value):
    """Update a specific configuration value"""
    config = load_config()
    if 'error' in config:
        return config

    if section not in config:
        config[section] = {}

    config[section][key] = value
    return save_config(config)

def get_config():
    """Get current configuration"""
    return load_config()

def main():
    parser = argparse.ArgumentParser(description='VectorOS Configuration Manager')
    parser.add_argument('action', choices=['get', 'set', 'reset'])
    parser.add_argument('--section', help='Configuration section')
    parser.add_argument('--key', help='Configuration key')
    parser.add_argument('--value', help='Configuration value')

    args = parser.parse_args()

    try:
        if args.action == 'get':
            result = get_config()
        elif args.action == 'set':
            if not args.section or not args.key or not args.value:
                result = {'error': 'Section, key, and value are required'}
            else:
                # Try to parse value as JSON
                try:
                    value = json.loads(args.value)
                except:
                    value = args.value
                result = update_config(args.section, args.key, value)
        elif args.action == 'reset':
            result = save_config(DEFAULT_CONFIG)

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
