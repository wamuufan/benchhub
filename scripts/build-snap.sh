#!/bin/bash
set -e
cd "$(dirname "$0")/.." || exit 1

echo "=== BenchHub Snapcraft Derleme Hazırlığı ==="

SNAPCRAFT_BIN="$(command -v snapcraft 2>/dev/null || echo "/snap/bin/snapcraft")"
LXC_BIN="$(command -v lxc 2>/dev/null || echo "/snap/bin/lxc")"

# 1. snapcraft kontrolü
if [ ! -x "$SNAPCRAFT_BIN" ]; then
    echo "Hata: 'snapcraft' bulunamadı. Lütfen 'sudo snap install snapcraft --classic' komutunu çalıştırın."
    exit 1
fi

# 2. LXD depolama havuzu kontrolü ve otomatik oluşturma
if ! "$LXC_BIN" storage show default &>/dev/null; then
    echo "LXD varsayılan depolama havuzu (default dir) oluşturuluyor..."
    "$LXC_BIN" storage create default dir || true
fi

# 3. LXD default profilinde kök disk kontrolü
if ! "$LXC_BIN" profile device show default 2>/dev/null | grep -q "path: /"; then
    echo "LXD default profiline kök disk ekleniyor..."
    "$LXC_BIN" profile device add default root disk path=/ pool=default || true
fi

# 4. LXD ağ köprüsü kontrolü ve otomatik oluşturma (openSUSE firewalld docker bölgesini otomatik ACCEPT eder)
if ! "$LXC_BIN" network show docker0 &>/dev/null; then
    echo "LXD ağ köprüsü (docker0) oluşturuluyor..."
    "$LXC_BIN" network create docker0 || true
fi

# 5. LXD default profilinde ağ aygıtı kontrolü
if ! "$LXC_BIN" profile device show default 2>/dev/null | grep -q "network: docker0"; then
    echo "LXD default profiline eth0 ağı ekleniyor..."
    "$LXC_BIN" profile device add default eth0 nic network=docker0 name=eth0 2>/dev/null || \
    "$LXC_BIN" profile device set default eth0 network=docker0 || true
fi


echo "Snapcraft derlemesi başlatılıyor..."
sg lxd -c "$SNAPCRAFT_BIN pack --use-lxd $*"
