#!/usr/bin/env python3
"""VectorOS VPP ACL Manager - Direct VPP ACL plugin management"""

import sys
import json
import subprocess

def run_vppctl(cmd):
    """Run vppctl command"""
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

def add_acl(action, src='0.0.0.0/0', dst='0.0.0.0/0', proto=0, sport='0-65535', dport='0-65535', tag='vectoros'):
    """Add ACL rule"""
    cmd = f'set acl-plugin acl {action} src {src} dst {dst} proto {proto} sport {sport} dport {dport} tag {tag}'
    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to add ACL'}

    # Parse ACL index from output
    try:
        idx = int(stdout.split(':')[1].strip())
        return {'status': 'ok', 'acl_index': idx, 'message': f'ACL rule added (index: {idx})'}
    except:
        return {'status': 'ok', 'message': 'ACL rule added'}

def delete_acl(index):
    """Delete ACL rule by index"""
    cmd = f'acl-plugin delete acl {index}'
    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to delete ACL'}
    return {'status': 'ok', 'message': f'ACL rule {index} deleted'}

def show_acls():
    """Show all ACL rules"""
    stdout, stderr, rc = run_vppctl('show acl-plugin acl')
    if rc != 0:
        return {'error': stderr or 'Failed to show ACLs'}

    rules = []
    current_rule = None

    for line in stdout.split('\n'):
        line = line.strip()
        if line.startswith('acl-index'):
            if current_rule:
                rules.append(current_rule)
            parts = line.split()
            current_rule = {
                'index': int(parts[1]),
                'count': int(parts[3].replace('count', '')),
                'tag': parts[5] if len(parts) > 5 else '',
                'rules': []
            }
        elif line and current_rule and ':' in line:
            # Parse rule line: "0: ipv4 permit src 0.0.0.0/0 dst 0.0.0.0/0 ..."
            rule_parts = line.split(': ', 1)
            if len(rule_parts) == 2:
                rule = rule_parts[1]
                current_rule['rules'].append(rule)

    if current_rule:
        rules.append(current_rule)

    return {'rules': rules, 'count': len(rules)}

def apply_acl_to_interface(interface, acl_index, input_acl=True):
    """Apply ACL to interface"""
    direction = 'input' if input_acl else 'output'
    cmd = f'set acl-plugin interface {interface} {acl_index} {direction}'
    stdout, stderr, rc = run_vppctl(cmd)
    if rc != 0:
        return {'error': stderr or 'Failed to apply ACL to interface'}
    return {'status': 'ok', 'message': f'ACL {acl_index} applied to {interface} {direction}'}

def show_interface_acls():
    """Show ACLs applied to interfaces"""
    stdout, stderr, rc = run_vppctl('show acl-plugin interface')
    if rc != 0:
        return {'error': stderr or 'Failed to show interface ACLs'}

    interfaces = []
    current_iface = None

    for line in stdout.split('\n'):
        line = line.strip()
        if line and not line.startswith('acl-index') and not line.startswith('input') and not line.startswith('output'):
            if ' ' not in line and line.endswith(':'):
                current_iface = line[:-1]
                interfaces.append({'name': current_iface, 'input_acls': [], 'output_acls': []})
            elif current_iface and line.startswith('input'):
                # Parse input ACLs
                parts = line.split()
                if len(parts) > 1:
                    interfaces[-1]['input_acls'] = [int(x) for x in parts[1:]]
            elif current_iface and line.startswith('output'):
                # Parse output ACLs
                parts = line.split()
                if len(parts) > 1:
                    interfaces[-1]['output_acls'] = [int(x) for x in parts[1:]]

    return {'interfaces': interfaces}

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS VPP ACL Manager')
    parser.add_argument('action', choices=['add', 'delete', 'show', 'apply', 'interfaces'])
    parser.add_argument('--action-type', choices=['permit', 'deny'], default='deny')
    parser.add_argument('--src', default='0.0.0.0/0')
    parser.add_argument('--dst', default='0.0.0.0/0')
    parser.add_argument('--proto', type=int, default=0)
    parser.add_argument('--sport', default='0-65535')
    parser.add_argument('--dport', default='0-65535')
    parser.add_argument('--tag', default='vectoros')
    parser.add_argument('--index', type=int)
    parser.add_argument('--interface')
    parser.add_argument('--direction', choices=['input', 'output'], default='input')

    args = parser.parse_args()

    try:
        if args.action == 'add':
            result = add_acl(args.action_type, args.src, args.dst, args.proto, args.sport, args.dport, args.tag)
        elif args.action == 'delete':
            result = delete_acl(args.index)
        elif args.action == 'show':
            result = show_acls()
        elif args.action == 'apply':
            result = apply_acl_to_interface(args.interface, args.index, args.direction == 'input')
        elif args.action == 'interfaces':
            result = show_interface_acls()

        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
