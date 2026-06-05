#!/usr/bin/env python3
"""VectorOS VPP DHCP Server - Lightweight DHCPv4 server for LAN clients

This is a simple DHCP server that:
1. Listens on the LAN interface
2. Assigns IP addresses from a configured range
3. Provides gateway, DNS, and other options
4. Supports lease management

This is a custom implementation since VPP doesn't have a DHCPv4 server plugin.
"""

import sys
import json
import socket
import struct
import threading
import time
import os
import signal
import random

class DHCPLease:
    """DHCP lease"""
    def __init__(self, mac, ip, hostname='', lease_time=86400):
        self.mac = mac
        self.ip = ip
        self.hostname = hostname
        self.lease_time = lease_time
        self.created_at = time.time()
        self.expires_at = time.time() + lease_time

    def is_expired(self):
        return time.time() > self.expires_at

    def to_dict(self):
        return {
            'mac': self.mac,
            'ip': self.ip,
            'hostname': self.hostname,
            'lease_time': self.lease_time,
            'expires_at': self.expires_at,
            'created_at': self.created_at
        }

class DHCPServer:
    """Simple DHCP server"""
    def __init__(self, interface='lan0', gateway='192.168.1.1',
                 start_ip='192.168.1.100', end_ip='192.168.1.200',
                 dns_servers=None, lease_time=86400):
        self.interface = interface
        self.gateway = gateway
        self.start_ip = start_ip
        self.end_ip = end_ip
        self.dns_servers = dns_servers or ['8.8.8.8', '1.1.1.1']
        self.lease_time = lease_time
        self.leases = {}  # mac -> DHCPLease
        self.running = False
        self.socket = None

        # Parse IP range
        self.start_ip_int = self.ip_to_int(start_ip)
        self.end_ip_int = self.ip_to_int(end_ip)
        self.allocated_ips = set()

    def ip_to_int(self, ip):
        """Convert IP to integer"""
        parts = ip.split('.')
        return (int(parts[0]) << 24) + (int(parts[1]) << 16) + (int(parts[2]) << 8) + int(parts[3])

    def int_to_ip(self, ip_int):
        """Convert integer to IP"""
        return f"{(ip_int >> 24) & 0xFF}.{(ip_int >> 16) & 0xFF}.{(ip_int >> 8) & 0xFF}.{ip_int & 0xFF}"

    def allocate_ip(self, mac):
        """Allocate an IP address"""
        # Check if MAC already has a lease
        if mac in self.leases and not self.leases[mac].is_expired():
            return self.leases[mac].ip

        # Find available IP
        for ip_int in range(self.start_ip_int, self.end_ip_int + 1):
            ip = self.int_to_ip(ip_int)
            if ip not in self.allocated_ips:
                self.allocated_ips.add(ip)
                self.leases[mac] = DHCPLease(mac, ip, lease_time=self.lease_time)
                return ip

        return None

    def release_ip(self, mac):
        """Release an IP address"""
        if mac in self.leases:
            ip = self.leases[mac].ip
            self.allocated_ips.discard(ip)
            del self.leases[mac]

    def start(self):
        """Start DHCP server"""
        self.running = True

        # Create raw socket for DHCP
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.bind(('0.0.0.0', 67))
            print(f"DHCP server listening on port 67", file=sys.stderr)
        except Exception as e:
            print(f"Failed to start DHCP server: {e}", file=sys.stderr)
            return

        while self.running:
            try:
                data, addr = self.socket.recvfrom(1024)
                threading.Thread(target=self.handle_dhcp, args=(data, addr)).start()
            except Exception as e:
                if self.running:
                    print(f"Error: {e}", file=sys.stderr)

    def stop(self):
        """Stop DHCP server"""
        self.running = False
        if self.socket:
            self.socket.close()

    def handle_dhcp(self, data, addr):
        """Handle DHCP message"""
        try:
            # Parse DHCP message
            if len(data) < 240:
                return

            # Get message type
            msg_type = data[0]
            if msg_type != 1:  # BOOTREQUEST
                return

            # Get MAC address
            mac = ':'.join(f'{b:02x}' for b in data[28:34])

            # Get DHCP message type from options
            dhcp_type = self.get_dhcp_option(data, 53)
            if not dhcp_type:
                return

            dhcp_type = dhcp_type[0]

            if dhcp_type == 1:  # DHCPDISCOVER
                self.handle_discover(data, mac, addr)
            elif dhcp_type == 3:  # DHCPREQUEST
                self.handle_request(data, mac, addr)
            elif dhcp_type == 7:  # DHCPRELEASE
                self.release_ip(mac)

        except Exception as e:
            print(f"Error handling DHCP: {e}", file=sys.stderr)

    def handle_discover(self, data, mac, addr):
        """Handle DHCPDISCOVER"""
        ip = self.allocate_ip(mac)
        if not ip:
            return

        # Build DHCPOFFER
        response = self.build_dhcp_response(data, 2, ip, mac)  # 2 = DHCPOFFER
        self.socket.sendto(response, ('<broadcast>', 68))

    def handle_request(self, data, mac, addr):
        """Handle DHCPREQUEST"""
        ip = self.allocate_ip(mac)
        if not ip:
            return

        # Build DHCPACK
        response = self.build_dhcp_response(data, 5, ip, mac)  # 5 = DHCPACK
        self.socket.sendto(response, ('<broadcast>', 68))

    def get_dhcp_option(self, data, option_code):
        """Get DHCP option value"""
        offset = 240  # Skip fixed header
        while offset < len(data):
            if offset + 2 > len(data):
                break
            code = data[offset]
            if code == 255:  # End option
                break
            if code == 0:  # Pad option
                offset += 1
                continue
            length = data[offset + 1]
            if offset + 2 + length > len(data):
                break
            if code == option_code:
                return data[offset + 2:offset + 2 + length]
            offset += 2 + length
        return None

    def build_dhcp_response(self, request, msg_type, ip, mac):
        """Build DHCP response"""
        # Get transaction ID
        xid = request[4:8]

        # Build response
        response = bytearray(240)

        # Message type (BOOTREPLY)
        response[0] = 2
        # Hardware type (Ethernet)
        response[1] = 1
        # Hardware address length
        response[2] = 6
        # Hops
        response[3] = 0
        # Transaction ID
        response[4:8] = xid
        # Seconds
        response[8:10] = b'\x00\x00'
        # Flags (broadcast)
        response[10:12] = b'\x80\x00'
        # Client IP
        response[12:16] = b'\x00\x00\x00\x00'
        # Your IP
        response[16:20] = socket.inet_aton(ip)
        # Server IP
        response[20:24] = socket.inet_aton(self.gateway)
        # Gateway IP
        response[24:28] = b'\x00\x00\x00\x00'
        # Client MAC
        response[28:34] = bytes.fromhex(mac.replace(':', ''))

        # DHCP options
        options = bytearray()

        # DHCP Message Type
        options.extend([53, 1, msg_type])

        # Server Identifier
        options.extend([54, 4])
        options.extend(socket.inet_aton(self.gateway))

        # IP Address Lease Time
        options.extend([51, 4])
        options.extend(struct.pack('!I', self.lease_time))

        # Subnet Mask
        options.extend([1, 4])
        options.extend(socket.inet_aton('255.255.255.0'))

        # Router (Gateway)
        options.extend([3, 4])
        options.extend(socket.inet_aton(self.gateway))

        # DNS Servers
        options.extend([6, len(self.dns_servers) * 4])
        for dns in self.dns_servers:
            options.extend(socket.inet_aton(dns))

        # End option
        options.extend([255])

        return bytes(response) + bytes(options)

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS DHCP Server')
    parser.add_argument('action', choices=['start', 'stop', 'status'])
    parser.add_argument('--interface', default='lan0', help='Interface to listen on')
    parser.add_argument('--gateway', default='192.168.1.1', help='Gateway IP')
    parser.add_argument('--start-ip', default='192.168.1.100', help='DHCP range start')
    parser.add_argument('--end-ip', default='192.168.1.200', help='DHCP range end')
    parser.add_argument('--dns', nargs='+', default=['8.8.8.8', '1.1.1.1'], help='DNS servers')
    parser.add_argument('--lease-time', type=int, default=86400, help='Lease time in seconds')

    args = parser.parse_args()

    if args.action == 'start':
        server = DHCPServer(
            interface=args.interface,
            gateway=args.gateway,
            start_ip=args.start_ip,
            end_ip=args.end_ip,
            dns_servers=args.dns,
            lease_time=args.lease_time
        )
        try:
            server.start()
        except KeyboardInterrupt:
            server.stop()
    elif args.action == 'stop':
        os.system('pkill -f "vpp_dhcp_server.py start"')
        print('DHCP server stopped')
    elif args.action == 'status':
        result = os.system('pgrep -f "vpp_dhcp_server.py start" > /dev/null')
        if result == 0:
            print(json.dumps({'status': 'running', 'interface': 'lan0'}))
        else:
            print(json.dumps({'status': 'stopped'}))

if __name__ == '__main__':
    main()
