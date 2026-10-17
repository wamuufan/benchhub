#!/bin/bash
set -e

# Proje ana dizinine geç
cd "$(dirname "$0")" || exit 1

# BenchHub'ı optimize release profilinde çalıştır
exec cargo run --release "$@"
