#!/bin/bash
set -e

# Go to project root
cd "$(dirname "$0")/.." || exit 1

echo "=== BenchHub Temizlik Betiği ==="
echo "1. Cargo derleme kalıntıları (target/) temizleniyor..."
cargo clean

echo "2. Paketleme ve dağıtım kalıntıları temizleniyor..."
rm -rf benchhub-dist build-dir repo parts stage prime .snapcraft .flatpak-builder
rm -f benchhub-native-linux.tar.gz benchhub.flatpak *.snap

echo "3. Geçici log ve test dizinleri temizleniyor..."
rm -rf temp tmp
rm -f benchhub_history_*.csv benchhub_comparison_*.png benchhub_comparison_*.svg

echo "=== Temizlik Başarıyla Tamamlandı! ==="
