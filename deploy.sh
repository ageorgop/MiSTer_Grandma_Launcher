#!/usr/bin/env bash
# Deploy Grandma Launcher to MiSTer
# Usage: ./deploy.sh [mister-host]
#   mister-host: SSH host (default: "mister", from ~/.ssh/config)
set -euo pipefail

HOST="${1:-mister}"
TARGET="armv7-unknown-linux-musleabihf"
RELEASE_DIR="target/${TARGET}/release"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
STAGING_DIR=$(mktemp -d)
TARBALL="${STAGING_DIR}/grandma-launcher.tar.gz"

trap "rm -rf ${STAGING_DIR}" EXIT

echo "=== Grandma Launcher Deploy ==="
echo "Host: ${HOST}"
echo ""

# Step 1: Cross-compile
echo "Building..."
cross build --target "${TARGET}" --release
echo ""

# Step 2: Stage everything into a flat directory for install.sh
PACK_DIR="${STAGING_DIR}/grandma-launcher"
mkdir -p "${PACK_DIR}"

for bin in grandma-supervisor grandma-splash grandma-launcher grandma-admin; do
    cp "${RELEASE_DIR}/${bin}" "${PACK_DIR}/"
done

for script in mister_scripts/install.sh mister_scripts/uninstall.sh mister_scripts/admin-start.sh mister_scripts/admin-stop.sh; do
    [ -f "${SCRIPT_DIR}/${script}" ] && cp "${SCRIPT_DIR}/${script}" "${PACK_DIR}/"
done

echo "Binary sizes:"
for bin in grandma-supervisor grandma-splash grandma-launcher grandma-admin; do
    size=$(du -h "${PACK_DIR}/${bin}" | cut -f1)
    echo "  ${bin}: ${size}"
done
echo ""

# Step 3: Create tarball
tar czf "${TARBALL}" -C "${STAGING_DIR}" grandma-launcher

# Step 4: Copy tarball and run install
echo "Deploying to ${HOST}..."
ssh "${HOST}" "killall grandma-supervisor grandma-launcher grandma-splash 2>/dev/null || true"
scp -q "${TARBALL}" "${HOST}:/tmp/grandma-launcher.tar.gz"
ssh "${HOST}" 'cd /tmp && tar xzf grandma-launcher.tar.gz && cd grandma-launcher && chmod +x *.sh && ./install.sh && rm -rf /tmp/grandma-launcher /tmp/grandma-launcher.tar.gz && nohup setsid /media/fat/grandma_launcher/bin/grandma-supervisor /media/fat/grandma_launcher </dev/null >/dev/null 2>&1 &'
echo ""

echo "=== Deploy complete ==="
echo "Launcher is running. Will also start automatically on boot."
echo "To uninstall: ssh ${HOST} '/media/fat/grandma_launcher/uninstall.sh'"
echo "To manage games: ssh ${HOST} '/media/fat/grandma_launcher/admin-start.sh'"
