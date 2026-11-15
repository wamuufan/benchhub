#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
cd "${ROOT_DIR}"

[ -d "${HOME}/.cargo/bin" ] && export PATH="${HOME}/.cargo/bin:${PATH}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "[ERROR] 'cargo' is not installed or not in PATH." >&2
    exit 1
fi

if ! command -v trunk >/dev/null 2>&1; then
    echo "[ERROR] 'trunk' is not installed or not in PATH." >&2
    echo "[INFO] Please install trunk (e.g. 'cargo install --locked trunk') or run scripts/install-tools.sh." >&2
    exit 1
fi

echo "=== BenchHub Build Script (Release) ==="

echo "1. Building background daemon binary (benchhubd)..."
cargo build --release -p benchhub-daemon

echo "2. Building Leptos CSR frontend with Trunk..."
(cd "${ROOT_DIR}/crates/benchhub-gui" && trunk build --config Trunk.toml --release)

echo "3. Building Tauri desktop binary (custom-protocol)..."
touch "${ROOT_DIR}/crates/benchhub-gui/src-tauri/build.rs" "${ROOT_DIR}/crates/benchhub-gui/src-tauri/src/lib.rs"
cargo build --release -p benchhub-desktop --features custom-protocol

echo "4. Packaging release archive (benchhub-linux-x86_64.tar.gz)..."
STAGE_ROOT="${ROOT_DIR}/target/package-stage"
PKG_DIR="${STAGE_ROOT}/benchhub-linux-x86_64"
BIN_DIR="${PKG_DIR}/Benchhub-bin"

rm -rf "${STAGE_ROOT}"
mkdir -p "${BIN_DIR}"

cp "${ROOT_DIR}/scripts/install.sh" "${PKG_DIR}/install.sh"
cp "${ROOT_DIR}/INSTALLATION.md" "${PKG_DIR}/INSTALLATION.md"
cp "${ROOT_DIR}/README.md" "${PKG_DIR}/README.md"
cp "${ROOT_DIR}/LICENSE" "${PKG_DIR}/LICENSE"

cp "${ROOT_DIR}/target/release/benchhubd" "${BIN_DIR}/benchhubd"
cp "${ROOT_DIR}/target/release/benchhub-desktop" "${BIN_DIR}/benchhub-desktop"
cp "${ROOT_DIR}/scripts/benchhub-launch.sh" "${BIN_DIR}/benchhub-launch.sh"
cp "${ROOT_DIR}/scripts/uninstall.sh" "${BIN_DIR}/uninstall.sh"

chmod 755 "${PKG_DIR}/install.sh" \
    "${BIN_DIR}/benchhubd" \
    "${BIN_DIR}/benchhub-desktop" \
    "${BIN_DIR}/benchhub-launch.sh" \
    "${BIN_DIR}/uninstall.sh"

ARCHIVE_NAME="benchhub-linux-x86_64.tar.gz"
tar -C "${STAGE_ROOT}" -czf "${ROOT_DIR}/${ARCHIVE_NAME}" "benchhub-linux-x86_64"
(cd "${ROOT_DIR}" && sha256sum "${ARCHIVE_NAME}" > "${ARCHIVE_NAME}.sha256")
rm -rf "${STAGE_ROOT}"

echo "Build and packaging finished:"
echo "  - Daemon  : target/release/benchhubd"
echo "  - Desktop : target/release/benchhub-desktop"
echo "  - Archive : ${ARCHIVE_NAME}"
echo "  - Checksum: ${ARCHIVE_NAME}.sha256"
