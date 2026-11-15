#!/bin/bash
set -e

[ -d "$HOME/.cargo/bin" ] && export PATH="$HOME/.cargo/bin:$PATH"

echo "=== Installing BenchHub Frontend & Tauri Build Tools ==="

echo "1. Adding wasm32-unknown-unknown target..."
rustup target add wasm32-unknown-unknown

echo "2. Installing trunk and tauri-cli..."
cargo install trunk tauri-cli --locked

echo "=== Tools Installed Successfully ==="
