#!/usr/bin/env bash
set -euo pipefail

# Navigate to project root directory
cd "$(dirname "$0")/.." || exit 1

if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "Error: 'flatpak-builder' is not installed or not in PATH." >&2
    exit 1
fi

if [ ! -f target/release/benchhubd ] || [ ! -f target/release/benchhub-desktop ]; then
    echo "Prebuilt binaries not found; running ./scripts/build.sh first..."
    bash scripts/build.sh
fi

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo || true

echo "Starting Flatpak build (local repository)..."
flatpak-builder --user --install-deps-from=flathub --repo=repo --force-clean build-dir packaging/flatpak/org.benchhub.BenchHub.yml

echo "Generating redistributable .flatpak bundle..."
flatpak build-bundle repo benchhub-linux-x86_64.flatpak org.benchhub.BenchHub
sha256sum benchhub-linux-x86_64.flatpak > benchhub-linux-x86_64.flatpak.sha256

# Clean temporary build artifacts
rm -rf build-dir repo

echo "Done! Generated benchhub-linux-x86_64.flatpak and benchhub-linux-x86_64.flatpak.sha256 in the project root directory."
