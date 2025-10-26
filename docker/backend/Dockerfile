# Multi-stage Docker build for KalamDB backend + CLI
# Feature: 006-docker-wasm-examples
# This Dockerfile builds both kalamdb-server and kalam-cli binaries

# ============================================================================
# Stage 1: Builder
# ============================================================================
FROM rust:1.90-bookworm AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y \
        build-essential \
        pkg-config \
        libssl-dev \
        libclang-dev \
        clang \
        cmake \
    && rm -rf /var/lib/apt/lists/*

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

# Copy link (dependency for CLI)
COPY link/Cargo.toml ./link/
COPY link/src ./link/src

# Build release binaries
RUN cargo build --release --bin kalamdb-server --bin kalam

# ============================================================================
# Stage 2: Runtime
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user (non-root)
RUN useradd -m -u 1000 kalamdb && \
    mkdir -p /data && \
    chown -R kalamdb:kalamdb /data

# Copy binaries from builder
COPY --from=builder /build/target/release/kalamdb-server /usr/local/bin/kalamdb-server
COPY --from=builder /build/target/release/kalam /usr/local/bin/kalam-cli

# Make binaries executable
RUN chmod +x /usr/local/bin/kalamdb-server /usr/local/bin/kalam-cli

# Switch to non-root user
USER kalamdb

# Set working directory
WORKDIR /data

# Expose default port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/kalam-cli", "exec", "SELECT 1"]

# Default command: run server
CMD ["/usr/local/bin/kalamdb-server"]
