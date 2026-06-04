#!/usr/bin/env python3
"""VectorOS Comprehensive System Monitor

Collects system metrics including:
- CPU usage per core
- Memory usage (total, used, free, cached)
- Disk usage and I/O
- Network throughput per interface
- VPP performance metrics
- Process monitoring (VPP, dnsmasq, etc.)
- Temperature monitoring (if available)
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False


def get_cpu_per_core():
    """Get CPU usage per core from /proc/stat."""
    cores = []
    try:
        with open('/proc/stat') as f:
            for line in f:
                if line.startswith('cpu') and line[3] != ' ':
                    parts = line.split()
                    core_id = int(parts[0].replace('cpu', ''))
                    user = int(parts[1])
                    nice = int(parts[2])
                    system = int(parts[3])
                    idle = int(parts[4])
                    iowait = int(parts[5]) if len(parts) > 5 else 0
                    irq = int(parts[6]) if len(parts) > 6 else 0
                    softirq = int(parts[7]) if len(parts) > 7 else 0
                    steal = int(parts[8]) if len(parts) > 8 else 0

                    total = user + nice + system + idle + iowait + irq + softirq + steal
                    active = total - idle - iowait

                    cores.append({
                        'core': core_id,
                        'total': total,
                        'active': active,
                    })
    except Exception:
        pass

    # Calculate percentages using previous snapshot
    prev_file = '/tmp/vos_cpu_prev.json'
    prev_data = {}
    if os.path.exists(prev_file):
        try:
            with open(prev_file) as f:
                prev_data = json.load(f)
        except Exception:
            pass

    result = []
    current = {}
    for core in cores:
        cid = str(core['core'])
        current[cid] = {'total': core['total'], 'active': core['active']}

        if cid in prev_data:
            d_total = core['total'] - prev_data[cid].get('total', 0)
            d_active = core['active'] - prev_data[cid].get('active', 0)
            pct = (d_active / d_total * 100) if d_total > 0 else 0.0
        else:
            pct = 0.0

        result.append({
            'core': core['core'],
            'percent': round(pct, 1),
        })

    # Save snapshot
    try:
        with open(prev_file, 'w') as f:
            json.dump(current, f)
    except Exception:
        pass

    return result


def get_cpu_summary():
    """Get overall CPU usage."""
    try:
        with open('/proc/stat') as f:
            line = f.readline()
        parts = line.split()
        user = int(parts[1])
        nice = int(parts[2])
        system = int(parts[3])
        idle = int(parts[4])
        iowait = int(parts[5]) if len(parts) > 5 else 0

        total = user + nice + system + idle + iowait
        active = total - idle - iowait

        prev_file = '/tmp/vos_cpu_sum_prev.json'
        prev = {}
        if os.path.exists(prev_file):
            try:
                with open(prev_file) as f:
                    prev = json.load(f)
            except Exception:
                pass

        d_total = total - prev.get('total', 0)
        d_active = active - prev.get('active', 0)
        pct = (d_active / d_total * 100) if d_total > 0 else 0.0

        try:
            with open(prev_file, 'w') as f:
                json.dump({'total': total, 'active': active}, f)
        except Exception:
            pass

        return {
            'percent': round(pct, 1),
            'count': os.cpu_count() or 1,
        }
    except Exception as e:
        return {'percent': 0.0, 'count': 1, 'error': str(e)}


def get_memory():
    """Get memory usage from /proc/meminfo."""
    meminfo = {}
    try:
        with open('/proc/meminfo') as f:
            for line in f:
                parts = line.split()
                key = parts[0].rstrip(':')
                value = int(parts[1]) * 1024  # kB to bytes
                meminfo[key] = value
    except Exception:
        return {'total': 0, 'used': 0, 'free': 0, 'cached': 0, 'percent': 0.0}

    total = meminfo.get('MemTotal', 0)
    free = meminfo.get('MemFree', 0)
    buffers = meminfo.get('Buffers', 0)
    cached = meminfo.get('Cached', 0)
    available = meminfo.get('MemAvailable', total)
    used = total - available

    return {
        'total': total,
        'used': used,
        'free': free,
        'buffers': buffers,
        'cached': cached,
        'available': available,
        'percent': round((used / total * 100) if total > 0 else 0, 1),
    }


def get_disk_usage():
    """Get disk usage for all mounted filesystems."""
    disks = []
    try:
        output = subprocess.check_output(['df', '-B1', '--output=source,fstype,size,used,avail,pcent,target'],
                                          text=True, timeout=5)
        for line in output.strip().split('\n')[1:]:
            parts = line.split()
            if len(parts) >= 7 and not parts[0].startswith('tmpfs'):
                disks.append({
                    'device': parts[0],
                    'fstype': parts[1],
                    'total': int(parts[2]),
                    'used': int(parts[3]),
                    'available': int(parts[4]),
                    'percent': float(parts[5].rstrip('%')),
                    'mountpoint': parts[6],
                })
    except Exception:
        pass
    return disks


def get_disk_io():
    """Get disk I/O statistics."""
    prev_file = '/tmp/vos_diskio_prev.json'
    prev = {}
    if os.path.exists(prev_file):
        try:
            with open(prev_file) as f:
                prev = json.load(f)
        except Exception:
            pass

    result = {'total_read_bytes_per_sec': 0, 'total_write_bytes_per_sec': 0, 'devices': []}

    try:
        with open('/proc/diskstats') as f:
            now = time.time()
            for line in f:
                parts = line.split()
                if len(parts) < 14:
                    continue
                dev_name = parts[2]
                # Skip partitions (only whole disks like sda, vda, nvme0n1)
                if re.search(r'[0-9]$', dev_name) and not dev_name.startswith('nvme'):
                    continue
                reads = int(parts[3]) * 512
                writes = int(parts[7]) * 512

                prev_dev = prev.get(dev_name, {})
                dt = now - prev.get('_timestamp', now)
                if dt > 0 and prev_dev:
                    read_rate = (reads - prev_dev.get('reads', reads)) / dt
                    write_rate = (writes - prev_dev.get('writes', writes)) / dt
                else:
                    read_rate = 0
                    write_rate = 0

                result['devices'].append({
                    'name': dev_name,
                    'read_bytes_per_sec': round(read_rate),
                    'write_bytes_per_sec': round(write_rate),
                })
                result['total_read_bytes_per_sec'] += round(read_rate)
                result['total_write_bytes_per_sec'] += round(write_rate)

                prev[dev_name] = {'reads': reads, 'writes': writes}

            prev['_timestamp'] = now
    except Exception:
        pass

    try:
        with open(prev_file, 'w') as f:
            json.dump(prev, f)
    except Exception:
        pass

    return result


def get_network_per_interface():
    """Get network throughput per interface from /proc/net/dev."""
    prev_file = '/tmp/vos_net_prev.json'
    prev = {}
    if os.path.exists(prev_file):
        try:
            with open(prev_file) as f:
                prev = json.load(f)
        except Exception:
            pass

    interfaces = []
    now = time.time()

    try:
        with open('/proc/net/dev') as f:
            for line in f:
                if ':' not in line:
                    continue
                iface, data = line.split(':', 1)
                iface = iface.strip()
                if iface == 'lo':
                    continue
                parts = data.split()
                rx_bytes = int(parts[0])
                tx_bytes = int(parts[8])
                rx_packets = int(parts[1])
                tx_packets = int(parts[9])
                rx_errors = int(parts[2])
                tx_errors = int(parts[10])
                rx_drops = int(parts[3])
                tx_drops = int(parts[11])

                prev_iface = prev.get(iface, {})
                dt = now - prev.get('_timestamp', now)
                if dt > 0 and prev_iface:
                    rx_bps = (rx_bytes - prev_iface.get('rx_bytes', rx_bytes)) * 8 / dt
                    tx_bps = (tx_bytes - prev_iface.get('tx_bytes', tx_bytes)) * 8 / dt
                    rx_pps = (rx_packets - prev_iface.get('rx_packets', rx_packets)) / dt
                    tx_pps = (tx_packets - prev_iface.get('tx_packets', tx_packets)) / dt
                else:
                    rx_bps = tx_bps = rx_pps = tx_pps = 0

                # Check if interface is up
                operstate = 'unknown'
                try:
                    with open(f'/sys/class/net/{iface}/operstate') as sf:
                        operstate = sf.read().strip()
                except Exception:
                    pass

                interfaces.append({
                    'name': iface,
                    'state': operstate,
                    'rx_bytes': rx_bytes,
                    'tx_bytes': tx_bytes,
                    'rx_packets': rx_packets,
                    'tx_packets': tx_packets,
                    'rx_errors': rx_errors,
                    'tx_errors': tx_errors,
                    'rx_drops': rx_drops,
                    'tx_drops': tx_drops,
                    'rx_bps': round(rx_bps),
                    'tx_bps': round(tx_bps),
                    'rx_pps': round(rx_pps),
                    'tx_pps': round(tx_pps),
                })

        prev['_timestamp'] = now
        for iface_data in interfaces:
            prev[iface_data['name']] = {
                'rx_bytes': iface_data['rx_bytes'],
                'tx_bytes': iface_data['tx_bytes'],
                'rx_packets': iface_data['rx_packets'],
                'tx_packets': iface_data['tx_packets'],
            }
    except Exception:
        pass

    try:
        with open(prev_file, 'w') as f:
            json.dump(prev, f)
    except Exception:
        pass

    return interfaces


def get_vpp_metrics():
    """Get VPP-specific metrics via vppctl."""

    def run_vppctl(args, timeout=5):
        try:
            result = subprocess.run(
                ['vppctl'] + args,
                capture_output=True, text=True, timeout=timeout,
            )
            return result.stdout.strip(), result.returncode
        except Exception:
            return '', 1

    # VPP version
    version, rc = run_vppctl(['show', 'version'])
    if rc != 0:
        return {'available': False}

    # Interface stats
    iface_output, _ = run_vppctl(['show', 'interface'])
    interfaces = []
    current_iface = None
    for line in iface_output.split('\n'):
        if not line or line.startswith(' ') or 'Name' in line:
            if current_iface and line.strip():
                parts = line.strip().split()
                if len(parts) >= 2:
                    counter = parts[0]
                    val = int(parts[-1]) if parts[-1].isdigit() else 0
                    if counter == 'rx' and 'packets' in parts:
                        current_iface['rx_packets'] = val
                    elif counter == 'rx' and 'bytes' in parts:
                        current_iface['rx_bytes'] = val
                    elif counter == 'tx' and 'packets' in parts:
                        current_iface['tx_packets'] = val
                    elif counter == 'tx' and 'bytes' in parts:
                        current_iface['tx_bytes'] = val
                    elif counter == 'drops':
                        current_iface['drops'] = val
            continue
        parts = line.split()
        if len(parts) >= 4:
            try:
                idx = int(parts[1])
                current_iface = {
                    'name': parts[0],
                    'sw_if_index': idx,
                    'state': parts[2],
                    'rx_packets': 0,
                    'tx_packets': 0,
                    'rx_bytes': 0,
                    'tx_bytes': 0,
                    'drops': 0,
                }
                interfaces.append(current_iface)
            except ValueError:
                current_iface = None

    # NAT sessions
    nat_output, _ = run_vppctl(['show', 'nat44', 'ei', 'sessions'])
    nat_sessions = 0
    for line in nat_output.split('\n'):
        if 'sessions' in line.lower():
            parts = line.split()
            for i, p in enumerate(parts):
                if p == 'sessions' and i > 0:
                    try:
                        nat_sessions += int(parts[i - 1])
                    except ValueError:
                        pass

    # PPPoE status
    pppoe_output, _ = run_vppctl(['show', 'pppoe', 'client'])
    pppoe = {'active': 0, 'discovery': 0, 'total': 0}
    for line in pppoe_output.split('\n'):
        if 'sw-if-index' in line:
            pppoe['total'] += 1
            if 'SESSION' in line:
                pppoe['active'] += 1
            elif 'DISCOVERY' in line:
                pppoe['discovery'] += 1

    # VPP memory
    mem_output, _ = run_vppctl(['show', 'memory'])
    vpp_mem = {'total_mb': 0, 'used_mb': 0, 'free_mb': 0, 'percent': 0}
    for line in mem_output.split('\n'):
        if 'total:' in line and 'used:' in line and 'free:' in line:
            parts = line.split()
            try:
                total = float(parts[1].rstrip('Mm,'))
                used = float(parts[3].rstrip('Mm,'))
                free = float(parts[5].rstrip('Mm,'))
                vpp_mem = {
                    'total_mb': total,
                    'used_mb': used,
                    'free_mb': free,
                    'percent': round((used / total * 100) if total > 0 else 0, 1),
                }
            except (ValueError, IndexError):
                pass

    # VPP errors
    err_output, _ = run_vppctl(['show', 'errors'])
    errors = {'total': 0, 'counters': []}
    for line in err_output.split('\n'):
        parts = line.split()
        if len(parts) >= 3:
            try:
                count = int(parts[0])
                if count > 0:
                    errors['counters'].append({
                        'count': count,
                        'node': parts[1],
                        'reason': ' '.join(parts[2:]),
                    })
                    errors['total'] += count
            except ValueError:
                pass
    errors['counters'] = errors['counters'][:20]

    return {
        'available': True,
        'version': version,
        'interfaces': interfaces,
        'nat_sessions': nat_sessions,
        'pppoe': pppoe,
        'memory': vpp_mem,
        'errors': errors,
    }


def get_process_status():
    """Check status of key processes."""
    processes = [
        'vpp',
        'dnsmasq',
        'frr',
        'zebra',
        'bgpd',
        'ospfd',
        'vectoros',
    ]

    result = []
    for proc_name in processes:
        try:
            output = subprocess.check_output(
                ['pgrep', '-x', proc_name],
                stderr=subprocess.DEVNULL, text=True, timeout=3,
            )
            pids = [p.strip() for p in output.strip().split('\n') if p.strip()]

            # Get memory and CPU for first PID
            mem_rss = 0
            cpu_pct = 0.0
            if pids and HAS_PSUTIL:
                try:
                    p = psutil.Process(int(pids[0]))
                    mem_rss = p.memory_info().rss
                    cpu_pct = p.cpu_percent(interval=0)
                except Exception:
                    pass
            elif pids:
                # Fallback: read from /proc
                try:
                    with open(f'/proc/{pids[0]}/status') as f:
                        for line in f:
                            if line.startswith('VmRSS:'):
                                mem_rss = int(line.split()[1]) * 1024
                                break
                except Exception:
                    pass

            result.append({
                'name': proc_name,
                'running': len(pids) > 0,
                'pid': int(pids[0]) if pids else None,
                'mem_rss': mem_rss,
                'cpu_percent': cpu_pct,
            })
        except Exception:
            result.append({
                'name': proc_name,
                'running': False,
                'pid': None,
                'mem_rss': 0,
                'cpu_percent': 0,
            })

    return result


def get_temperature():
    """Get temperature sensors if available."""
    temps = []

    # Try thermal zones
    thermal_dir = Path('/sys/class/thermal')
    if thermal_dir.exists():
        for zone in sorted(thermal_dir.glob('thermal_zone*')):
            try:
                type_file = zone / 'type'
                temp_file = zone / 'temp'
                if temp_file.exists():
                    temp_raw = int(temp_file.read_text().strip())
                    temp_c = temp_raw / 1000.0
                    zone_type = type_file.read_text().strip() if type_file.exists() else 'unknown'
                    temps.append({
                        'sensor': zone_type,
                        'temp_celsius': round(temp_c, 1),
                    })
            except Exception:
                pass

    # Try hwmon
    hwmon_dir = Path('/sys/class/hwmon')
    if hwmon_dir.exists():
        for hwmon in sorted(hwmon_dir.glob('hwmon*')):
            try:
                name_file = hwmon / 'name'
                hwmon_name = name_file.read_text().strip() if name_file.exists() else 'unknown'
                for temp_file in sorted(hwmon.glob('temp*_input')):
                    temp_raw = int(temp_file.read_text().strip())
                    temp_c = temp_raw / 1000.0
                    label_file = hwmon / f'{temp_file.stem}_label'
                    label = label_file.read_text().strip() if label_file.exists() else temp_file.stem
                    temps.append({
                        'sensor': f'{hwmon_name}/{label}',
                        'temp_celsius': round(temp_c, 1),
                    })
            except Exception:
                pass

    return temps


def get_load_average():
    """Get system load averages."""
    try:
        load1, load5, load15 = os.getloadavg()
        return {
            'load_1m': round(load1, 2),
            'load_5m': round(load5, 2),
            'load_15m': round(load15, 2),
        }
    except Exception:
        return {'load_1m': 0, 'load_5m': 0, 'load_15m': 0}


def get_uptime():
    """Get system uptime in seconds."""
    try:
        with open('/proc/uptime') as f:
            return float(f.read().split()[0])
    except Exception:
        return 0


def main():
    now = time.time()

    result = {
        'timestamp': now,
        'cpu': get_cpu_summary(),
        'cpu_cores': get_cpu_per_core(),
        'memory': get_memory(),
        'disk_usage': get_disk_usage(),
        'disk_io': get_disk_io(),
        'network': get_network_per_interface(),
        'vpp': get_vpp_metrics(),
        'processes': get_process_status(),
        'temperatures': get_temperature(),
        'load_average': get_load_average(),
        'uptime': get_uptime(),
    }

    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
