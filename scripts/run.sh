#!/bin/bash
set -e

# Navigate to project root directory
cd "$(dirname "$0")/.." || exit 1

DAEMON_BIN="target/release/benchhubd"
TARGET_BIN="target/release/benchhub-desktop"

# If binaries do not exist or --build flag is explicitly passed, run the build script first
if [ ! -f "$TARGET_BIN" ] || [ ! -f "$DAEMON_BIN" ] || [ "$1" = "--build" ]; then
    if [ "$1" = "--build" ]; then
        shift
    fi
    echo "=== Binary not found or --build requested, building first... ==="
    bash scripts/build.sh
fi

echo "=== Running BenchHub Desktop ==="
exec "./$TARGET_BIN" "$@"
