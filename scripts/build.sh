#!/bin/bash
set -e

# Go to project root
cd "$(dirname "$0")/.." || exit 1

echo "=== BenchHub Derleme Betiği (Release) ==="
cargo build --release

echo "Derleme tamamlandı: target/release/benchhub"
