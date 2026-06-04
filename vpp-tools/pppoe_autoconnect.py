#!/usr/bin/env python3
"""VectorOS PPPoE Auto-Connect Manager

Monitors PPPoE connection status and automatically reconnects on disconnect
with exponential backoff, health checks, and DNS refresh on reconnect.
"""

import sys
import json
import time
import signal
import argparse
import subprocess
import logging
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, '/usr/lib/python3/dist-packages')
try:
    from vpp_papi import VPPApiClient
    HAS_VPP_API = True
except ImportError:
    HAS_VPP_API = False

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s [%(levelname)s] %(message)s',
    datefmt='%Y-%m-%d %H:%M:%S',
)
log = logging.getLogger('pppoe-autoconnect')

# ---------------------------------------------------------------------------
# State file for persisting auto-connect configuration and history
# ---------------------------------------------------------------------------
STATE_DIR = Path('/var/lib/vectoros')
STATE_FILE = STATE_DIR / 'pppoe_autoconnect.json'

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
DEFAULT_CHECK_INTERVAL = 10          # seconds between status checks
DEFAULT_RETRY_INTERVAL = 5          # initial retry interval (seconds)
DEFAULT_MAX_RETRIES = 0             # 0 = infinite retries
DEFAULT_BACKOFF_FACTOR = 2.0        # exponential backoff multiplier
DEFAULT_MAX_RETRY_INTERVAL = 300    # cap at 5 minutes
DEFAULT_HEALTH_CHECK_INTERVAL = 60  # seconds between health pings

PPPOE_STATE_SESSION = 3             # PPPoE session established


