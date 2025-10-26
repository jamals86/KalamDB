#!/usr/bin/env bash
# Setup script for KalamDB TODO example application
# Feature: 006-docker-wasm-examples
#
# This script:
# 1. Validates KalamDB server is accessible
# 2. Creates the todos table from todo-app.sql
# 3. Verifies setup was successful
#
# Usage:
#   ./setup.sh [options]
#
# Options:
#   --server URL    KalamDB server URL (default: http://localhost:8080)
#   --help          Show this help message

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KALAMDB_URL="${KALAMDB_URL:-http://localhost:8080}"
SQL_FILE="$SCRIPT_DIR/todo-app.sql"

# ============================================================================
# Functions
# ============================================================================

show_help() {
    cat << EOF
Setup KalamDB TODO Example Application

Usage: $0 [options]

Options:
    --server URL    KalamDB server URL (default: http://localhost:8080)
    --help          Show this help message

Environment Variables:
    KALAMDB_URL     Override default server URL

Examples:
    # Setup with default localhost server
    ./setup.sh

    # Setup with custom server
    ./setup.sh --server http://192.168.1.100:8080

    # Or use environment variable
    KALAMDB_URL=http://192.168.1.100:8080 ./setup.sh

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
        --server)
            KALAMDB_URL="$2"
            shift 2
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

# Check kalam-cli is available
if ! command -v kalam &> /dev/null && ! command -v kalam-cli &> /dev/null; then
    log_error "kalam-cli not found in PATH"
    log_info "Please install kalam-cli or add it to your PATH"
    log_info "From repository root: cargo build --release --bin kalam"
    exit 1
fi

# Determine which command to use
KALAM_CMD="kalam"
if ! command -v kalam &> /dev/null; then
    KALAM_CMD="kalam-cli"
fi

# Check SQL file exists
if [[ ! -f "$SQL_FILE" ]]; then
    log_error "SQL file not found: $SQL_FILE"
    exit 1
fi

log_success "Pre-flight checks passed"

# ============================================================================
# Test Connection
# ============================================================================

log_info "Testing connection to KalamDB..."
log_info "  Server: $KALAMDB_URL"

# Try to execute a simple query
if ! $KALAM_CMD exec "SELECT 1" --url "$KALAMDB_URL" &> /dev/null; then
    log_error "Cannot connect to KalamDB server at $KALAMDB_URL"
    log_info "Please ensure:"
    log_info "  1. KalamDB server is running"
    log_info "  2. Server URL is correct: $KALAMDB_URL"
    log_info "  3. Network connectivity is working"
    exit 1
fi

log_success "Connection successful"

# ============================================================================
# Load Schema
# ============================================================================

log_info "Creating todos table..."

# Load SQL file (idempotent with CREATE TABLE IF NOT EXISTS)
if $KALAM_CMD load "$SQL_FILE" --url "$KALAMDB_URL"; then
    log_success "Table created successfully"
else
    log_error "Failed to create table"
    exit 1
fi

# ============================================================================
# Verify Setup
# ============================================================================

log_info "Verifying setup..."

# Check table exists by querying it
if $KALAM_CMD exec "SELECT COUNT(*) FROM todos" --url "$KALAMDB_URL" &> /dev/null; then
    log_success "Table verified"
else
    log_error "Table verification failed"
    exit 1
fi

# ============================================================================
# Summary
# ============================================================================

cat << EOF

🎉 Setup complete!

Database: $KALAMDB_URL
Table:    todos

Next steps:
  1. Install dependencies:
     npm install

  2. Create .env file:
     cp .env.example .env
     # Edit .env to add your API key

  3. Start development server:
     npm run dev

  4. Open browser:
     http://localhost:5173

Documentation:
  - Quick Start: $SCRIPT_DIR/../../specs/006-docker-wasm-examples/quickstart.md
  - README: $SCRIPT_DIR/README.md

EOF
