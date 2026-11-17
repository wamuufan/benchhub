#!/usr/bin/env bash
set -euo pipefail

echo "[INFO] Stopping any running BenchHub processes..."
pkill -u "$(id -u)" -f '/benchhub-desktop($| )' 2>/dev/null || pkill -u "$(id -u)" -x benchhub-deskto 2>/dev/null || true
pkill -u "$(id -u)" -x benchhubd 2>/dev/null || true

BIN_DIR="${HOME}/.local/bin/Benchhub"
SYMLINK_PATH="${HOME}/.local/bin/benchhub"
DESKTOP_FILE="${HOME}/.local/share/applications/benchhub.desktop"

if [ -L "${SYMLINK_PATH}" ] || [ -e "${SYMLINK_PATH}" ]; then
    echo "[INFO] Removing symlink ${SYMLINK_PATH}..."
    rm -f "${SYMLINK_PATH}"
fi

if [ -f "${DESKTOP_FILE}" ]; then
    echo "[INFO] Removing desktop entry ${DESKTOP_FILE}..."
    rm -f "${DESKTOP_FILE}"
fi

if [ -d "${BIN_DIR}" ]; then
    echo "[INFO] Removing installation directory ${BIN_DIR}..."
    rm -rf "${BIN_DIR}"
fi

echo "[SUCCESS] BenchHub has been uninstalled."
echo "[INFO] User benchmark history and runners in ~/.local/share/benchhub are preserved unless removed manually."
