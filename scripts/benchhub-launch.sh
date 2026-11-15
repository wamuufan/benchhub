#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
export PATH="${SCRIPT_DIR}:${PATH}"

exec "${SCRIPT_DIR}/benchhub-desktop" "$@"
