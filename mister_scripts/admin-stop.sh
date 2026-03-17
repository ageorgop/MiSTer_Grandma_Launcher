#!/usr/bin/env bash
# Stop the Grandma Launcher admin web server
set -euo pipefail

INSTALL_DIR="/media/fat/grandma_launcher"
PIDFILE="$INSTALL_DIR/admin.pid"

if [ -f "$PIDFILE" ]; then
    PID=$(cat "$PIDFILE")
    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID"
        echo "Admin server stopped (PID $PID)"
    else
        echo "Admin server was not running (stale PID file)"
    fi
    rm -f "$PIDFILE"
else
    # Fall back to killall
    if killall grandma-admin 2>/dev/null; then
        echo "Admin server stopped"
    else
        echo "Admin server is not running"
    fi
fi
