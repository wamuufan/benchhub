#!/bin/bash
set -e

# Navigate to project root directory
cd "$(dirname "$0")/.." || exit 1

echo "=== BenchHub Clean Script ==="
echo "1. Cleaning Cargo target directories..."
cargo clean

echo "2. Cleaning GUI dist and Trunk artifacts..."
rm -rf crates/benchhub-gui/dist

echo "3. Cleaning packaging, temporary logs and export files..."
rm -rf benchhub-dist build-dir repo parts stage prime .snapcraft .flatpak-builder temp tmp
rm -f benchhub-native-linux.tar.gz benchhub.flatpak *.snap
rm -f benchhub_history_*.csv benchhub_comparison_*.png benchhub_comparison_*.svg

echo "=== Clean Completed Successfully ==="
