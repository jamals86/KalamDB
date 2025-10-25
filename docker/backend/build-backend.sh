#!/usr/bin/env bash
# Build script for KalamDB backend Docker image
# Feature: 006-docker-wasm-examples
# 
# This script builds a complete Docker image containing:
# - kalamdb-server (backend server)
# - kalam-cli (CLI tool)
#
# Usage:
#   ./build-backend.sh [options]
#
# Options:
#   --tag TAG       Set image tag (default: kalamdb:latest)
#   --no-cache      Build without using cache
#   --push          Push to registry after build
#   --help          Show this help message

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IMAGE_TAG="${IMAGE_TAG:-kalamdb:latest}"
NO_CACHE=""
PUSH_IMAGE=false

# ============================================================================
# Functions
# ============================================================================

show_help() {
    cat << EOF
Build KalamDB Backend Docker Image

Usage: $0 [options]

Options:
    --tag TAG       Set image tag (default: kalamdb:latest)
    --no-cache      Build without using cache
    --push          Push to registry after build
    --help          Show this help message

Examples:
    # Build with default tag
    ./build-backend.sh

    # Build with custom tag
    ./build-backend.sh --tag myregistry.com/kalamdb:v1.0.0

    # Build without cache and push
    ./build-backend.sh --no-cache --push --tag myregistry.com/kalamdb:v1.0.0

Environment Variables:
    IMAGE_TAG       Override default image tag
    DOCKER_BUILDKIT Enable BuildKit (recommended, set to 1)

EOF
}

log_info() {
    echo "ℹ️  $*"
}

log_success() {
    echo "✅ $*"
}

log_error() {
    echo "❌ $*" >&2
}

# ============================================================================
# Parse Arguments
# ============================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        --tag)
            IMAGE_TAG="$2"
            shift 2
            ;;
        --no-cache)
            NO_CACHE="--no-cache"
            shift
            ;;
        --push)
            PUSH_IMAGE=true
            shift
            ;;
        --help)
            show_help
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# ============================================================================
# Pre-flight Checks
# ============================================================================

log_info "Pre-flight checks..."

# Check Docker is installed
if ! command -v docker &> /dev/null; then
    log_error "Docker is not installed or not in PATH"
    exit 1
fi

# Check we're in the right directory
if [[ ! -f "$REPO_ROOT/backend/Cargo.toml" ]]; then
    log_error "Cannot find backend/Cargo.toml - are you in the right directory?"
    exit 1
fi

if [[ ! -f "$SCRIPT_DIR/Dockerfile" ]]; then
    log_error "Cannot find Dockerfile at $SCRIPT_DIR/Dockerfile"
    exit 1
fi

log_success "Pre-flight checks passed"

# ============================================================================
# Build Image
# ============================================================================

log_info "Building Docker image..."
log_info "  Tag: $IMAGE_TAG"
log_info "  Context: $REPO_ROOT"
log_info "  Dockerfile: $SCRIPT_DIR/Dockerfile"

# Enable BuildKit for better caching and output
export DOCKER_BUILDKIT=1

# Build the image
if docker build \
    $NO_CACHE \
    --tag "$IMAGE_TAG" \
    --file "$SCRIPT_DIR/Dockerfile" \
    "$REPO_ROOT"; then
    log_success "Image built successfully: $IMAGE_TAG"
else
    log_error "Build failed"
    exit 1
fi

# ============================================================================
# Verify Image
# ============================================================================

log_info "Verifying image..."

# Check image exists
if ! docker image inspect "$IMAGE_TAG" &> /dev/null; then
    log_error "Image not found after build: $IMAGE_TAG"
    exit 1
fi

# Get image size
IMAGE_SIZE=$(docker image inspect "$IMAGE_TAG" --format='{{.Size}}' | awk '{print int($1/1024/1024)"MB"}')
log_info "  Image size: $IMAGE_SIZE"

# Check binaries exist in image
if docker run --rm "$IMAGE_TAG" /usr/local/bin/kalamdb-server --version &> /dev/null; then
    log_success "kalamdb-server binary verified"
else
    log_error "kalamdb-server binary not found or not working"
    exit 1
fi

if docker run --rm "$IMAGE_TAG" /usr/local/bin/kalam-cli --version &> /dev/null; then
    log_success "kalam-cli binary verified"
else
    log_error "kalam-cli binary not found or not working"
    exit 1
fi

# ============================================================================
# Push (Optional)
# ============================================================================

if [[ "$PUSH_IMAGE" == true ]]; then
    log_info "Pushing image to registry..."
    
    if docker push "$IMAGE_TAG"; then
        log_success "Image pushed: $IMAGE_TAG"
    else
        log_error "Push failed"
        exit 1
    fi
fi

# ============================================================================
# Summary
# ============================================================================

cat << EOF

🎉 Build complete!

Image: $IMAGE_TAG
Size:  $IMAGE_SIZE

Next steps:
  1. Start the container:
     cd $SCRIPT_DIR && docker-compose up -d

  2. Create a user:
     docker exec -it kalamdb kalam-cli user create --name "myuser" --role "user"

  3. Test the server:
     curl http://localhost:8080/health

Documentation:
  - Quick Start: $REPO_ROOT/specs/006-docker-wasm-examples/quickstart.md
  - Docker Compose: $SCRIPT_DIR/docker-compose.yml

EOF
