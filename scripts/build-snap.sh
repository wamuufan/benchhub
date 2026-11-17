#!/bin/bash
set -e
cd "$(dirname "$0")/.." || exit 1

echo "=== BenchHub Snapcraft Build Setup ==="

SNAPCRAFT_BIN="$(command -v snapcraft 2>/dev/null || echo "/snap/bin/snapcraft")"
LXC_BIN="$(command -v lxc 2>/dev/null || echo "/snap/bin/lxc")"

# 1. Verify snapcraft and lxc installations
if [ ! -x "$SNAPCRAFT_BIN" ]; then
    echo "Error: 'snapcraft' not found. Please run 'sudo snap install snapcraft --classic'." >&2
    exit 1
fi

if [ ! -x "$LXC_BIN" ]; then
    echo "Error: 'lxc' not found. Please install and initialize LXD first." >&2
    exit 1
fi

# 2. Ensure LXD default storage pool exists
if ! "$LXC_BIN" storage show default &>/dev/null; then
    echo "Creating LXD default storage pool (default dir)..."
    "$LXC_BIN" storage create default dir || true
fi

# 3. Ensure LXD default profile has a root disk device
if ! "$LXC_BIN" profile device show default 2>/dev/null | grep -q "path: /"; then
    echo "Adding root disk device to LXD default profile..."
    "$LXC_BIN" profile device add default root disk path=/ pool=default || true
fi

# 4. Ensure LXD network bridge exists
if ! "$LXC_BIN" network show docker0 &>/dev/null; then
    echo "Creating LXD network bridge (docker0)..."
    "$LXC_BIN" network create docker0 || true
fi

# 5. Ensure LXD default profile has a network interface attached
if ! "$LXC_BIN" profile device show default 2>/dev/null | grep -q "network: docker0"; then
    echo "Adding eth0 network device to LXD default profile..."
    "$LXC_BIN" profile device add default eth0 nic network=docker0 name=eth0 2>/dev/null || \
    "$LXC_BIN" profile device set default eth0 network=docker0 || true
fi

if [ ! -f target/release/benchhubd ] || [ ! -f target/release/benchhub-desktop ]; then
    echo "Prebuilt binaries not found; running ./scripts/build.sh first..."
    bash scripts/build.sh
fi

echo "Starting Snapcraft build..."
EXTRA_ARGS="$*"
sg lxd -c "\"$SNAPCRAFT_BIN\" pack --use-lxd${EXTRA_ARGS:+ $EXTRA_ARGS}"

SNAP_FILE="$(ls -1t benchhub_*.snap 2>/dev/null | head -n 1 || true)"
if [ -n "$SNAP_FILE" ]; then
    mv -f "$SNAP_FILE" benchhub-linux-x86_64.snap
    sha256sum benchhub-linux-x86_64.snap > benchhub-linux-x86_64.snap.sha256
    echo "Done! Generated benchhub-linux-x86_64.snap and benchhub-linux-x86_64.snap.sha256"
fi
