#!/usr/bin/env bash
set -euo pipefail

SKIP_DEPS=0

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Install BenchHub into ~/.local/bin/Benchhub and register the desktop entry.

Options:
  --skip-deps    Skip runtime dependency check/installation
  -h, --help     Show this help message and exit
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-deps)
            SKIP_DEPS=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "[ERROR] Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"

# Locate binaries and helper scripts (release archive layout or development tree)
if [[ -d "${SCRIPT_DIR}/Benchhub-bin" ]]; then
    BIN_SRC="${SCRIPT_DIR}/Benchhub-bin"
    HELPER_SRC="${SCRIPT_DIR}/Benchhub-bin"
elif [[ -d "${SCRIPT_DIR}/target/release" && -d "${SCRIPT_DIR}/scripts" ]]; then
    BIN_SRC="${SCRIPT_DIR}/target/release"
    HELPER_SRC="${SCRIPT_DIR}/scripts"
elif [[ -d "${SCRIPT_DIR}/../target/release" ]]; then
    BIN_SRC="${SCRIPT_DIR}/../target/release"
    HELPER_SRC="${SCRIPT_DIR}"
else
    echo "[ERROR] Could not locate Benchhub-bin or target/release directory." >&2
    exit 1
fi

for bin_name in benchhubd benchhub-desktop; do
    if [[ ! -f "${BIN_SRC}/${bin_name}" ]]; then
        echo "[ERROR] Missing required binary: ${BIN_SRC}/${bin_name}" >&2
        exit 1
    fi
done

for script_name in benchhub-launch.sh uninstall.sh; do
    if [[ ! -f "${HELPER_SRC}/${script_name}" ]]; then
        echo "[ERROR] Missing required helper script: ${HELPER_SRC}/${script_name}" >&2
        exit 1
    fi
done

if [[ "${SKIP_DEPS}" -eq 0 ]]; then
    if [[ -f /etc/os-release ]] && grep -qE '^ID="?opensuse-tumbleweed"?$' /etc/os-release; then
        REQUIRED_PKGS=(libwebkit2gtk-4_1-0 libayatana-appindicator3-1 curl tar xz unzip)
        MISSING_PKGS=()
        for pkg in "${REQUIRED_PKGS[@]}"; do
            if ! rpm -q "${pkg}" >/dev/null 2>&1; then
                MISSING_PKGS+=("${pkg}")
            fi
        done

        if [[ ${#MISSING_PKGS[@]} -gt 0 ]]; then
            echo "[INFO] Installing missing runtime dependencies: ${MISSING_PKGS[*]}"
            sudo zypper install -y "${MISSING_PKGS[@]}"
        else
            echo "[INFO] All required runtime dependencies are already installed."
        fi
    else
        echo "[INFO] Non-openSUSE Tumbleweed distribution detected."
        echo "[INFO] Please ensure the runtime dependencies listed in INSTALLATION.md are installed,"
        echo "[INFO] then re-run this installer with: ./install.sh --skip-deps"
        exit 1
    fi
fi

INSTALL_DIR="${HOME}/.local/bin/Benchhub"
SYMLINK_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"

mkdir -p "${INSTALL_DIR}" "${SYMLINK_DIR}" "${APP_DIR}"

echo "[INFO] Installing binaries and helper scripts to ${INSTALL_DIR}..."
install -m 755 "${BIN_SRC}/benchhubd" "${INSTALL_DIR}/benchhubd"
install -m 755 "${BIN_SRC}/benchhub-desktop" "${INSTALL_DIR}/benchhub-desktop"
install -m 755 "${HELPER_SRC}/benchhub-launch.sh" "${INSTALL_DIR}/benchhub-launch.sh"
install -m 755 "${HELPER_SRC}/uninstall.sh" "${INSTALL_DIR}/uninstall.sh"

ln -sf "${INSTALL_DIR}/benchhub-launch.sh" "${SYMLINK_DIR}/benchhub"

cat > "${APP_DIR}/benchhub.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=BenchHub
Comment=Linux Hardware Benchmark & Telemetry Suite
Exec=${INSTALL_DIR}/benchhub-launch.sh
Icon=utilities-system-monitor
Terminal=false
Categories=System;Monitor;Utility;
EOF

echo "[SUCCESS] BenchHub installed successfully!"
echo "[INFO] Launch from your application menu (BenchHub) or run 'benchhub' in your terminal."
echo "[INFO] To uninstall later, run: ${INSTALL_DIR}/uninstall.sh"
