#!/usr/bin/env python3
"""VectorOS Backup Manager - Full system backup and restore.

Backs up:
  - VPP configuration (startup.conf)
  - VectorOS main config (config.json)
  - PPPoE configuration (embedded in config.json)
  - DHCP/DNS configuration (embedded in config.json)
  - Firewall rules (firewall-rules.json)
  - FRRouting configuration (/etc/frr/)
  - Network interfaces configuration

Backups are stored as tar.gz archives in /var/lib/vectoros/backups/.
"""

import sys
import os
import json
import argparse
import tarfile
import hashlib
import shutil
import subprocess
import glob
import time
from pathlib import Path
from datetime import datetime

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
BACKUP_DIR = Path("/var/lib/vectoros/backups")
CONFIG_DIR = Path("/etc/vectoros")
VPP_CONF = Path("/etc/vpp/startup.conf")
FRR_CONF_DIR = Path("/etc/frr")
NETPLAN_DIR = Path("/etc/netplan")
INTERFACES_FILE = Path("/etc/network/interfaces")
MAX_BACKUPS = 10
BACKUP_PREFIX = "vectoros-backup"

# Files/directories that are collected into the backup archive
CONFIG_FILES = {
    "config.json": CONFIG_DIR / "config.json",
    "firewall-rules.json": CONFIG_DIR / "firewall-rules.json",
}

