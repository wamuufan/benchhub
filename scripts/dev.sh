#!/bin/bash
set -e

# Navigate to project root directory
cd "$(dirname "$0")/.." || exit 1

[ -d "$HOME/.cargo/bin" ] && export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: 'cargo' is not installed or not in PATH." >&2
    exit 1
fi

if ! command -v cargo-tauri >/dev/null 2>&1; then
    echo "Error: 'cargo-tauri' is not installed or not in PATH." >&2
    echo "Please run scripts/install-tools.sh to install required tools." >&2
    exit 1
fi

if ! command -v trunk >/dev/null 2>&1; then
    echo "Error: 'trunk' is not installed or not in PATH." >&2
    echo "Please run scripts/install-tools.sh to install required tools." >&2
    exit 1
fi

SOCK_PATH="${BENCHHUB_SOCKET_PATH:-}"
if [ -z "$SOCK_PATH" ]; then
    if [ -n "${XDG_RUNTIME_DIR:-}" ]; then
        SOCK_PATH="$XDG_RUNTIME_DIR/benchhubd.sock"
    else
        SOCK_PATH="/tmp/benchhub-$(id -u)/benchhubd.sock"
    fi
fi

if [ -S "$SOCK_PATH" ]; then
    echo "Warning: Socket $SOCK_PATH already exists."
    echo "If an older benchhubd process is still listening, stop it first so debug daemon edits take effect."
fi

echo "=== Building benchhubd Daemon (Debug) ==="
cargo build -p benchhub-daemon

echo "=== Starting BenchHub Dev Mode (Tauri + Trunk) ==="
cd crates/benchhub-gui
exec cargo tauri dev "$@"
