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

PREBUILT_DIR="target/flatpak-prebuilt"
mkdir -p "$PREBUILT_DIR"

if [ ! -x "$PREBUILT_DIR/c-ray" ] || [ ! -f "$PREBUILT_DIR/sphfract" ]; then
    echo "Prebuilding c-ray for Flatpak..."
    TMP_CRAY="$(mktemp -d)"
    curl -fsSL "https://github.com/jtsiomb/c-ray/releases/download/c-ray-1.1/c-ray-1.1.tar.gz" -o "$TMP_CRAY/c-ray.tar.gz"
    tar -xzf "$TMP_CRAY/c-ray.tar.gz" --strip-components=1 -C "$TMP_CRAY"
    gcc -O3 -pthread "$TMP_CRAY/c-ray-mt.c" -lm -o "$PREBUILT_DIR/c-ray"
    cp "$TMP_CRAY/sphfract" "$PREBUILT_DIR/sphfract"
    rm -rf "$TMP_CRAY"
fi

if [ ! -x "$PREBUILT_DIR/stream-standard" ] || [ ! -x "$PREBUILT_DIR/stream-compact" ] || [ ! -x "$PREBUILT_DIR/stream-extended" ]; then
    echo "Prebuilding STREAM variants for Flatpak..."
    TMP_STREAM="$(mktemp -d)"
    curl -fsSL "https://raw.githubusercontent.com/jeffhammond/STREAM/master/stream.c" -o "$TMP_STREAM/stream.c" \
        || curl -fsSL "https://www.cs.virginia.edu/stream/FTP/Code/stream.c" -o "$TMP_STREAM/stream.c"
    gcc -O3 -fopenmp -DSTREAM_ARRAY_SIZE=80000000 -DNTIMES=10 "$TMP_STREAM/stream.c" -o "$PREBUILT_DIR/stream-standard"
    gcc -O3 -fopenmp -DSTREAM_ARRAY_SIZE=20000000 -DNTIMES=10 "$TMP_STREAM/stream.c" -o "$PREBUILT_DIR/stream-compact"
    gcc -O3 -fopenmp -mcmodel=medium -DSTREAM_ARRAY_SIZE=160000000 -DNTIMES=10 "$TMP_STREAM/stream.c" -o "$PREBUILT_DIR/stream-extended"
    rm -rf "$TMP_STREAM"
fi

if [ ! -s "$PREBUILT_DIR/libGLU.so.1" ]; then
    echo "Preparing libGLU.so.1 for Flatpak..."
    if [ -f /usr/lib/x86_64-linux-gnu/libGLU.so.1 ]; then
        cp -L /usr/lib/x86_64-linux-gnu/libGLU.so.1 "$PREBUILT_DIR/libGLU.so.1"
    elif [ -f /usr/lib64/libGLU.so.1 ]; then
        cp -L /usr/lib64/libGLU.so.1 "$PREBUILT_DIR/libGLU.so.1"
    else
        TMP_GLU="$(mktemp -d)"
        curl -fsSL "https://mirrors.edge.kernel.org/ubuntu/pool/main/libg/libglu/libglu1-mesa_9.0.2-1.1build1_amd64.deb" -o "$TMP_GLU/glu.deb"
        ar p "$TMP_GLU/glu.deb" data.tar.zst | tar --zstd -xf - ./usr/lib/x86_64-linux-gnu/libGLU.so.1.3.1 -O > "$PREBUILT_DIR/libGLU.so.1"
        rm -rf "$TMP_GLU"
    fi
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
