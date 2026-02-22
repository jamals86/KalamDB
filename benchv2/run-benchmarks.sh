#!/usr/bin/env bash
# KalamDB Benchmark Runner
# Usage: ./run-benchmarks.sh [--url URL] [--user USER] [--password PASS] [--iterations N]
set -euo pipefail

# Raise file-descriptor limit for the benchmark process (WebSocket connections, etc.)
ulimit -n "$(ulimit -Hn 2>/dev/null || echo 65536)" 2>/dev/null || true

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Default values (can be overridden via args or env vars)
URL="${KALAMDB_URL:-http://localhost:8080}"
USER="${KALAMDB_USER:-admin}"
PASSWORD="${KALAMDB_PASSWORD:-kalamdb123}"
EXTRA_ARGS=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --url) URL="$2"; shift 2;;
        --user) USER="$2"; shift 2;;
        --password) PASSWORD="$2"; shift 2;;
        *) EXTRA_ARGS="$EXTRA_ARGS $1"; shift;;
    esac
done

echo "🚀 KalamDB Benchmark Suite"
echo "   Server: $URL"
echo ""

# Build and run
cargo run --release -- \
    --url "$URL" \
    --user "$USER" \
    --password "$PASSWORD" \
    $EXTRA_ARGS

echo ""
echo "📊 Reports saved to results/"
ls -la results/*.html results/*.json 2>/dev/null || true