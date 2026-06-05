#!/bin/bash
# VectorOS Startup Script
# Configures VPP and services after boot

set -e

echo "=== VectorOS Startup ==="

# Wait for VPP to be ready
echo "[1/6] Waiting for VPP..."
for i in $(seq 1 30); do
    if vppctl show version >/dev/null 2>&1; then
        echo "VPP is ready"
        break
    fi
    sleep 1
done

# Configure interfaces
echo "[2/6] Configuring interfaces..."
vppctl set interface state wan0 up
vppctl set interface state lan0 up
vppctl set interface state lan1 up
vppctl set interface ip address lan0 192.168.1.1/24 2>/dev/null || true

# Configure NAT
echo "[3/6] Configuring NAT..."
vppctl nat44 ei plugin enable sessions 65536 users 8192 2>/dev/null || true
vppctl set interface nat44 ei in lan0 out pppoe-wan0 2>/dev/null || true
vppctl nat44 ei add interface address pppoe-wan0 2>/dev/null || true

# Configure DHCP
echo "[4/6] Configuring DHCP..."
mkdir -p /etc/dnsmasq.d
cat > /etc/dnsmasq.d/vectoros-dhcp.conf << 'EOF'
interface=lan0
bind-dynamic
dhcp-range=192.168.1.100,192.168.1.200,86400s
dhcp-option=option:router,192.168.1.1
dhcp-option=option:dns-server,8.8.8.8,1.1.1.1
log-dhcp
EOF

pkill -9 dnsmasq 2>/dev/null || true
sleep 1
dnsmasq --conf-file=/etc/dnsmasq.d/vectoros-dhcp.conf &

# Configure DNS
echo "[5/6] Configuring DNS..."
cat > /etc/dnsmasq.d/vectoros-dns.conf << 'EOF'
server=8.8.8.8
server=1.1.1.1
server=2001:4860:4860::8888
server=2606:4700:4700::1111
cache-size=1000
listen-address=127.0.0.1,192.168.1.1
bind-dynamic
no-resolv
no-poll
EOF

# Start VectorOS
echo "[6/6] Starting VectorOS..."
systemctl start vectoros

echo "=== VectorOS Startup Complete ==="
echo ""
echo "Access VectorOS at: http://192.168.1.7:8080/"
echo "VPP CLI: vppctl"
echo ""
