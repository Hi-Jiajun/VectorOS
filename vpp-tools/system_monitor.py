#!/usr/bin/env python3
"""VectorOS System Monitor"""

import sys
import json
import subprocess
import psutil

def get_system_info():
    """Get system information"""
    try:
        cpu_percent = psutil.cpu_percent(interval=1)
        memory = psutil.virtual_memory()
        disk = psutil.disk_usage('/')

        return {
            'cpu': {
                'percent': cpu_percent,
                'count': psutil.cpu_count()
            },
            'memory': {
                'total': memory.total,
                'available': memory.available,
                'used': memory.used,
                'percent': memory.percent
            },
            'disk': {
                'total': disk.total,
                'used': disk.used,
                'free': disk.free,
                'percent': disk.percent
            }
        }
    except Exception as e:
        return {'error': str(e)}

def get_vpp_stats():
    """Get VPP statistics"""
    try:
        # Get VPP version
        result = subprocess.run(['vppctl', 'show', 'version'], capture_output=True, text=True, timeout=5)
        version = result.stdout.strip() if result.returncode == 0 else 'N/A'

        # Get interface count
        result = subprocess.run(['vppctl', 'show', 'interface'], capture_output=True, text=True, timeout=5)
        iface_count = len([l for l in result.stdout.split('\n') if l.strip() and not l.startswith(' ') and 'Name' not in l]) if result.returncode == 0 else 0

        return {
            'version': version,
            'interface_count': iface_count
        }
    except Exception as e:
        return {'error': str(e)}

def get_network_stats():
    """Get network statistics"""
    try:
        net_io = psutil.net_io_counters()
        return {
            'bytes_sent': net_io.bytes_sent,
            'bytes_recv': net_io.bytes_recv,
            'packets_sent': net_io.packets_sent,
            'packets_recv': net_io.packets_recv,
            'errin': net_io.errin,
            'errout': net_io.errout,
            'dropin': net_io.dropin,
            'dropout': net_io.dropout
        }
    except Exception as e:
        return {'error': str(e)}

def main():
    try:
        result = {
            'system': get_system_info(),
            'vpp': get_vpp_stats(),
            'network': get_network_stats()
        }
        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    main()
