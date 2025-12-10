# Multi-stage Docker build for KalamDB backend + CLI
# Feature: 006-docker-wasm-examples
# This Dockerfile builds both kalamdb-server and kalam-cli binaries
# Supports multi-architecture: linux/amd64, linux/arm64

# ============================================================================
# Stage 1: Builder
# ============================================================================
FROM rust:1.90-bookworm AS builder

# Install build dependencies including Node.js for UI build
RUN apt-get update && \
    apt-get install -y \
        build-essential \
        pkg-config \
        libssl-dev \
        libclang-dev \
        clang \
        cmake \
        curl \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20 LTS for UI build
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g npm@latest && \
    rm -rf /var/lib/apt/lists/*

# Install wasm-pack for building WASM SDK
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Install wasm32 target for Rust
RUN rustup target add wasm32-unknown-unknown

# Set working directory
WORKDIR /build

# Copy workspace Cargo files (root level)
COPY Cargo.toml Cargo.lock ./

# Copy backend Cargo file and source
COPY backend/Cargo.toml ./backend/
COPY backend/src ./backend/src
COPY backend/crates ./backend/crates

# Copy CLI Cargo file and source
COPY cli/Cargo.toml ./cli/
COPY cli/src ./cli/src
COPY cli/build.rs ./cli/

# Copy link (dependency for CLI and UI SDK)
COPY link/Cargo.toml ./link/
COPY link/src ./link/src
COPY link/sdks ./link/sdks

# Copy UI source for embedded admin UI
COPY ui ./ui

# Copy benchmark (needed for workspace, but we won't build it)
COPY benchmark/Cargo.toml ./benchmark/
RUN mkdir -p benchmark/src && echo "fn main() {}" > benchmark/src/main.rs

# Copy build scripts
COPY backend/build.rs ./backend/

# Copy or generate version.toml for compile-time version info
# If version.toml exists, use it; otherwise create a default
COPY version.tom[l] ./
RUN if [ ! -f version.toml ]; then \
    echo '# Auto-generated for Docker build' > version.toml && \
    echo '[version]' >> version.toml && \
    echo 'git_commit_hash = "docker"' >> version.toml && \
    echo 'git_branch = "docker"' >> version.toml && \
    echo "build_date = \"$(date -u '+%Y-%m-%d %H:%M:%S UTC')\"" >> version.toml; \
    fi

# Build optimized release binaries using docker profile
# (opt-level=s, thin LTO, codegen-units=16 - less memory-intensive for cross-compilation)
RUN cargo build --profile docker --bin kalamdb-server --bin kalam

# ============================================================================
# Stage 2: Runtime (optimized for minimal size)
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies and create user in single layer
# Note: curl is needed for healthcheck
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/* && \
    useradd -m -u 1000 kalamdb && \
    mkdir -p /data/rocksdb /data/storage /data/logs /config && \
    chown -R kalamdb:kalamdb /data /config

# Copy binaries from builder with correct permissions in single layer
# Using --chmod to set executable bit without extra layer
COPY --from=builder --chmod=755 /build/target/docker/kalamdb-server /usr/local/bin/kalamdb-server
COPY --from=builder --chmod=755 /build/target/docker/kalam /usr/local/bin/kalam-cli

# Create symlink so 'kalam' command works (docs reference this name)
RUN ln -s /usr/local/bin/kalam-cli /usr/local/bin/kalam

# Copy default server configuration (owned by kalamdb user)
COPY --chown=kalamdb:kalamdb docker/backend/server.toml /config/server.toml
COPY --chown=kalamdb:kalamdb docker/backend/server.toml /data/server.toml

# Switch to non-root user
USER kalamdb

# Set working directory (server looks for server.toml here)
WORKDIR /data

# Expose default port
EXPOSE 8080

# Health check using curl (more reliable than CLI which needs auth)
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8080/health || exit 1

# Default command: run server (looks for server.toml in current directory)
CMD ["/usr/local/bin/kalamdb-server"]