VPP_FILES = {
    "startup.conf": VPP_CONF,
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _ts() -> str:
    """Return a timestamp string suitable for file names."""
    return datetime.now().strftime("%Y%m%d-%H%M%S")


def _sha256(path: Path) -> str:
    """Compute SHA-256 hex digest of a file."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def _ensure_dir(d: Path):
    d.mkdir(parents=True, exist_ok=True)


def _collect_frr_configs(tmp_dir: Path):
    """Copy FRRouting configuration files into *tmp_dir*/frr/."""
    if not FRR_CONF_DIR.is_dir():
        return
    dest = tmp_dir / "frr"
    _ensure_dir(dest)
    for item in FRR_CONF_DIR.iterdir():
        if item.is_file():
            shutil.copy2(item, dest / item.name)
        elif item.is_dir():
            shutil.copytree(item, dest / item.name, dirs_exist_ok=True)


def _collect_netplan(tmp_dir: Path):
    """Copy netplan / network-interfaces configuration."""
    dest = tmp_dir / "network"
    _ensure_dir(dest)
    if NETPLAN_DIR.is_dir():
        for item in NETPLAN_DIR.iterdir():
            if item.is_file():
                shutil.copy2(item, dest / item.name)
    elif INTERFACES_FILE.is_file():
        shutil.copy2(INTERFACES_FILE, dest / "interfaces")


def _restart_service(name: str) -> bool:
    """Restart a systemd service; returns True on success."""
    try:
        result = subprocess.run(
            ["systemctl", "restart", name],
            capture_output=True, text=True, timeout=30,
        )
        return result.returncode == 0
    except Exception:
        return False


# ---------------------------------------------------------------------------
# Export (create backup)
# ---------------------------------------------------------------------------

def export_backup(output_path: Path | None = None) -> dict:
    """Create a full system backup archive.

    Returns a dict with status and backup metadata.
    """
    _ensure_dir(BACKUP_DIR)

    ts = _ts()
    if output_path is None:
        output_path = BACKUP_DIR / f"{BACKUP_PREFIX}-{ts}.tar.gz"

    _ensure_dir(output_path.parent)

    with tarfile.open(output_path, "w:gz") as tar:
        manifest: dict = {
            "version": "1.0",
            "created_at": datetime.now().isoformat(),
            "hostname": _get_hostname(),
            "components": [],
        }

        # --- VectorOS config files ---
        for name, src in CONFIG_FILES.items():
            if src.is_file():
                tar.add(src, arcname=f"vectoros/{name}")
                manifest["components"].append({
                    "name": name,
                    "source": str(src),
                    "sha256": _sha256(src),
                })

        # --- VPP startup.conf ---
        for name, src in VPP_FILES.items():
            if src.is_file():
                tar.add(src, arcname=f"vpp/{name}")
                manifest["components"].append({
                    "name": f"vpp/{name}",
                    "source": str(src),
                    "sha256": _sha256(src),
                })

        # --- FRRouting config ---
        frr_tmp = None
        if FRR_CONF_DIR.is_dir():
            frr_tmp = Path("/tmp/_vback_frr")
            if frr_tmp.exists():
                shutil.rmtree(frr_tmp)
            _collect_frr_configs(frr_tmp)
            tar.add(frr_tmp / "frr", arcname="frr")
            manifest["components"].append({
                "name": "frr/",
                "source": str(FRR_CONF_DIR),
            })

        # --- Network interfaces config ---
        net_tmp = Path("/tmp/_vback_net")
        if net_tmp.exists():
            shutil.rmtree(net_tmp)
        _collect_netplan(net_tmp)
        if (net_tmp / "network").is_dir():
            tar.add(net_tmp / "network", arcname="network")
            manifest["components"].append({
                "name": "network/",
                "source": str(NETPLAN_DIR) if NETPLAN_DIR.is_dir() else str(INTERFACES_FILE),
            })

        # --- Write manifest ---
        manifest_json = json.dumps(manifest, indent=2)
        import io
        info = tarfile.TarInfo(name="manifest.json")
        info.size = len(manifest_json.encode())
        tar.addfile(info, io.BytesIO(manifest_json.encode()))

        # Clean up temp dirs
        if frr_tmp and frr_tmp.exists():
            shutil.rmtree(frr_tmp)
        if net_tmp.exists():
            shutil.rmtree(net_tmp)

    # Set permissions
    os.chmod(output_path, 0o600)

    # Prune old backups
    _prune_backups()

    return {
        "status": "ok",
        "backup_file": str(output_path),
        "size_bytes": output_path.stat().st_size,
        "created_at": manifest["created_at"],
        "components": [c["name"] for c in manifest["components"]],
    }


# ---------------------------------------------------------------------------
# Restore
# ---------------------------------------------------------------------------

def restore_backup(backup_path: Path) -> dict:
    """Restore system configuration from a backup archive.

    Returns a dict with status and restore details.
    """
    if not backup_path.is_file():
        return {"error": f"Backup file not found: {backup_path}"}

    # Validate it's a tar.gz
    if not tarfile.is_tarfile(backup_path):
        return {"error": "File is not a valid tar archive"}

    tmp_dir = Path("/tmp/_vrestore")
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
    _ensure_dir(tmp_dir)

    try:
        with tarfile.open(backup_path, "r:gz") as tar:
            tar.extractall(tmp_dir)
    except Exception as e:
        shutil.rmtree(tmp_dir)
        return {"error": f"Failed to extract backup: {e}"}

    # Read manifest for validation
    manifest_path = tmp_dir / "manifest.json"
    manifest = {}
    if manifest_path.is_file():
        with open(manifest_path) as f:
            manifest = json.load(f)

    restored = []
    errors = []

    # --- Restore VectorOS config files ---
    for name in ("config.json", "firewall-rules.json"):
        src = tmp_dir / "vectoros" / name
        if src.is_file():
            dest = CONFIG_DIR / name
            _ensure_dir(dest.parent)
            shutil.copy2(src, dest)
            restored.append(str(dest))

    # --- Restore VPP startup.conf ---
    vpp_conf = tmp_dir / "vpp" / "startup.conf"
    if vpp_conf.is_file():
        _ensure_dir(VPP_CONF.parent)
        shutil.copy2(vpp_conf, VPP_CONF)
        restored.append(str(VPP_CONF))

    # --- Restore FRRouting config ---
    frr_src = tmp_dir / "frr"
    if frr_src.is_dir() and FRR_CONF_DIR.is_dir():
        for item in frr_src.iterdir():
            dest = FRR_CONF_DIR / item.name
            if item.is_file():
                shutil.copy2(item, dest)
                restored.append(str(dest))
            elif item.is_dir():
                shutil.copytree(item, dest, dirs_exist_ok=True)
                restored.append(str(dest))

    # --- Restore network config ---
    net_src = tmp_dir / "network"
    if net_src.is_dir():
        if NETPLAN_DIR.is_dir():
            for item in net_src.iterdir():
                if item.is_file():
                    dest = NETPLAN_DIR / item.name
                    shutil.copy2(item, dest)
                    restored.append(str(dest))
        elif (net_src / "interfaces").is_file():
            shutil.copy2(net_src / "interfaces", INTERFACES_FILE)
            restored.append(str(INTERFACES_FILE))

    # --- Restart services ---
    services_restarted = []
    for svc in ("vpp", "frr", "vectoros"):
        if _restart_service(svc):
            services_restarted.append(svc)

    # Clean up
    shutil.rmtree(tmp_dir)

    return {
        "status": "ok",
        "restored_files": restored,
        "services_restarted": services_restarted,
        "errors": errors,
        "manifest_version": manifest.get("version", "unknown"),
    }


# ---------------------------------------------------------------------------
# List backups
# ---------------------------------------------------------------------------

def list_backups() -> dict:
    """Return a list of available backup files with metadata."""
    _ensure_dir(BACKUP_DIR)
    backups = []
    for f in sorted(BACKUP_DIR.glob(f"{BACKUP_PREFIX}-*.tar.gz"), reverse=True):
        stat = f.stat()
        backups.append({
            "id": f.stem.replace(f"{BACKUP_PREFIX}-", ""),
            "filename": f.name,
            "path": str(f),
            "size_bytes": stat.st_size,
            "created_at": datetime.fromtimestamp(stat.st_mtime).isoformat(),
        })
    return {"status": "ok", "backups": backups, "count": len(backups)}


# ---------------------------------------------------------------------------
# Delete a backup
# ---------------------------------------------------------------------------

def delete_backup(backup_id: str) -> dict:
    """Delete a backup by its id (timestamp portion) or filename."""
    _ensure_dir(BACKUP_DIR)

    # Try matching by id first
    pattern = f"{BACKUP_PREFIX}-{backup_id}.tar.gz"
    target = BACKUP_DIR / pattern

    if not target.is_file():
        # Try matching by full filename
        target = BACKUP_DIR / backup_id
        if not target.is_file():
            return {"error": f"Backup not found: {backup_id}"}

    target.unlink()
    return {"status": "ok", "message": f"Deleted {target.name}"}


# ---------------------------------------------------------------------------
# Introspect: show what would be backed up
# ---------------------------------------------------------------------------

def inspect_backup(backup_path: Path) -> dict:
    """Show contents and manifest of an existing backup archive."""
    if not backup_path.is_file():
        return {"error": f"File not found: {backup_path}"}
    if not tarfile.is_tarfile(backup_path):
        return {"error": "Not a valid tar archive"}

    with tarfile.open(backup_path, "r:gz") as tar:
        members = tar.getnames()

    manifest = {}
    try:
        with tarfile.open(backup_path, "r:gz") as tar:
            f = tar.extractfile("manifest.json")
            if f:
                manifest = json.loads(f.read())
    except Exception:
        pass

    return {
        "status": "ok",
        "file": str(backup_path),
        "size_bytes": backup_path.stat().st_size,
        "members": members,
        "manifest": manifest,
    }


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _get_hostname() -> str:
    try:
        return subprocess.check_output(["hostname"], text=True).strip()
    except Exception:
        return "unknown"


def _prune_backups():
    """Remove oldest backups beyond MAX_BACKUPS."""
    backups = sorted(
        BACKUP_DIR.glob(f"{BACKUP_PREFIX}-*.tar.gz"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )
    for old in backups[MAX_BACKUPS:]:
        try:
            old.unlink()
        except Exception:
            pass


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="VectorOS Backup Manager")
    sub = parser.add_subparsers(dest="command", required=True)

    # export
    exp = sub.add_parser("export", help="Create a full system backup")
    exp.add_argument("--output", type=str, default=None, help="Output file path")

    # restore
    res = sub.add_parser("restore", help="Restore from backup")
    res.add_argument("backup_file", type=str, help="Path to backup tar.gz")

    # list
    sub.add_parser("list", help="List available backups")

    # delete
    del_p = sub.add_parser("delete", help="Delete a backup")
    del_p.add_argument("backup_id", type=str, help="Backup ID or filename")

    # inspect
    insp = sub.add_parser("inspect", help="Show backup contents")
    insp.add_argument("backup_file", type=str, help="Path to backup tar.gz")

    args = parser.parse_args()

    try:
        if args.command == "export":
            out = Path(args.output) if args.output else None
            result = export_backup(out)
        elif args.command == "restore":
            result = restore_backup(Path(args.backup_file))
        elif args.command == "list":
            result = list_backups()
        elif args.command == "delete":
            result = delete_backup(args.backup_id)
        elif args.command == "inspect":
            result = inspect_backup(Path(args.backup_file))
        else:
            result = {"error": f"Unknown command: {args.command}"}

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
