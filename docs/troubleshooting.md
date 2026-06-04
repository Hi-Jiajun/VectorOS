# VectorOS Troubleshooting Guide

## Quick Diagnostics

Run these commands to quickly assess system health:

```bash
# Check if VectorOS is running
sudo systemctl status vectoros

# Check VPP status
sudo systemctl status vpp
vppctl show version

# Check API health
curl http://localhost:8080/api/health

# Check system resources
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/system

# View recent logs
curl -X POST http://localhost:8080/api/logs \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"limit": 50}'
```

## Common Issues

### 1. VectorOS Fails to Start

**Symptom:** `systemctl start vectoros` fails or the process exits immediately.

**Check logs:**
```bash
sudo journalctl -u vectoros -n 50 --no-pager
```

**Common causes:**

- **VPP not running:** VectorOS requires VPP to be running first.
  ```bash
  sudo systemctl start vpp
  # Wait for VPP to initialize
  sleep 3
  sudo systemctl start vectoros
  ```

- **Port already in use:** Another process is using port 8080.
  ```bash
  sudo lsof -i :8080
  # Kill the conflicting process or change the port
  vectoros --api-listen 0.0.0.0:8081
  ```

- **Missing configuration file:**
  ```bash
  # Create default config
  sudo mkdir -p /etc/vectoros
  sudo cp config.toml /etc/vectoros/config.toml
  ```

- **Database directory not writable:**
  ```bash
  sudo mkdir -p /var/lib/vectoros
  sudo chown root:root /var/lib/vectoros
  ```

### 2. Cannot Connect to API

**Symptom:** `curl http://localhost:8080/api/health` returns connection refused.

**Check:**
```bash
# Is VectorOS running?
ps aux | grep vectoros

# Is the port listening?
sudo ss -tlnp | grep 8080

# Firewall blocking?
sudo iptables -L -n | grep 8080
```

**Solutions:**
- Start VectorOS: `sudo systemctl start vectoros`
- Check the `--api-listen` argument
- Add firewall exception: `sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT`

### 3. VPP Socket Not Found

**Symptom:** API returns `{"error": "Failed to connect to VPP socket: /run/vpp/api.sock"}`

**Check:**
```bash
# Is VPP running?
sudo systemctl status vpp

# Does the socket exist?
ls -la /run/vpp/api.sock

# Check VPP startup logs
sudo journalctl -u vpp -n 50
```

**Solutions:**
- Start VPP: `sudo systemctl start vpp`
- Verify VPP socket path in config matches VPP's actual socket location
- Check VPP configuration in `/etc/vpp/startup.conf`

### 4. PPPoE Connection Fails

**Symptom:** PPPoE client stays in DISCOVERY state and never reaches SESSION state.

**Check:**
```bash
# Check PPPoE status via API
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/pppoe/status

# Check VPP logs for PPPoE errors
vppctl show log | grep -i pppoe

# Verify physical link
vppctl show interface
```

**Common causes:**
- **Wrong credentials:** Verify username and password with your ISP
- **Wrong interface:** Ensure the physical interface is bound to VPP
- **VLAN tagging:** Some ISPs require VLAN tags -- configure if needed
- **MTU mismatch:** PPPoE requires MTU <= 1492

**Solutions:**
```bash
# Re-create PPPoE client with correct settings
curl -X POST http://localhost:8080/api/pppoe/create \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "correct-username",
    "password": "correct-password",
    "interface": "enp1s0",
    "mtu": 1492
  }'

# Check interface binding
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/interfaces/bound
```

### 5. No Internet Through Router

**Symptom:** LAN clients get DHCP addresses but cannot reach the internet.

**Check:**
```bash
# Is NAT enabled?
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/nat/status

# Are interfaces up?
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/interfaces

# Is IP forwarding enabled?
cat /proc/sys/net/ipv4/ip_forward

# Check routing table
vppctl show ip fib
```

**Solutions:**
```bash
# Enable NAT
curl -X POST http://localhost:8080/api/nat/enable \
  -H "Authorization: Bearer <token>"

# Ensure interfaces are up
curl -X POST http://localhost:8080/api/interfaces/wan0/up \
  -H "Authorization: Bearer <token>"
curl -X POST http://localhost:8080/api/interfaces/lan0/up \
  -H "Authorization: Bearer <token>"

# Enable IP forwarding in VPP
vppctl set ip6 fib table 0 0::/0
```

