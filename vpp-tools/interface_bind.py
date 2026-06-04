#!/usr/bin/env python3
"""VectorOS Interface Bind - Bind VF interfaces to VPP

Supports two binding methods:
  - dpdk: Bind via PCI address using VPP DPDK driver
  - rdma: Bind via RDMA host-interface (no driver change needed)

Usage:
  python3 interface_bind.py bind --vf enp1s0 --vpp-name wan0 --method rdma
  python3 interface_bind.py bind --vf enp1s0 --vpp-name wan0 --method dpdk --pci 0000:01:00.0
  python3 interface_bind.py unbind --vpp-name wan0
  python3 interface_bind.py list
  python3 interface_bind.py configure --vpp-name wan0 --ip 192.168.1.1/24 --mtu 1500
"""

import sys
import json
import argparse
import subprocess
import os
import re
import glob

# Persisted bindings file
BINDINGS_FILE = "/etc/vectoros/interface_bindings.json"

# Default VF to VPP name mapping
DEFAULT_BINDINGS = {
    "enp1s0": {"vpp_name": "wan0", "method": "rdma"},
    "enp2s0": {"vpp_name": "lan0", "method": "rdma"},
    "enp3s0": {"vpp_name": "lan1", "method": "rdma"},
}


def run_vppctl(args):
    """Run a vppctl command and return (stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            ['vppctl'] + args,
            capture_output=True,
            text=True,
            timeout=15
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except FileNotFoundError:
        return '', 'vppctl not found', 1
    except Exception as e:
        return '', str(e), 1


def run_cmd(args, timeout=15):
    """Run a shell command and return (stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=timeout
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except FileNotFoundError:
        return '', f'Command not found: {args[0]}', 1
    except Exception as e:
        return '', str(e), 1


def get_pci_address(vf_name):
    """Get the PCI address for a VF interface name (e.g. enp1s0 -> 0000:01:00.0)."""
    # Check /sys/class/net/<vf_name>/device symlink
    device_path = f"/sys/class/net/{vf_name}/device"
    if os.path.islink(device_path):
        real = os.path.realpath(device_path)
        # PCI address is the last component (e.g. 0000:01:00.0)
        pci = os.path.basename(real)
        if ':' in pci and len(pci.split(':')) == 3:
            return pci

    # Try lspci
    stdout, stderr, rc = run_cmd(['lspci', '-D', '-s', vf_name])
    if rc == 0 and stdout:
        for line in stdout.splitlines():
            if vf_name in line or ':' in line:
                parts = line.split()
                if parts:
                    return parts[0]

    # Try reading from /sys/class/net/<vf_name>/device/uevent
    uevent_path = f"/sys/class/net/{vf_name}/device/uevent"
    if os.path.isfile(uevent_path):
        with open(uevent_path) as f:
            for line in f:
                if line.startswith('PCI_SLOT_NAME='):
                    return line.strip().split('=', 1)[1]

    return None


def load_bindings():
    """Load persisted bindings from disk."""
    if os.path.isfile(BINDINGS_FILE):
        try:
            with open(BINDINGS_FILE) as f:
                return json.load(f)
        except (json.JSONDecodeError, IOError):
            pass
    return {}


def save_bindings(bindings):
    """Persist bindings to disk."""
    os.makedirs(os.path.dirname(BINDINGS_FILE), exist_ok=True)
    with open(BINDINGS_FILE, 'w') as f:
        json.dump(bindings, f, indent=2)


def is_bound_in_vpp(vpp_name):
    """Check if an interface with the given VPP name exists."""
    stdout, stderr, rc = run_vppctl(['show', 'interface'])
    if rc != 0:
        return False
    for line in stdout.splitlines():
        parts = line.split()
        if parts and parts[0] == vpp_name:
            return True
    return False


def get_vf_driver(vf_name):
    """Get the current kernel driver for a VF interface."""
    driver_path = f"/sys/class/net/{vf_name}/device/driver"
    if os.path.islink(driver_path):
        return os.path.basename(os.readlink(driver_path))
    return None


def unbind_kernel_driver(vf_name):
    """Unbind a VF from its current kernel driver (needed for DPDK binding)."""
    driver = get_vf_driver(vf_name)
    if not driver:
        return {'error': f'Could not determine driver for {vf_name}'}

    # Find PCI address
    pci = get_pci_address(vf_name)
    if not pci:
        return {'error': f'Could not determine PCI address for {vf_name}'}

    unbind_path = f"/sys/bus/pci/drivers/{driver}/unbind"
    if os.path.isfile(unbind_path):
        try:
            with open(unbind_path, 'w') as f:
                f.write(pci)
            return {'status': 'ok', 'message': f'Unbound {vf_name} (PCI {pci}) from {driver}'}
        except PermissionError:
            return {'error': f'Permission denied unbinding {vf_name}. Run as root.'}
        except Exception as e:
            return {'error': f'Failed to unbind: {e}'}

    return {'error': f'Unbind path not found: {unbind_path}'}


def bind_to_vfio_pci(pci):
    """Bind a PCI device to vfio-pci driver."""
    # Load vfio-pci if not loaded
    run_cmd(['modprobe', 'vfio-pci'])

    # Bind to vfio-pci
    bind_path = "/sys/bus/pci/drivers/vfio-pci/bind"
    if os.path.isfile(bind_path):
        try:
            with open(bind_path, 'w') as f:
                f.write(pci)
            return True
        except Exception:
            pass
    return False


def bind_vf_to_vpp(vf_name, vpp_name, method='rdma', pci=None):
    """Bind a VF interface to VPP.

    Methods:
      - rdma: Uses 'create interface rdma host-if <vf_name> name <vpp_name>'
      - dpdk: Unbinds from kernel, creates 'create interface dpdk dev <pci> name <vpp_name>'
    """
    # Check if already bound in VPP
    if is_bound_in_vpp(vpp_name):
        return {'error': f'Interface {vpp_name} already exists in VPP'}

    if method == 'rdma':
        # RDMA host-interface binding (simplest, no driver change)
        stdout, stderr, rc = run_vppctl([
            'create', 'interface', 'rdma',
            'host-if', vf_name,
            'name', vpp_name
        ])
        if rc != 0:
            # Try alternative RDMA syntax
            stdout, stderr, rc = run_vppctl([
                'create', 'interface', 'host',
                vpp_name,
                'host-if', vf_name
            ])

        if rc != 0:
            # Fall back to DPDK if RDMA not available
            return bind_vf_to_vpp(vf_name, vpp_name, method='dpdk', pci=pci)

        # Bring interface up
        run_vppctl(['set', 'interface', 'state', vpp_name, 'up'])

        # Persist binding
        bindings = load_bindings()
        bindings[vpp_name] = {
            'vf_name': vf_name,
            'vpp_name': vpp_name,
            'method': 'rdma',
            'pci': pci or get_pci_address(vf_name),
        }
        save_bindings(bindings)

        return {
            'status': 'ok',
            'vpp_name': vpp_name,
            'vf_name': vf_name,
            'method': 'rdma',
            'message': f'Bound {vf_name} to VPP as {vpp_name} via RDMA host-interface'
        }

    elif method == 'dpdk':
        # DPDK binding - requires PCI address
        if not pci:
            pci = get_pci_address(vf_name)
        if not pci:
            return {'error': f'Could not determine PCI address for {vf_name}. Use --pci to specify.'}

        # Unbind from kernel driver
        unbind_result = unbind_kernel_driver(vf_name)
        if 'error' in unbind_result:
            return unbind_result

        # Bind to vfio-pci
        if not bind_to_vfio_pci(pci):
            return {'error': f'Failed to bind {pci} to vfio-pci'}

        # Create VPP DPDK interface
        stdout, stderr, rc = run_vppctl([
            'create', 'interface', 'dpdk',
            'dev', pci,
            'name', vpp_name
        ])
        if rc != 0:
            return {'error': f'Failed to create DPDK interface: {stderr}'}

        # Bring interface up
        run_vppctl(['set', 'interface', 'state', vpp_name, 'up'])

        # Persist binding
        bindings = load_bindings()
        bindings[vpp_name] = {
            'vf_name': vf_name,
            'vpp_name': vpp_name,
            'method': 'dpdk',
            'pci': pci,
        }
        save_bindings(bindings)

        return {
            'status': 'ok',
            'vpp_name': vpp_name,
            'vf_name': vf_name,
            'method': 'dpdk',
            'pci': pci,
            'message': f'Bound {vf_name} ({pci}) to VPP as {vpp_name} via DPDK'
        }

    else:
        return {'error': f'Unknown binding method: {method}. Use "rdma" or "dpdk".'}


def unbind_from_vpp(vpp_name):
    """Remove an interface from VPP."""
    if not is_bound_in_vpp(vpp_name):
        return {'error': f'Interface {vpp_name} not found in VPP'}

    # Set interface down first
    run_vppctl(['set', 'interface', 'state', vpp_name, 'down'])

    # Delete the interface
    stdout, stderr, rc = run_vppctl(['delete', 'interface', vpp_name])
    if rc != 0:
        # Try alternative syntax
        stdout, stderr, rc = run_vppctl(['set', 'interface', 'unnumbered', vpp_name, 'del'])
        stdout, stderr, rc = run_vppctl(['delete', 'sub-interface', vpp_name])

    # Remove from persisted bindings
    bindings = load_bindings()
    removed = bindings.pop(vpp_name, None)

    # If RDMA binding, the kernel driver is automatically restored
    # If DPDK binding, we could restore the original driver but that's risky

    save_bindings(bindings)

    return {
        'status': 'ok',
        'vpp_name': vpp_name,
        'vf_name': removed.get('vf_name', 'unknown') if removed else 'unknown',
        'message': f'Unbound {vpp_name} from VPP'
    }


def list_bound_interfaces():
    """List all interfaces currently bound to VPP, including binding metadata."""
    stdout, stderr, rc = run_vppctl(['show', 'interface'])
    if rc != 0:
        return {'error': f'Failed to list interfaces: {stderr}'}

    vpp_ifaces = []
    for line in stdout.splitlines():
        line = line.strip()
        if not line or line.startswith('Name') or line.startswith('---') or line.startswith('admin') or line.startswith('  '):
            continue
        parts = line.split()
        if len(parts) >= 4:
            name = parts[0]
            idx = parts[1] if parts[1].isdigit() else '0'
            state = parts[2]
            mtu_str = parts[3].split('/')[0] if '/' in parts[3] else parts[3]
            mtu = mtu_str if mtu_str.isdigit() else '0'

            vpp_ifaces.append({
                'vpp_name': name,
                'sw_if_index': int(idx),
                'state': state,
                'mtu': int(mtu),
            })

    # Merge with persisted bindings
    bindings = load_bindings()
    for iface in vpp_ifaces:
        b = bindings.get(iface['vpp_name'], {})
        iface['vf_name'] = b.get('vf_name', '')
        iface['method'] = b.get('method', '')
        iface['pci'] = b.get('pci', '')
        iface['bound'] = bool(b)

    # Also add VF interfaces that exist on the system but aren't bound
    available_vfs = []
    for vf_name in sorted(DEFAULT_BINDINGS.keys()):
        if not any(i['vf_name'] == vf_name for i in vpp_ifaces):
            pci = get_pci_address(vf_name)
            driver = get_vf_driver(vf_name)
            available_vfs.append({
                'vf_name': vf_name,
                'pci': pci or '',
                'driver': driver or '',
                'bound': False,
                'suggested_vpp_name': DEFAULT_BINDINGS[vf_name]['vpp_name'],
            })

    return {
        'interfaces': vpp_ifaces,
        'available_vfs': available_vfs,
        'count': len(vpp_ifaces),
    }


def configure_interface(vpp_name, ip=None, mtu=None):
    """Configure an already-bound VPP interface."""
    if not is_bound_in_vpp(vpp_name):
        return {'error': f'Interface {vpp_name} not found in VPP'}

    applied = []
    errors = []

    if ip:
        stdout, stderr, rc = run_vppctl([
            'set', 'interface', 'ip', 'address', vpp_name, ip
        ])
        if rc != 0:
            errors.append(f'IP: {stderr}')
        else:
            applied.append(f'IP {ip} added')

    if mtu is not None:
        stdout, stderr, rc = run_vppctl([
            'set', 'interface', 'mtu', 'packet', str(mtu), vpp_name
        ])
        if rc != 0:
            errors.append(f'MTU: {stderr}')
        else:
            applied.append(f'MTU {mtu} set')

    # Always set interface up
    stdout, stderr, rc = run_vppctl([
        'set', 'interface', 'state', vpp_name, 'up'
    ])
    if rc == 0:
        applied.append('Interface brought up')

    if errors:
        return {'status': 'error', 'applied': applied, 'errors': errors}
    return {'status': 'ok', 'applied': applied}


def bind_all_defaults():
    """Bind all default VF interfaces using the default mapping."""
    results = []
    for vf_name, config in DEFAULT_BINDINGS.items():
        vpp_name = config['vpp_name']
        method = config['method']

        # Check if already bound
        if is_bound_in_vpp(vpp_name):
            results.append({
                'vf_name': vf_name,
                'vpp_name': vpp_name,
                'status': 'skipped',
                'message': f'{vpp_name} already exists in VPP'
            })
            continue

        pci = get_pci_address(vf_name)
        result = bind_vf_to_vpp(vf_name, vpp_name, method=method, pci=pci)
        results.append({
            'vf_name': vf_name,
            'vpp_name': vpp_name,
            **result
        })

    return {'results': results}


def main():
    parser = argparse.ArgumentParser(description='VectorOS Interface Bind')
    parser.add_argument('action', choices=[
        'bind', 'unbind', 'list', 'configure', 'bind-all', 'status'
    ])
    parser.add_argument('--vf', help='VF interface name (e.g. enp1s0)')
    parser.add_argument('--vpp-name', help='VPP interface name (e.g. wan0)')
    parser.add_argument('--method', choices=['rdma', 'dpdk'], default='rdma',
                        help='Binding method (default: rdma)')
    parser.add_argument('--pci', help='PCI address (e.g. 0000:01:00.0)')
    parser.add_argument('--ip', help='IP address with CIDR (e.g. 192.168.1.1/24)')
    parser.add_argument('--mtu', type=int, help='MTU value')

    args = parser.parse_args()

    try:
        if args.action == 'bind':
            if not args.vf or not args.vpp_name:
                result = {'error': '--vf and --vpp-name are required for bind'}
            else:
                result = bind_vf_to_vpp(args.vf, args.vpp_name,
                                        method=args.method, pci=args.pci)

        elif args.action == 'unbind':
            if not args.vpp_name:
                result = {'error': '--vpp-name is required for unbind'}
            else:
                result = unbind_from_vpp(args.vpp_name)

        elif args.action == 'list':
            result = list_bound_interfaces()

        elif args.action == 'configure':
            if not args.vpp_name:
                result = {'error': '--vpp-name is required for configure'}
            else:
                result = configure_interface(args.vpp_name,
                                             ip=args.ip, mtu=args.mtu)

        elif args.action == 'bind-all':
            result = bind_all_defaults()

        elif args.action == 'status':
            result = list_bound_interfaces()

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)


if __name__ == '__main__':
    main()
