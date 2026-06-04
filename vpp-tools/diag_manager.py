#!/usr/bin/env python3
"""VectorOS Network Diagnostics Manager

Provides ping, traceroute, DNS lookup, and port scanning tools
similar to OpenWrt/ImmortalWrt luci-app-diag.

Usage:
    diag_manager.py ping --host <target> [--count <n>] [--size <bytes>]
    diag_manager.py traceroute --host <target> [--max-hops <n>]
    diag_manager.py dns --domain <domain> [--server <dns_server>]
    diag_manager.py portscan --host <target> --ports <start-end|port,port>
"""

import json
import subprocess
import sys
import argparse
import re
import socket


def run_cmd(cmd, timeout=30):
    """Run a command and return (stdout, stderr, returncode)."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.stdout.strip(), result.stderr.strip(), result.returncode
    except FileNotFoundError:
        return "", f"Command not found: {cmd[0]}", 127
    except subprocess.TimeoutExpired:
        return "", "Command timed out", 124
    except Exception as e:
        return "", str(e), 1


def cmd_ping(args):
    """Perform a ping and return parsed results."""
    cmd = ["ping", "-c", str(args.count)]

    if args.size:
        cmd.extend(["-s", str(args.size)])
    if args.host:
        cmd.append(args.host)
    else:
        return {"error": "host is required"}

    stdout, stderr, rc = run_cmd(cmd, timeout=max(args.count * 5, 30))

    if rc != 0 and not stdout:
        return {"error": stderr or "Ping failed", "host": args.host}

    # Parse ping output
    lines = stdout.split("\n")
    packets_sent = 0
    packets_received = 0
    packet_loss = "100%"
    rtt_min = 0.0
    rtt_avg = 0.0
    rtt_max = 0.0
    rtt_mdev = 0.0
    replies = []

    for line in lines:
        # Parse individual reply lines: "64 bytes from 8.8.8.8: icmp_seq=1 ttl=116 time=12.3 ms"
        reply_match = re.match(
            r"(\d+) bytes from ([^:]+): icmp_seq=(\d+) ttl=(\d+) time=([\d.]+)",
            line,
        )
        if reply_match:
            replies.append({
                "bytes": int(reply_match.group(1)),
                "source": reply_match.group(2),
                "icmp_seq": int(reply_match.group(3)),
                "ttl": int(reply_match.group(4)),
                "time_ms": float(reply_match.group(5)),
            })

        # Parse ping statistics line: "3 packets transmitted, 3 received, 0% packet loss"
        stats_match = re.search(
            r"(\d+) (?:packets? )?transmitted.*?(\d+) (?:packets? )?received.*?(\d+\.?\d*)% packet loss",
            line,
        )
        if stats_match:
            packets_sent = int(stats_match.group(1))
            packets_received = int(stats_match.group(2))
            packet_loss = f"{stats_match.group(3)}%"

        # Parse rtt line: "rtt min/avg/max/mdev = 12.345/13.456/14.567/1.234 ms"
        rtt_match = re.search(
            r"rtt min/avg/max/mdev = ([\d.]+)/([\d.]+)/([\d.]+)/([\d.]+)",
            line,
        )
        if rtt_match:
            rtt_min = float(rtt_match.group(1))
            rtt_avg = float(rtt_match.group(2))
            rtt_max = float(rtt_match.group(3))
            rtt_mdev = float(rtt_match.group(4))

    return {
        "host": args.host,
        "packets_sent": packets_sent,
        "packets_received": packets_received,
        "packet_loss": packet_loss,
        "rtt": {
            "min": rtt_min,
            "avg": rtt_avg,
            "max": rtt_max,
            "mdev": rtt_mdev,
        },
        "replies": replies,
        "raw_output": stdout,
    }


def cmd_traceroute(args):
    """Perform a traceroute and return parsed results."""
    cmd = ["traceroute", "-n", "-m", str(args.max_hops)]

    if args.host:
        cmd.append(args.host)
    else:
        return {"error": "host is required"}

    stdout, stderr, rc = run_cmd(cmd, timeout=60)

    hops = []
    for line in stdout.split("\n"):
        line = line.strip()
        if not line or line.startswith("traceroute"):
            continue

        # Parse: " 1  192.168.1.1  1.234 ms  2.345 ms  3.456 ms"
        hop_match = re.match(r"\s*(\d+)\s+(.+)", line)
        if hop_match:
            hop_num = int(hop_match.group(1))
            rest = hop_match.group(2)

            # Extract IPs and times
            addresses = re.findall(r"([\d.]+)", rest)
            times = re.findall(r"([\d.]+)\s*ms", rest)

            hop_entry = {
                "hop": hop_num,
                "addresses": addresses[:3],  # up to 3 probes
                "times_ms": [float(t) for t in times[:3]],
            }

            # Check for timeout (*)
            if "*" in rest and not addresses:
                hop_entry = {
                    "hop": hop_num,
                    "addresses": [],
                    "times_ms": [],
                    "timeout": True,
                }

            hops.append(hop_entry)

    return {
        "host": args.host,
        "max_hops": args.max_hops,
        "hop_count": len(hops),
        "hops": hops,
        "raw_output": stdout,
    }


def cmd_dns(args):
    """Perform DNS lookup and return results."""
    domain = args.domain
    server = args.server

    # Build dig command
    cmd = ["dig", "+short", domain]
    if server:
        cmd = ["dig", f"@{server}", "+short", domain]

    stdout, stderr, rc = run_cmd(cmd, timeout=10)

    # Also try for A and AAAA records specifically
    cmd_a = ["dig", "+short", "A", domain]
    cmd_aaaa = ["dig", "+short", "AAAA", domain]
    if server:
        cmd_a = ["dig", f"@{server}", "+short", "A", domain]
        cmd_aaaa = ["dig", f"@{server}", "+short", "AAAA", domain]

    stdout_a, _, _ = run_cmd(cmd_a, timeout=10)
    stdout_aaaa, _, _ = run_cmd(cmd_aaaa, timeout=10)

    a_records = [r.strip() for r in stdout_a.split("\n") if r.strip() and not r.strip().startswith(";")]
    aaaa_records = [r.strip() for r in stdout_aaaa.split("\n") if r.strip() and not r.strip().startswith(";")]

    # Try SOA, MX, NS via dig
    cmd_soa = ["dig", "+short", "SOA", domain]
    cmd_mx = ["dig", "+short", "MX", domain]
    cmd_ns = ["dig", "+short", "NS", domain]
    if server:
        cmd_soa = ["dig", f"@{server}", "+short", "SOA", domain]
        cmd_mx = ["dig", f"@{server}", "+short", "MX", domain]
        cmd_ns = ["dig", f"@{server}", "+short", "NS", domain]

    stdout_soa, _, _ = run_cmd(cmd_soa, timeout=10)
    stdout_mx, _, _ = run_cmd(cmd_mx, timeout=10)
    stdout_ns, _, _ = run_cmd(cmd_ns, timeout=10)

    soa_record = stdout_soa.strip() if stdout_soa.strip() else None
    mx_records = [r.strip() for r in stdout_mx.split("\n") if r.strip() and not r.strip().startswith(";")]
    ns_records = [r.strip() for r in stdout_ns.split("\n") if r.strip() and not r.strip().startswith(";")]

    return {
        "domain": domain,
        "server": server or "system default",
        "a_records": a_records,
        "aaaa_records": aaaa_records,
        "soa_record": soa_record,
        "mx_records": mx_records,
        "ns_records": ns_records,
        "raw_output": stdout,
    }


def parse_port_range(port_spec):
    """Parse a port specification: '80,443' or '1-1024' or '80,443,8080-8090'."""
    ports = set()
    for part in port_spec.split(","):
        part = part.strip()
        if "-" in part:
            start, end = part.split("-", 1)
            start, end = int(start.strip()), int(end.strip())
            ports.update(range(start, end + 1))
        else:
            ports.add(int(part))
    return sorted(ports)


def cmd_portscan(args):
    """Scan specified ports on a target host."""
    if not args.host:
        return {"error": "host is required"}

    if not args.ports:
        return {"error": "ports is required"}

    # Resolve hostname
    try:
        target_ip = socket.gethostbyname(args.host)
    except socket.gaierror as e:
        return {"error": f"Cannot resolve hostname: {e}", "host": args.host}

    ports = parse_port_range(args.ports)
    if len(ports) > 1024:
        return {"error": "Maximum 1024 ports per scan", "port_count": len(ports)}

    open_ports = []
    closed_ports = []
    filtered_ports = []

    for port in ports:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(2)
            result = sock.connect_ex((target_ip, port))
            sock.close()

            if result == 0:
                # Try to get service name
                try:
                    service = socket.getservbyport(port)
                except OSError:
                    service = "unknown"

                open_ports.append({"port": port, "state": "open", "service": service})
            else:
                closed_ports.append(port)
        except Exception:
            filtered_ports.append(port)

    # Common port services for reference
    common_services = {
        21: "FTP", 22: "SSH", 23: "Telnet", 25: "SMTP",
        53: "DNS", 80: "HTTP", 110: "POP3", 143: "IMAP",
        443: "HTTPS", 993: "IMAPS", 995: "POP3S",
        3306: "MySQL", 5432: "PostgreSQL", 6379: "Redis",
        8080: "HTTP-Alt", 8443: "HTTPS-Alt",
    }

    return {
        "host": args.host,
        "target_ip": target_ip,
        "ports_scanned": len(ports),
        "open_count": len(open_ports),
        "closed_count": len(closed_ports),
        "filtered_count": len(filtered_ports),
        "open_ports": open_ports,
        "closed_ports": closed_ports[:100],  # Cap output
        "filtered_ports": filtered_ports[:100],
    }


def main():
    parser = argparse.ArgumentParser(description="VectorOS Network Diagnostics Manager")
    subparsers = parser.add_subparsers(dest="action", help="Diagnostic tool to run")

    # Ping
    ping_parser = subparsers.add_parser("ping", help="Ping a host")
    ping_parser.add_argument("--host", required=True, help="Target host")
    ping_parser.add_argument("--count", type=int, default=4, help="Number of pings")
    ping_parser.add_argument("--size", type=int, default=None, help="Packet size in bytes")

    # Traceroute
    traceroute_parser = subparsers.add_parser("traceroute", help="Traceroute to a host")
    traceroute_parser.add_argument("--host", required=True, help="Target host")
    traceroute_parser.add_argument("--max-hops", type=int, default=30, help="Maximum hops")

    # DNS
    dns_parser = subparsers.add_parser("dns", help="DNS lookup")
    dns_parser.add_argument("--domain", required=True, help="Domain to resolve")
    dns_parser.add_argument("--server", default=None, help="DNS server to use")

    # Port scan
    portscan_parser = subparsers.add_parser("portscan", help="Scan ports on a host")
    portscan_parser.add_argument("--host", required=True, help="Target host")
    portscan_parser.add_argument("--ports", required=True, help="Ports to scan (e.g. '80,443' or '1-1024')")

    args = parser.parse_args()

    if not args.action:
        parser.print_help()
        sys.exit(0)

    actions = {
        "ping": lambda: cmd_ping(args),
        "traceroute": lambda: cmd_traceroute(args),
        "dns": lambda: cmd_dns(args),
        "portscan": lambda: cmd_portscan(args),
    }

    try:
        result = actions[args.action]()
        print(json.dumps(result, indent=2))
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
