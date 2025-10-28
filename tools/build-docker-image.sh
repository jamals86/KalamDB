#!/usr/bin/env bash

# Build the KalamDB cross-compilation Docker image
# This only needs to be run once (or when Dockerfile.builder changes)

set -e

DOCKER_IMAGE="kalamdb-builder"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Building KalamDB cross-compilation Docker image..."
echo "This will take 5-10 minutes on first run."
echo ""

cd "$SCRIPT_DIR"
docker build -t "$DOCKER_IMAGE" -f Dockerfile.builder .

echo ""
echo "✓ Docker image built successfully!"
echo ""
echo "You can now run:"
echo "  ./tools/release-multiplatform.sh v0.1.0"
