#!/usr/bin/env python3
"""VectorOS Log Manager - Collect, filter, and rotate logs from VPP, dnsmasq, and VectorOS."""

import sys
import os
import json
import argparse
import subprocess
import re
from datetime import datetime, timedelta
from pathlib import Path

LOG_DIR = Path("/var/log/vectoros")
LOG_SOURCES = {
    "vpp": "/var/log/vpp/vpp.log",
    "dnsmasq": "/var/log/dnsmasq.log",
    "vectoros": "/var/log/vectoros/control-plane.log",
}
MAX_LOG_SIZE_MB = 50
LOG_RETENTION_DAYS = 7
VALID_LEVELS = ["debug", "info", "warn", "error"]

LEVEL_PRIORITY = {"debug": 0, "info": 1, "warn": 2, "error": 3}


def ensure_log_dir():
    """Ensure log directory exists."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)


def parse_log_line(line):
    """Parse a log line into structured components."""
    line = line.strip()
    if not line:
        return None

    # Try common formats
    # Format: 2026-06-04 12:34:56 [level] message
    m = re.match(r'^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?)\s*(?:\[|\b)(debug|info|warn|error|warning|DEBUG|INFO|WARN|ERROR|WARNING)\b(?:\])?\s*(.*)', line)
    if m:
        ts, level, msg = m.groups()
        level = level.lower()
        if level == "warning":
            level = "warn"
        return {"timestamp": ts, "level": level, "message": msg}

    # Format: level: message
    m = re.match(r'^(debug|info|warn|error|warning|DEBUG|INFO|WARN|ERROR|WARNING)[:\s]+(.*)', line)
    if m:
        level = m.group(1).lower()
        if level == "warning":
            level = "warn"
        return {"timestamp": datetime.now().isoformat(), "level": level, "message": m.group(2)}

    # Unrecognized line, treat as info
    return {"timestamp": datetime.now().isoformat(), "level": "info", "message": line}


def read_log_file(filepath, tail_lines=200):
    """Read last N lines from a log file."""
    path = Path(filepath)
    if not path.exists():
        return []

    lines = []
    try:
        with open(path, "r", errors="replace") as f:
            all_lines = f.readlines()
            lines = all_lines[-tail_lines:]
    except Exception:
        return []

    parsed = []
    for line in lines:
        entry = parse_log_line(line)
        if entry:
            parsed.append(entry)
    return parsed


def cmd_show(args):
    """Show logs from specified sources with optional filtering."""
    ensure_log_dir()
    sources = args.sources.split(",") if args.sources else list(LOG_SOURCES.keys())
    min_level = args.level.lower() if args.level else "debug"
    min_priority = LEVEL_PRIORITY.get(min_level, 0)
    tail = args.lines if args.lines else 200
    keyword = args.filter.lower() if args.filter else None

    all_logs = []

    for source in sources:
        source = source.strip()
        if source not in LOG_SOURCES:
            continue
        filepath = LOG_SOURCES[source]
        entries = read_log_file(filepath, tail)
        for entry in entries:
            entry["source"] = source
        all_logs.extend(entries)

    # Filter by level
    all_logs = [e for e in all_logs if LEVEL_PRIORITY.get(e["level"], 0) >= min_priority]

    # Filter by keyword
    if keyword:
        all_logs = [e for e in all_logs if keyword in e["message"].lower()]

    # Sort by timestamp descending
    all_logs.sort(key=lambda x: x.get("timestamp", ""), reverse=True)

    # Limit output
    all_logs = all_logs[:args.limit if args.limit else 100]

    print(json.dumps({
        "status": "ok",
        "count": len(all_logs),
        "logs": all_logs
    }))


def cmd_clear(args):
    """Clear logs for specified sources."""
    ensure_log_dir()
    sources = args.sources.split(",") if args.sources else list(LOG_SOURCES.keys())
    results = []

    for source in sources:
        source = source.strip()
        if source not in LOG_SOURCES:
            results.append({"source": source, "status": "error", "message": "Unknown source"})
            continue
        filepath = LOG_SOURCES[source]
        try:
            path = Path(filepath)
            if path.exists():
                # Rotate before clearing
                rotated_name = f"{filepath}.{datetime.now().strftime('%Y%m%d%H%M%S')}.bak"
                path.rename(rotated_name)
                path.touch()
                results.append({"source": source, "status": "ok", "message": f"Log cleared, rotated to {rotated_name}"})
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
                results.append({"source": source, "status": "ok", "message": "Log file created (was empty)"})
        except Exception as e:
            results.append({"source": source, "status": "error", "message": str(e)})

    print(json.dumps({"status": "ok", "results": results}))


def cmd_filter(args):
    """Advanced log filtering with multiple criteria."""
    ensure_log_dir()
    sources = args.sources.split(",") if args.sources else list(LOG_SOURCES.keys())
    tail = args.lines if args.lines else 500

    all_logs = []
    for source in sources:
        source = source.strip()
        if source not in LOG_SOURCES:
            continue
        filepath = LOG_SOURCES[source]
        entries = read_log_file(filepath, tail)
        for entry in entries:
            entry["source"] = source
        all_logs.extend(entries)

    # Filter by level
    if args.level:
        min_priority = LEVEL_PRIORITY.get(args.level.lower(), 0)
        all_logs = [e for e in all_logs if LEVEL_PRIORITY.get(e["level"], 0) >= min_priority]

    # Filter by source
    if args.source_filter:
        allowed = [s.strip().lower() for s in args.source_filter.split(",")]
        all_logs = [e for e in all_logs if e["source"].lower() in allowed]

    # Filter by keyword in message
    if args.keyword:
        kw = args.keyword.lower()
        all_logs = [e for e in all_logs if kw in e["message"].lower()]

    # Filter by time range
    if args.since:
        try:
            since_dt = datetime.fromisoformat(args.since)
            all_logs = [e for e in all_logs if e.get("timestamp", "") >= since_dt.isoformat()]
        except ValueError:
            pass

    all_logs.sort(key=lambda x: x.get("timestamp", ""), reverse=True)
    all_logs = all_logs[:args.limit if args.limit else 100]

    print(json.dumps({
        "status": "ok",
        "count": len(all_logs),
        "logs": all_logs
    }))


def cmd_rotate(args):
    """Force log rotation for specified sources."""
    ensure_log_dir()
    sources = args.sources.split(",") if args.sources else list(LOG_SOURCES.keys())
    results = []

    for source in sources:
        source = source.strip()
        if source not in LOG_SOURCES:
            results.append({"source": source, "status": "error", "message": "Unknown source"})
            continue
        filepath = LOG_SOURCES[source]
        path = Path(filepath)
        if not path.exists():
            results.append({"source": source, "status": "ok", "message": "No log file to rotate"})
            continue

        size_mb = path.stat().st_size / (1024 * 1024)
        if size_mb < MAX_LOG_SIZE_MB and not args.force:
            results.append({"source": source, "status": "ok", "message": f"Log is {size_mb:.1f}MB, below threshold ({MAX_LOG_SIZE_MB}MB)"})
            continue

        rotated_name = f"{filepath}.{datetime.now().strftime('%Y%m%d%H%M%S')}.bak"
        try:
            path.rename(rotated_name)
            path.touch()
            results.append({"source": source, "status": "ok", "message": f"Rotated to {rotated_name}"})
        except Exception as e:
            results.append({"source": source, "status": "error", "message": str(e)})

    # Clean old rotated logs
    cleaned = 0
    cutoff = datetime.now() - timedelta(days=LOG_RETENTION_DAYS)
    for bak in LOG_DIR.glob("*.bak"):
        try:
            mtime = datetime.fromtimestamp(bak.stat().st_mtime)
            if mtime < cutoff:
                bak.unlink()
                cleaned += 1
        except Exception:
            pass

    print(json.dumps({"status": "ok", "results": results, "cleaned_old": cleaned}))


def main():
    parser = argparse.ArgumentParser(description="VectorOS Log Manager")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # show
    show_parser = subparsers.add_parser("show", help="Show logs")
    show_parser.add_argument("--sources", type=str, default=None, help="Comma-separated sources: vpp,dnsmasq,vectoros")
    show_parser.add_argument("--level", type=str, default="debug", help="Minimum log level: debug,info,warn,error")
    show_parser.add_argument("--lines", type=int, default=200, help="Number of lines to read per source")
    show_parser.add_argument("--filter", type=str, default=None, help="Keyword filter")
    show_parser.add_argument("--limit", type=int, default=100, help="Max log entries to return")

    # clear
    clear_parser = subparsers.add_parser("clear", help="Clear logs")
    clear_parser.add_argument("--sources", type=str, default=None, help="Comma-separated sources to clear")

    # filter
    filter_parser = subparsers.add_parser("filter", help="Advanced log filtering")
    filter_parser.add_argument("--sources", type=str, default=None, help="Comma-separated sources")
    filter_parser.add_argument("--level", type=str, default=None, help="Minimum level")
    filter_parser.add_argument("--source-filter", type=str, default=None, help="Filter by source name")
    filter_parser.add_argument("--keyword", type=str, default=None, help="Keyword in message")
    filter_parser.add_argument("--since", type=str, default=None, help="ISO timestamp, show logs after this time")
    filter_parser.add_argument("--lines", type=int, default=500, help="Lines to read per source")
    filter_parser.add_argument("--limit", type=int, default=100, help="Max entries to return")

    # rotate
    rotate_parser = subparsers.add_parser("rotate", help="Rotate log files")
    rotate_parser.add_argument("--sources", type=str, default=None, help="Comma-separated sources")
    rotate_parser.add_argument("--force", action="store_true", help="Force rotation regardless of size")

    args = parser.parse_args()

    try:
        if args.command == "show":
            cmd_show(args)
        elif args.command == "clear":
            cmd_clear(args)
        elif args.command == "filter":
            cmd_filter(args)
        elif args.command == "rotate":
            cmd_rotate(args)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
