#!/usr/bin/env bash
# Start the Grandma Launcher admin web server
# Usage: ./admin-start.sh [port]
#   port: override default port from settings.json (default: 8080)
set -euo pipefail

INSTALL_DIR="/media/fat/grandma_launcher"
BIN="$INSTALL_DIR/bin/grandma-admin"
PIDFILE="$INSTALL_DIR/admin.pid"

if [ ! -f "$BIN" ]; then
    echo "ERROR: grandma-admin not found at $BIN"
    echo "Run install.sh first."
    exit 1
fi

if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
    echo "Admin server is already running (PID $(cat "$PIDFILE"))"
    echo "Stop it first: ./admin-stop.sh"
    exit 1
fi

setsid "$BIN" "$INSTALL_DIR" </dev/null >/dev/null 2>&1 &
echo $! > "$PIDFILE"

PORT=$(grep -o '"admin_port"[[:space:]]*:[[:space:]]*[0-9]*' "$INSTALL_DIR/settings.json" 2>/dev/null | grep -o '[0-9]*$' || echo "8080")
IP=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "<mister-ip>")

echo "Admin server started (PID $(cat "$PIDFILE"))"
echo "Open in your browser: http://${IP}:${PORT}"
echo "Stop with: ./admin-stop.sh"
