#!/usr/bin/env bash
# Generate packaged Linux PostgreSQL extension artifacts for the Docker Compose
# test setup.
#
# Important:
# - On macOS, local `cargo build` / `cargo pgrx install` produces Mach-O `.dylib`
#   files for Darwin. The official PostgreSQL Docker image needs Linux ELF `.so`
#   files, so host-built macOS artifacts cannot be copied into the container.
# - The fast inner loop on macOS is still local `cargo pgrx install` against your
#   local pgrx PostgreSQL. Use this script only when you need Linux artifacts for
#   the real PostgreSQL Docker container.
#
# Usage:
#   cd <repo-root>
#   ./pg/docker/build.sh              # reuse builder image if present, else build + package + extract
#   ./pg/docker/build.sh --artifacts  # same as default
#   ./pg/docker/build.sh --extract    # extract from an already packaged container
#   ./pg/docker/build.sh --rebuild    # force rebuild builder image + package + extract
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ARTIFACTS_DIR="$REPO_ROOT/pg/docker/artifacts"
BUILDER_IMAGE="kalamdb-pg-builder:latest"
PACKAGE_CONTAINER="kalamdb-pg-package"

cleanup_package_container() {
    docker rm -f "$PACKAGE_CONTAINER" >/dev/null 2>&1 || true
}

build_builder() {
    echo "==> Building builder image (full Rust compilation)..."
    docker build -f "$REPO_ROOT/pg/docker/Dockerfile" \
        -t "$BUILDER_IMAGE" \
        --target builder \
        "$REPO_ROOT"
}

ensure_builder() {
    if docker image inspect "$BUILDER_IMAGE" >/dev/null 2>&1; then
        echo "==> Reusing existing builder image $BUILDER_IMAGE"
        return
    fi

    build_builder
}

package_extension() {
    echo "==> Packaging extension inside cached Linux builder image..."
    cleanup_package_container
    docker run --name "$PACKAGE_CONTAINER" "$BUILDER_IMAGE" sh -lc '
        export PATH=/usr/local/cargo/bin:/usr/local/rustup/toolchains/1.92.0-aarch64-unknown-linux-gnu/bin:$PATH
        export CARGO_PROFILE_RELEASE_LTO=thin
        export CARGO_PROFILE_RELEASE_STRIP=debuginfo
        export CARGO_PROFILE_RELEASE_OPT_LEVEL=z
        cd /build
        cargo pgrx install \
            -p kalam-pg-extension \
            -c /usr/bin/pg_config \
            --no-default-features \
            --release \
            -F "pg16"
        mkdir -p /tmp/pg-artifacts
        cp /usr/lib/postgresql/16/lib/pg_kalam.so /tmp/pg-artifacts/
        cp /usr/share/postgresql/16/extension/pg_kalam.control /tmp/pg-artifacts/
        cp /usr/share/postgresql/16/extension/pg_kalam--*.sql /tmp/pg-artifacts/
    '
}

extract_artifacts() {
    echo "==> Extracting extension artifacts..."
    mkdir -p "$ARTIFACTS_DIR"
    rm -f "$ARTIFACTS_DIR"/pg_kalam.so "$ARTIFACTS_DIR"/pg_kalam.control "$ARTIFACTS_DIR"/pg_kalam--*.sql
    docker cp "$PACKAGE_CONTAINER":/tmp/pg-artifacts/. "$ARTIFACTS_DIR/"

    echo "    Artifacts saved to $ARTIFACTS_DIR/"
    ls -lh "$ARTIFACTS_DIR"/
}

# --remote flag kept for backward compatibility (remote is the only mode)
if [ "${1:-}" = "--remote" ]; then
    shift
fi

case "${1:-}" in
    --artifacts)
        ensure_builder
        package_extension
        extract_artifacts
        cleanup_package_container
        ;;
    --rebuild)
        build_builder
        package_extension
        extract_artifacts
        cleanup_package_container
        ;;
    --extract)
        if ! docker ps -a --format '{{.Names}}' | grep -qx "$PACKAGE_CONTAINER"; then
            echo "Missing packaged container $PACKAGE_CONTAINER"
            echo "Run ./pg/docker/build.sh first."
            exit 1
        fi
        extract_artifacts
        ;;
    "")
        ensure_builder
        package_extension
        extract_artifacts
        cleanup_package_container
        echo ""
        echo "Done. Start PostgreSQL with: cd pg/docker && docker compose up -d"
        ;;
    *)
        echo "Unknown option: $1"
        echo "Usage: ./pg/docker/build.sh [--artifacts|--extract|--rebuild]"
        exit 1
        ;;
esac
