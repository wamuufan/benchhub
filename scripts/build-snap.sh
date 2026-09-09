#!/bin/bash
set -e
cd "$(dirname "$0")/.." || exit 1

echo "Snapcraft derlemesi başlatılıyor..."
/snap/bin/snapcraft pack --use-lxd
