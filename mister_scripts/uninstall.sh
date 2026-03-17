#!/bin/bash
set -e

INSTALL_DIR="/media/fat/grandma_launcher"
BACKUP_DIR="/media/fat/grandma_launcher_backup"
STARTUP="/media/fat/linux/user-startup.sh"
KILL_SWITCH="/media/fat/grandma_launcher.disabled"
FORCE=false

if [ "$1" = "-f" ]; then
    FORCE=true
fi

echo "=== Grandma Launcher Uninstaller ==="

# Check if installed
if [ ! -d "$INSTALL_DIR" ]; then
    echo "Grandma Launcher is not installed ($INSTALL_DIR does not exist)."
    exit 0
fi

# Kill running processes
echo "Stopping running processes..."
killall grandma-supervisor grandma-launcher grandma-splash grandma-admin 2>/dev/null || true
sleep 1

# Remove startup hook
if [ -f "$STARTUP" ] && grep -q "grandma-supervisor" "$STARTUP"; then
    echo "Removing startup hook..."
    sed -i '/grandma-supervisor/d' "$STARTUP"
    echo "  Removed grandma-supervisor from user-startup.sh"
else
    echo "  No startup hook found"
fi

# Handle user data
if [ "$FORCE" = true ]; then
    echo "Force mode: removing everything..."
else
    HAVE_DATA=false
    if [ -f "$INSTALL_DIR/games.json" ] || [ -f "$INSTALL_DIR/settings.json" ]; then
        HAVE_DATA=true
    fi

    if [ "$HAVE_DATA" = true ]; then
        echo ""
        echo "Found user configuration files (games.json, settings.json)."
        printf "Back up configuration before removing? [Y/n] "
        read -r REPLY
        if [ "$REPLY" != "n" ] && [ "$REPLY" != "N" ]; then
            mkdir -p "$BACKUP_DIR"
            [ -f "$INSTALL_DIR/games.json" ] && cp "$INSTALL_DIR/games.json" "$BACKUP_DIR/" && echo "  Backed up games.json"
            [ -f "$INSTALL_DIR/settings.json" ] && cp "$INSTALL_DIR/settings.json" "$BACKUP_DIR/" && echo "  Backed up settings.json"
            echo "  Backup saved to $BACKUP_DIR/"
        fi
    fi
fi

# Remove install directory
echo "Removing $INSTALL_DIR..."
rm -rf "$INSTALL_DIR"
echo "  Removed"

# Remove kill switch if present
if [ -f "$KILL_SWITCH" ]; then
    rm -f "$KILL_SWITCH"
    echo "  Removed kill switch file"
fi

echo ""
echo "=== Uninstall complete ==="
echo "Reboot to return to the standard MiSTer menu."
if [ -d "$BACKUP_DIR" ]; then
    echo "Your configuration was backed up to $BACKUP_DIR/"
fi