class AutoConnectManager:
    """Manages PPPoE auto-reconnection with exponential backoff."""

    def __init__(self, config=None):
        self.config = config or {}
        self.running = False
        self.enabled = self.config.get('enabled', False)

        # Retry parameters
        self.max_retries = self.config.get('max_retries', DEFAULT_MAX_RETRIES)
        self.retry_interval = self.config.get('retry_interval', DEFAULT_RETRY_INTERVAL)
        self.backoff_factor = self.config.get('backoff_factor', DEFAULT_BACKOFF_FACTOR)
        self.max_retry_interval = self.config.get('max_retry_interval', DEFAULT_MAX_RETRY_INTERVAL)
        self.check_interval = self.config.get('check_interval', DEFAULT_CHECK_INTERVAL)
        self.health_check_interval = self.config.get('health_check_interval', DEFAULT_HEALTH_CHECK_INTERVAL)

        # Runtime state
        self.current_retry_interval = self.retry_interval
        self.consecutive_failures = 0
        self.total_reconnects = 0
        self.last_connect_time = None
        self.last_disconnect_time = None
        self.last_health_check = None
        self.connection_history = []  # recent events (capped at 100)
        self.status = 'idle'  # idle, connecting, connected, retrying, disabled

        # Load persisted state
        self._load_state()

    def _load_state(self):
        """Load persisted state from disk."""
        if STATE_FILE.exists():
            try:
                data = json.loads(STATE_FILE.read_text())
                self.total_reconnects = data.get('total_reconnects', 0)
                self.connection_history = data.get('connection_history', [])[-100:]
                log.info("Loaded persisted state: %d reconnects", self.total_reconnects)
            except Exception as e:
                log.warning("Failed to load state: %s", e)

    def _save_state(self):
        """Persist state to disk."""
        STATE_DIR.mkdir(parents=True, exist_ok=True)
        data = {
            'total_reconnects': self.total_reconnects,
            'connection_history': self.connection_history[-100:],
        }
        try:
            STATE_FILE.write_text(json.dumps(data, indent=2))
        except Exception as e:
            log.warning("Failed to save state: %s", e)

    def _add_history(self, event_type, message):
        """Add an event to the connection history."""
        entry = {
            'timestamp': datetime.now(timezone.utc).isoformat(),
            'type': event_type,
            'message': message,
        }
        self.connection_history.append(entry)
        # Keep only last 100 entries
        if len(self.connection_history) > 100:
            self.connection_history = self.connection_history[-100:]
        self._save_state()

    def _connect_via_pppoe_manager(self, sw_if_index, username, password, mtu=1492, mru=1492):
        """Connect using the existing pppoe_manager.py script."""
        cmd = [
            'python3', '/root/VectorOS/vpp-tools/pppoe_manager.py',
            'create',
            '--sw-if-index', str(sw_if_index),
            '--username', username,
            '--password', password,
            '--mtu', str(mtu),
            '--mru', str(mru),
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode != 0:
            return {'error': f'Command failed: {result.stderr}'}
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return {'error': f'Invalid output: {result.stdout}'}

    def _check_status_via_script(self):
        """Check PPPoE status using pppoe_manager.py dump."""
        cmd = ['python3', '/root/VectorOS/vpp-tools/pppoe_manager.py', 'dump']
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            if result.returncode != 0:
                return None
            data = json.loads(result.stdout)
            return data
        except Exception:
            return None

    def _is_connected(self):
        """Check if PPPoE session is active."""
        status = self._check_status_via_script()
        if status is None:
            return False
        if isinstance(status, list):
            for client in status:
                state = client.get('client_state', 0)
                if state == PPPOE_STATE_SESSION:
                    return True
        elif isinstance(status, dict) and 'clients' in status:
            for client in status['clients']:
                state = client.get('client_state', 0)
                if state == PPPOE_STATE_SESSION:
                    return True
        return False

    def _refresh_dns(self):
        """Refresh DNS after reconnection by restarting the DNS service."""
        try:
            subprocess.run(
                ['systemctl', 'restart', 'vectoros-dns'],
                capture_output=True, timeout=10,
            )
            log.info("DNS refreshed after reconnection")
            self._add_history('dns_refresh', 'DNS resolver refreshed')
        except Exception as e:
            log.warning("DNS refresh failed: %s", e)

    def _attempt_connect(self, pppoe_config):
        """Attempt to establish PPPoE connection."""
        self.status = 'connecting'
        self._add_history('connect_attempt', f"Attempt #{self.consecutive_failures + 1}")

        result = self._connect_via_pppoe_manager(
            sw_if_index=pppoe_config.get('sw_if_index', 1),
            username=pppoe_config.get('username', ''),
            password=pppoe_config.get('password', ''),
            mtu=pppoe_config.get('mtu', 1492),
            mru=pppoe_config.get('mru', 1492),
        )

        if 'error' in result:
            self.consecutive_failures += 1
            self.status = 'retrying'
            self._add_history('connect_failed', result['error'])
            log.warning("Connect failed: %s (attempt %d)", result['error'], self.consecutive_failures)
            return False
        else:
            self.consecutive_failures = 0
            self.current_retry_interval = self.retry_interval
            self.total_reconnects += 1
            self.last_connect_time = time.time()
            self.status = 'connected'
            self._add_history('connected', f"Session established (total reconnects: {self.total_reconnects})")
            log.info("PPPoE connected (total reconnects: %d)", self.total_reconnects)
            self._save_state()

            # Refresh DNS on reconnect
            self._refresh_dns()
            return True

    def _get_next_retry_delay(self):
        """Calculate next retry delay with exponential backoff."""
        delay = self.current_retry_interval
        self.current_retry_interval = min(
            self.current_retry_interval * self.backoff_factor,
            self.max_retry_interval,
        )
        return delay

    def run(self, pppoe_config):
        """Main auto-connect loop."""
        if not self.enabled:
            self.status = 'disabled'
            log.info("Auto-connect is disabled")
            return

        self.running = True
        log.info("Starting auto-connect (check_interval=%ds, max_retries=%s)",
                 self.check_interval, self.max_retries or 'infinite')

        while self.running:
            try:
                if self._is_connected():
                    self.status = 'connected'
                    self.last_disconnect_time = None

                    # Health check
                    now = time.time()
                    if (self.last_health_check is None or
                            now - self.last_health_check >= self.health_check_interval):
                        self.last_health_check = now
                        if not self._is_connected():
                            self._add_history('health_check_failed', 'Connection lost during health check')
                            self.last_disconnect_time = now
                            self.current_retry_interval = self.retry_interval
                        else:
                            self._add_history('health_check_ok', 'Connection healthy')

                    time.sleep(self.check_interval)
                    continue

                # Not connected — try to reconnect
                self.last_disconnect_time = time.time()
                self._add_history('disconnected', 'PPPoE session not active')

                if self.max_retries > 0 and self.consecutive_failures >= self.max_retries:
                    self.status = 'failed'
                    self._add_history('max_retries_reached',
                                      f"Max retries ({self.max_retries}) reached, giving up")
                    log.error("Max retries (%d) reached, stopping auto-connect", self.max_retries)
                    break

                self._attempt_connect(pppoe_config)

                delay = self._get_next_retry_delay()
                if self.status != 'connected':
                    log.info("Retrying in %.1f seconds...", delay)
                    self._add_history('retry_scheduled', f"Next retry in {delay:.1f}s")
                    time.sleep(delay)

            except KeyboardInterrupt:
                log.info("Auto-connect interrupted")
                break
            except Exception as e:
                log.error("Auto-connect loop error: %s", e)
                self._add_history('error', str(e))
                time.sleep(self.check_interval)

        self.running = False
        self.status = 'idle' if self.enabled else 'disabled'
        log.info("Auto-connect stopped")

    def stop(self):
        """Signal the auto-connect loop to stop."""
        self.running = False

    def get_status(self):
        """Return current auto-connect status."""
        return {
            'enabled': self.enabled,
            'status': self.status,
            'running': self.running,
            'consecutive_failures': self.consecutive_failures,
            'total_reconnects': self.total_reconnects,
            'current_retry_interval': round(self.current_retry_interval, 1),
            'last_connect_time': self.last_connect_time,
            'last_disconnect_time': self.last_disconnect_time,
            'last_health_check': self.last_health_check,
            'config': {
                'max_retries': self.max_retries,
                'retry_interval': self.retry_interval,
                'backoff_factor': self.backoff_factor,
                'max_retry_interval': self.max_retry_interval,
                'check_interval': self.check_interval,
                'health_check_interval': self.health_check_interval,
            },
        }

    def get_history(self, limit=50):
        """Return recent connection history."""
        return self.connection_history[-limit:]


# ---------------------------------------------------------------------------
# CLI interface for standalone usage / testing
# ---------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(description='VectorOS PPPoE Auto-Connect Manager')
    parser.add_argument('action', choices=['start', 'stop', 'status', 'history', 'configure'])
    parser.add_argument('--enabled', action='store_true', default=False)
    parser.add_argument('--max-retries', type=int, default=0)
    parser.add_argument('--retry-interval', type=int, default=5)
    parser.add_argument('--backoff-factor', type=float, default=2.0)
    parser.add_argument('--max-retry-interval', type=int, default=300)
    parser.add_argument('--check-interval', type=int, default=10)
    parser.add_argument('--health-check-interval', type=int, default=60)
    parser.add_argument('--username', default='')
    parser.add_argument('--password', default='')
    parser.add_argument('--sw-if-index', type=int, default=1)
    parser.add_argument('--mtu', type=int, default=1492)
    parser.add_argument('--mru', type=int, default=1492)

    args = parser.parse_args()

    config = {
        'enabled': args.enabled,
        'max_retries': args.max_retries,
        'retry_interval': args.retry_interval,
        'backoff_factor': args.backoff_factor,
        'max_retry_interval': args.max_retry_interval,
        'check_interval': args.check_interval,
        'health_check_interval': args.health_check_interval,
    }

    manager = AutoConnectManager(config)

    if args.action == 'start':
        manager.enabled = True
        pppoe_config = {
            'username': args.username,
            'password': args.password,
            'sw_if_index': args.sw_if_index,
            'mtu': args.mtu,
            'mru': args.mru,
        }

        # Handle graceful shutdown
        def handle_signal(sig, frame):
            log.info("Received signal %s, stopping...", sig)
            manager.stop()

        signal.signal(signal.SIGTERM, handle_signal)
        signal.signal(signal.SIGINT, handle_signal)

        manager.run(pppoe_config)
        print(json.dumps(manager.get_status()))

    elif args.action == 'stop':
        manager.stop()
        print(json.dumps({'status': 'stopped'}))

    elif args.action == 'status':
        print(json.dumps(manager.get_status()))

    elif args.action == 'history':
        print(json.dumps(manager.get_history()))

    elif args.action == 'configure':
        manager.enabled = args.enabled
        manager.max_retries = args.max_retries
        manager.retry_interval = args.retry_interval
        manager.backoff_factor = args.backoff_factor
        manager.max_retry_interval = args.max_retry_interval
        manager.check_interval = args.check_interval
        manager.health_check_interval = args.health_check_interval
        print(json.dumps({'status': 'configured', **manager.get_status()}))


if __name__ == '__main__':
    main()
