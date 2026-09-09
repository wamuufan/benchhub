#!/bin/bash
set -e

# Proje kök dizinine geç
cd "$(dirname "$0")/.." || exit 1

echo "Flatpak derlemesi başlatılıyor (Local Repo)..."
# Uygulamayı derler ve 'repo' adında yerel bir flatpak deposuna çıkarır
flatpak-builder --repo=repo --force-clean build-dir packaging/flatpak/org.benchhub.BenchHub.yml

echo "Dağıtılabilir .flatpak paketi (bundle) üretiliyor..."
# Depodaki dosyayı alıp tek parça bir kurulabilir paket haline getirir
flatpak build-bundle repo benchhub.flatpak org.benchhub.BenchHub

# Geçici derleme dosyalarını temizle (isteğe bağlı ama temizlik iyidir)
rm -rf build-dir repo

echo "İşlem Tamamlandı! -> benchhub.flatpak dosyası proje ana dizininde oluşturuldu."
