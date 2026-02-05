#!/bin/bash
# build-all.sh - Build release binaries for all platforms

set -e

mkdir -p dist

echo "Building for macOS ARM64..."
cargo build --release
cp target/release/magento-static-deploy dist/magento-static-deploy-darwin-arm64

echo "Building for Linux x86_64..."
docker run --rm -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/dist/magento-static-deploy-linux-amd64"

echo "Building for Linux ARM64..."
docker run --rm --platform linux/arm64 -v "$(pwd):/app" -w /app rust:1.83-bookworm \
  sh -c "cargo build --release && cp target/release/magento-static-deploy /app/dist/magento-static-deploy-linux-arm64"

echo ""
echo "Done! Binaries:"
ls -lh dist/
