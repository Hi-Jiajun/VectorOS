#!/usr/bin/env python3
"""
VectorOS eBPF Manager

Manages eBPF programs for traffic steering, filtering, and monitoring.
Integrates with VPP for hybrid packet processing.

Usage:
    python3 ebpf_manager.py load <program> [--interface <iface>]
    python3 ebpf_manager.py unload <program>
    python3 ebpf_manager.py show [<program>]
    python3 ebpf_manager.py stats [--program <name>]
    python3 ebpf_manager.py maps [<program>]
    python3 ebpf_manager.py map-update <map> <key> <value>
    python3 ebpf_manager.py list

Commands:
    load        Load and attach an eBPF program
    unload      Detach and unload an eBPF program
    show        Show loaded eBPF program details
    stats       Show traffic statistics from eBPF maps
    maps        List eBPF maps for a program
    map-update  Update a value in an eBPF map
    list        List all loaded eBPF programs

Examples:
    # Load XDP pre-filter on WAN interface
    python3 ebpf_manager.py load xdp_pre_filter --interface eth0

    # Show all loaded programs
    python3 ebpf_manager.py show

    # Get traffic statistics
    python3 ebpf_manager.py stats

    # Update blocklist map
    python3 ebpf_manager.py map-update blocklist 192.168.1.100 1
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Any

# eBPF program directory
EBPF_DIR = "/usr/lib/vectoros/ebpf"
EBPF_PIN_DIR = "/sys/fs/bpf/vectoros"


class EbpfManager:
    """Manages eBPF programs for VectorOS."""

    def __init__(self):
        self.programs: Dict[str, Dict] = {}
        self._ensure_dirs()

    def _ensure_dirs(self):
        """Ensure eBPF directories exist."""
        os.makedirs(EBPF_DIR, exist_ok=True)
        os.makedirs(EBPF_PIN_DIR, exist_ok=True)

    def load(self, program_name: str, interface: str = "eth0") -> Dict:
        """
        Load and attach an eBPF program.

        Args:
            program_name: Name of the eBPF program to load
            interface: Network interface to attach to

        Returns:
            Dictionary with load status
        """
        program_path = f"{EBPF_DIR}/{program_name}.bpf.o"

        if not os.path.exists(program_path):
            return {
                "status": "error",
                "message": f"Program not found: {program_path}"
            }

        # In real implementation, this would use bpftool or libbpf
        # to load and attach the eBPF program
        try:
            # Placeholder: actual implementation would use:
            # 1. bpftool prog load <program_path> <pin_path>
            # 2. bpftool net attach xdp id <prog_id> dev <interface>

            self.programs[program_name] = {
                "name": program_name,
                "interface": interface,
                "path": program_path,
                "type": "xdp",  # or "tc" for TC programs
                "status": "loaded",
                "id": len(self.programs) + 1
            }

            return {
                "status": "success",
                "program": program_name,
                "interface": interface,
                "message": f"Program {program_name} loaded on {interface}"
            }
        except Exception as e:
            return {
                "status": "error",
                "message": str(e)
            }

    def unload(self, program_name: str) -> Dict:
        """
        Detach and unload an eBPF program.

        Args:
            program_name: Name of the program to unload

        Returns:
            Dictionary with unload status
        """
        if program_name not in self.programs:
            return {
                "status": "error",
                "message": f"Program not loaded: {program_name}"
            }

        try:
            # In real implementation:
            # 1. bpftool net detach xdp dev <interface>
            # 2. bpftool prog detach <prog_id> xdp
            # 3. rm <pin_path>

            del self.programs[program_name]

            return {
                "status": "success",
                "program": program_name,
                "message": f"Program {program_name} unloaded"
            }
        except Exception as e:
            return {
                "status": "error",
                "message": str(e)
            }

    def show(self, program_name: Optional[str] = None) -> Dict:
        """
        Show eBPF program details.

        Args:
            program_name: Optional specific program name

        Returns:
            Dictionary with program information
        """
        if program_name:
            if program_name in self.programs:
                return {
                    "status": "success",
                    "program": self.programs[program_name]
                }
            else:
                return {
                    "status": "error",
                    "message": f"Program not found: {program_name}"
                }

        return {
            "status": "success",
            "programs": list(self.programs.values()),
            "count": len(self.programs)
        }

    def stats(self, program_name: Optional[str] = None) -> Dict:
        """
        Get traffic statistics from eBPF maps.

        Args:
            program_name: Optional specific program name

        Returns:
            Dictionary with traffic statistics
        """
        # Placeholder: real implementation would read from eBPF maps
        stats_data = {
            "total_packets": 0,
            "total_bytes": 0,
            "dropped_packets": 0,
            "passed_packets": 0,
            "per_interface": {}
        }

        return {
            "status": "success",
            "stats": stats_data,
            "program": program_name
        }

    def maps(self, program_name: Optional[str] = None) -> Dict:
        """
        List eBPF maps for a program.

        Args:
            program_name: Optional specific program name

        Returns:
            Dictionary with map information
        """
        # Placeholder: real implementation would enumerate maps
        maps_data = [
            {
                "name": "traffic_stats",
                "type": "percpu_array",
                "max_entries": 256,
                "key_size": 4,
                "value_size": 24
            },
            {
                "name": "blocklist",
                "type": "lru_hash",
                "max_entries": 65536,
                "key_size": 4,
                "value_size": 4
            }
        ]

        return {
            "status": "success",
            "maps": maps_data,
            "program": program_name
        }

    def map_update(self, map_name: str, key: str, value: str) -> Dict:
        """
        Update a value in an eBPF map.

        Args:
            map_name: Name of the map
            key: Map key
            value: Map value

        Returns:
            Dictionary with update status
        """
        try:
            # In real implementation:
            # 1. Parse key/value based on map type
            # 2. Use bpftool map update <map_id> key <key> value <value>

            return {
                "status": "success",
                "map": map_name,
                "key": key,
                "value": value,
                "message": f"Map {map_name} updated"
            }
        except Exception as e:
            return {
                "status": "error",
                "message": str(e)
            }

    def list_programs(self) -> Dict:
        """
        List all loaded eBPF programs.

        Returns:
            Dictionary with list of programs
        """
        return {
            "status": "success",
            "programs": [
                {
                    "name": p["name"],
                    "interface": p["interface"],
                    "type": p["type"],
                    "status": p["status"]
                }
                for p in self.programs.values()
            ],
            "count": len(self.programs)
        }


def main():
    parser = argparse.ArgumentParser(
        description="VectorOS eBPF Manager",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument("command", choices=[
        "load", "unload", "show", "stats", "maps", "map-update", "list"
    ])
    parser.add_argument("args", nargs="*")
    parser.add_argument("--interface", "-i", default="eth0")
    parser.add_argument("--program", "-p")

    args = parser.parse_args()
    manager = EbpfManager()

    if args.command == "load":
        if not args.args:
            print(json.dumps({"status": "error", "message": "Program name required"}))
            sys.exit(1)
        result = manager.load(args.args[0], args.interface)
    elif args.command == "unload":
        if not args.args:
            print(json.dumps({"status": "error", "message": "Program name required"}))
            sys.exit(1)
        result = manager.unload(args.args[0])
    elif args.command == "show":
        program = args.args[0] if args.args else None
        result = manager.show(program)
    elif args.command == "stats":
        result = manager.stats(args.program)
    elif args.command == "maps":
        program = args.args[0] if args.args else None
        result = manager.maps(program)
    elif args.command == "map-update":
        if len(args.args) < 3:
            print(json.dumps({"status": "error", "message": "map, key, and value required"}))
            sys.exit(1)
        result = manager.map_update(args.args[0], args.args[1], args.args[2])
    elif args.command == "list":
        result = manager.list_programs()
    else:
        result = {"status": "error", "message": f"Unknown command: {args.command}"}

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