### 6. DHCP Not Working

**Symptom:** LAN clients do not get IP addresses.

**Check:**
```bash
# DHCP status
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/dhcp/status

# Check interface binding
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/interfaces
```

**Solutions:**
```bash
# Enable DHCP server
curl -X POST http://localhost:8080/api/dhcp/enable \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "interface": "lan0",
    "start_ip": "192.168.1.100",
    "end_ip": "192.168.1.200",
    "gateway": "192.168.1.1",
    "lease_time": 86400
  }'
```

### 7. Frontend Not Loading

**Symptom:** Web interface shows 404 or blank page.

**Check:**
```bash
# Is the frontend built?
ls -la frontend/dist/

# Is it in the correct location?
ls -la /etc/vectoros/frontend/dist/
```

**Solutions:**
```bash
# Rebuild frontend
cd frontend
npm install
npm run build
cd ..

# Copy to correct location
sudo cp -r frontend/dist /etc/vectoros/frontend/
```

### 8. Authentication Fails

**Symptom:** API returns 401 Unauthorized with valid token.

**Check:**
```bash
# Verify token is correct
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/config

# Check JWT secret
echo $JWT_SECRET

# Login again to get fresh token
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "vectoros"}'
```

**Solutions:**
- Tokens expire after 24 hours -- log in again
- Ensure `JWT_SECRET` environment variable is consistent across restarts
- Check that credentials match `VECTOROS_USERNAME` and `VECTOROS_PASSWORD`

### 9. Interface Binding Fails

**Symptom:** Cannot bind VF interface to VPP.

**Check:**
```bash
# List available VF interfaces
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/interfaces/bound

# Check PCI devices
lspci | grep Ethernet

# Check driver binding
sudo dpdk-devbind.py --status
```

**Solutions:**
```bash
# For RDMA binding (no driver change)
curl -X POST http://localhost:8080/api/interfaces/bind \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "vf_name": "enp1s0",
    "vpp_name": "wan0",
    "method": "rdma"
  }'

# For DPDK binding (requires vfio-pci)
sudo modprobe vfio-pci
sudo dpdk-devbind.py -b vfio-pci 0000:01:00.0
```

### 10. High CPU Usage

**Symptom:** VectorOS or VPP consuming excessive CPU.

**Check:**
```bash
# System resource usage
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/system

# VPP performance
curl -H "Authorization: Bearer <token>" http://localhost:8080/api/system/vpp-performance

# Process top
top -p $(pgrep vectoros)
```

**Solutions:**
- Check for packet storms or loops in the network
- Reduce logging level: `export RUST_LOG=warn`
- Check firewall rules for excessive logging
- Verify VPP worker thread count matches CPU cores

## Log Locations

| Component | Log Location |
|-----------|-------------|
| VectorOS | `journalctl -u vectoros` |
| VPP | `/var/log/vpp/vpp.log` or `journalctl -u vpp` |
| VectorOS app logs | API endpoint: `POST /api/logs` |

## Debug Mode

Enable debug logging:

```bash
# Set log level
export RUST_LOG=debug

# Run with verbose output
sudo RUST_LOG=debug vectoros --config /etc/vectoros/config.toml
```

## VPP Debug Commands

```bash
# Show all interfaces
vppctl show interface

# Show interface details
vppctl show interface <name>

# Show IP routes
vppctl show ip fib

# Show NAT sessions
vppctl show nat44 ei sessions

# Show PPPoE clients
vppctl show pppoe client

# Show VPP plugins
vppctl show plugin

# Show VPP version
vppctl show version

# Show VPP memory
vppctl show memory

# Show VPP errors
vppctl show error
```

## Getting Help

If you cannot resolve an issue:

1. Check the [GitHub Issues](https://github.com/Hi-Jiajun/vectoros/issues) for similar problems
2. Collect diagnostic information:
   ```bash
   # System info
   uname -a
   vppctl show version
   cargo --version
   
   # VectorOS logs
   sudo journalctl -u vectoros -n 100 > vectoros-logs.txt
   
   # VPP logs
   sudo journalctl -u vpp -n 100 > vpp-logs.txt
   
   # Configuration (redact passwords)
   cat /etc/vectoros/config.toml
   ```
3. Open a GitHub issue with the diagnostic output
