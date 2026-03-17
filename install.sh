#!/bin/bash
set -e

INSTALL_DIR="/media/fat/grandma_launcher"
BIN_DIR="$INSTALL_DIR/bin"
STARTUP="/media/fat/linux/user-startup.sh"
MISTER_INI="/media/fat/MiSTer.ini"
STARTUP_LINE="$BIN_DIR/grandma-supervisor $INSTALL_DIR &"

echo "=== Grandma Launcher Installer ==="

# Create directories
echo "Creating directories..."
mkdir -p "$BIN_DIR"
mkdir -p "$INSTALL_DIR/assets/boxart"
mkdir -p "$INSTALL_DIR/mgls"

# Copy binaries from same directory as install script
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
echo "Copying binaries..."
for bin in grandma-supervisor grandma-splash grandma-launcher grandma-admin; do
    if [ -f "$SCRIPT_DIR/$bin" ]; then
        cp "$SCRIPT_DIR/$bin" "$BIN_DIR/$bin"
        chmod +x "$BIN_DIR/$bin"
        echo "  Installed $bin"
    else
        echo "  WARNING: $bin not found in $SCRIPT_DIR"
    fi
done

# Copy assets
if [ -f "$SCRIPT_DIR/DejaVuSans.ttf" ]; then
    cp "$SCRIPT_DIR/DejaVuSans.ttf" "$INSTALL_DIR/assets/font.ttf"
    echo "  Installed font"
fi

# Create default settings if not present
if [ ! -f "$INSTALL_DIR/settings.json" ]; then
    cat > "$INSTALL_DIR/settings.json" << 'SETTINGS_EOF'
{
  "schema": 1,
  "title": "GAME TIME!",
  "boot_delay_seconds": 3,
  "admin_server": false,
  "admin_port": 8080,
  "columns": 3
}
SETTINGS_EOF
    echo "Created default settings.json"
fi

# Create empty games list if not present
if [ ! -f "$INSTALL_DIR/games.json" ]; then
    cat > "$INSTALL_DIR/games.json" << 'GAMES_EOF'
{
  "schema": 1,
  "games": []
}
GAMES_EOF
    echo "Created empty games.json"
fi

# Back up user-startup.sh
if [ -f "$STARTUP" ]; then
    cp "$STARTUP" "${STARTUP}.bak"
    echo "Backed up user-startup.sh"
fi

# Add to user-startup.sh (idempotent)
if ! grep -q "grandma-supervisor" "$STARTUP" 2>/dev/null; then
    echo "$STARTUP_LINE" >> "$STARTUP"
    echo "Added grandma-supervisor to user-startup.sh"
else
    echo "grandma-supervisor already in user-startup.sh"
fi

# Check fb_terminal=1 in MiSTer.ini
if [ -f "$MISTER_INI" ]; then
    if grep -q "^fb_terminal=1" "$MISTER_INI"; then
        echo "fb_terminal=1 already set"
    elif grep -q "^fb_terminal=" "$MISTER_INI"; then
        echo "WARNING: fb_terminal is set but not to 1. Please set fb_terminal=1 in MiSTer.ini"
    else
        echo "fb_terminal=1" >> "$MISTER_INI"
        echo "Added fb_terminal=1 to MiSTer.ini"
    fi
fi

# Check MiSTer_cmd
if [ -e "/dev/MiSTer_cmd" ]; then
    echo "/dev/MiSTer_cmd exists"
else
    echo "WARNING: /dev/MiSTer_cmd not found (normal if not running on MiSTer)"
fi

echo ""
echo "=== Installation complete ==="
echo "To add games: ssh into MiSTer, run: $BIN_DIR/grandma-admin $INSTALL_DIR"
echo "Then open http://<mister-ip>:8080 in your browser"
echo "Reboot to start the launcher automatically."
