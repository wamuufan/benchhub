#!/bin/bash
set -e

# Go to project root
cd "$(dirname "$0")/.." || exit 1

echo "Building BenchHub (Native Release)..."
cargo build --release

echo "Preparing distribution package..."
DIST_DIR="benchhub-dist/benchhub"
rm -rf benchhub-dist
mkdir -p "$DIST_DIR/libs"

# Copy binary
cp target/release/benchhub "$DIST_DIR/"
cp -r benchmarks "$DIST_DIR/"

# Copy libssl_shim.so
SSL_SHIM=$(find target/release/build -name "libssl_shim.so" | head -n 1)
if [ -n "$SSL_SHIM" ]; then
    cp "$SSL_SHIM" "$DIST_DIR/libs/"
    echo "Included libssl_shim.so"
else
    echo "Warning: libssl_shim.so not found! The engine might fail to intercept SSL."
fi

# Create run.sh for the distributed package
cat << 'INNER_EOF' > "$DIST_DIR/run.sh"
#!/bin/bash
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
export LD_LIBRARY_PATH="$DIR/libs:$LD_LIBRARY_PATH"
exec "$DIR/benchhub" "$@"
INNER_EOF
chmod +x "$DIST_DIR/run.sh"

echo "Creating tarball..."
tar -czvf benchhub-native-linux.tar.gz -C benchhub-dist benchhub
rm -rf benchhub-dist

echo "Done! Produced benchhub-native-linux.tar.gz"
