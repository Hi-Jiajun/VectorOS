#!/usr/bin/env python3
"""VectorOS VPP DNS Server - Lightweight DNS server for LAN clients

This is a simple DNS server that:
1. Listens on the LAN interface
2. Forwards queries to upstream DNS servers
3. Caches responses
4. Serves LAN clients

This replaces dnsmasq for DNS forwarding functionality.
"""

import sys
import json
import socket
import struct
import threading
import time
import os
import signal

class DNSRecord:
    """Simple DNS record"""
    def __init__(self, name, rtype, rdata, ttl=300):
        self.name = name
        self.rtype = rtype  # 1=A, 28=AAAA, 5=CNAME, 15=MX, 2=NS
        self.rdata = rdata
        self.ttl = ttl
        self.timestamp = time.time()

    def is_expired(self):
        return time.time() - self.timestamp > self.ttl

class DNSServer:
    """Simple DNS server"""
    def __init__(self, listen_addr='0.0.0.0', listen_port=53, upstream_servers=None):
        self.listen_addr = listen_addr
        self.listen_port = listen_port
        self.upstream_servers = upstream_servers or ['8.8.8.8', '1.1.1.1']
        self.cache = {}
        self.running = False
        self.socket = None

    def start(self):
        """Start DNS server"""
        self.running = True
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.socket.bind((self.listen_addr, self.listen_port))
        print(f"DNS server listening on {self.listen_addr}:{self.listen_port}", file=sys.stderr)

        while self.running:
            try:
                data, addr = self.socket.recvfrom(512)
                threading.Thread(target=self.handle_query, args=(data, addr)).start()
            except Exception as e:
                if self.running:
                    print(f"Error: {e}", file=sys.stderr)

    def stop(self):
        """Stop DNS server"""
        self.running = False
        if self.socket:
            self.socket.close()

    def handle_query(self, data, addr):
        """Handle DNS query"""
        try:
            # Parse DNS query
            query = self.parse_dns_query(data)
            if not query:
                return

            # Check cache
            cache_key = f"{query['name']}:{query['type']}"
            if cache_key in self.cache and not self.cache[cache_key].is_expired():
                record = self.cache[cache_key]
                response = self.build_dns_response(data, record)
                self.socket.sendto(response, addr)
                return

            # Forward to upstream DNS
            response = self.forward_query(data)
            if response:
                self.socket.sendto(response, addr)
                # Cache response
                self.cache_response(query, response)

        except Exception as e:
            print(f"Error handling query: {e}", file=sys.stderr)

    def parse_dns_query(self, data):
        """Parse DNS query"""
        try:
            # Skip header (12 bytes)
            if len(data) < 12:
                return None

            # Parse question section
            name_parts = []
            offset = 12
            while offset < len(data):
                length = data[offset]
                if length == 0:
                    offset += 1
                    break
                if length > 63:
                    return None
                offset += 1
                name_parts.append(data[offset:offset+length].decode('utf-8', errors='ignore'))
                offset += length

            name = '.'.join(name_parts)
            if offset + 4 <= len(data):
                qtype = struct.unpack('!H', data[offset:offset+2])[0]
                return {'name': name, 'type': qtype}
        except Exception:
            pass
        return None

    def forward_query(self, data):
        """Forward DNS query to upstream server"""
        for server in self.upstream_servers:
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                sock.settimeout(5)
                sock.sendto(data, (server, 53))
                response, _ = sock.recvfrom(512)
                sock.close()
                return response
            except Exception:
                continue
        return None

    def cache_response(self, query, response):
        """Cache DNS response"""
        try:
            # Parse answer section for caching
            if len(response) > 12:
                ancount = struct.unpack('!H', response[6:8])[0]
                if ancount > 0:
                    # Simple cache - store the whole response
                    cache_key = f"{query['name']}:{query['type']}"
                    self.cache[cache_key] = DNSRecord(
                        query['name'],
                        query['type'],
                        response,
                        ttl=300
                    )
        except Exception:
            pass

    def build_dns_response(self, query, record):
        """Build DNS response from cached record"""
        return record.rdata

def main():
    import argparse
    parser = argparse.ArgumentParser(description='VectorOS DNS Server')
    parser.add_argument('action', choices=['start', 'stop', 'status'])
    parser.add_argument('--listen', default='0.0.0.0', help='Listen address')
    parser.add_argument('--port', type=int, default=53, help='Listen port')
    parser.add_argument('--upstream', nargs='+', default=['8.8.8.8', '1.1.1.1'], help='Upstream DNS servers')
    parser.add_argument('--daemon', action='store_true', help='Run as daemon')

    args = parser.parse_args()

    if args.action == 'start':
        server = DNSServer(args.listen, args.port, args.upstream)
        try:
            server.start()
        except KeyboardInterrupt:
            server.stop()
    elif args.action == 'stop':
        # Kill existing DNS server
        os.system('pkill -f "vpp_dns_server.py start"')
        print('DNS server stopped')
    elif args.action == 'status':
        # Check if DNS server is running
        result = os.system('pgrep -f "vpp_dns_server.py start" > /dev/null')
        if result == 0:
            print(json.dumps({'status': 'running', 'listen': '0.0.0.0:53'}))
        else:
            print(json.dumps({'status': 'stopped'}))

if __name__ == '__main__':
    main()
