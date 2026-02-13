#!/usr/bin/env bash
# Setup script for KalamDB Chat with AI example application
#
# This script:
# 1. Validates KalamDB server is accessible
# 2. Logs in to get JWT token
# 3. Creates users (demo-user + ai-service)
# 4. Creates tables and topics from chat-app.sql
# 5. Generates .env.local file
#
# Usage:
#   ./setup.sh [options]
#
# Options:
#   --server URL       KalamDB server URL (default: http://localhost:8080)
#   --password PASS    Root password (default: kalamdb123)
#   --help             Show this help message

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KALAMDB_URL="${KALAMDB_URL:-http://localhost:8080}"
ROOT_PASSWORD="${KALAMDB_ROOT_PASSWORD:-kalamdb123}"
SQL_FILE="$SCRIPT_DIR/chat-app.sql"
ENV_FILE="$SCRIPT_DIR/.env.local"
ACCESS_TOKEN=""

# ============================================================================
# Functions
# ============================================================================

show_help() {
    cat << EOF
Setup KalamDB Chat with AI Example Application

Usage: $0 [options]

Options:
    --server URL       KalamDB server URL (default: http://localhost:8080)
    --password PASS    Root password (default: kalamdb123)
    --help             Show this help message

Environment Variables:
    KALAMDB_URL            Override default server URL
    KALAMDB_ROOT_PASSWORD  Override default root password (default: kalamdb123)

Examples:
    # Setup with defaults
    ./setup.sh

    # Setup with custom server
    ./setup.sh --server http://192.168.1.100:8080 --password mysecret

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

log_warn() {
    echo "⚠️  $*"
}

# Login and get JWT access token
login() {
    log_info "Logging in as root..."
    
    local response
    response=$(curl -s -X POST "${KALAMDB_URL}/v1/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\": \"root\", \"password\": \"$ROOT_PASSWORD\"}")
    
    # Check for error
    if echo "$response" | jq -e '.error' >/dev/null 2>&1; then
        log_error "Login failed:"
        echo "$response" | jq -r '.error // .message'
        exit 1
    fi
    
    # Extract access token
    ACCESS_TOKEN=$(echo "$response" | jq -r '.access_token')
    
    if [[ -z "$ACCESS_TOKEN" || "$ACCESS_TOKEN" == "null" ]]; then
        log_error "Failed to extract access token"
        exit 1
    fi
    
    log_success "Logged in successfully"
}

execute_sql() {
    local sql="$1"
    local response
    
    response=$(curl -s -w "\n%{http_code}" \
        -X POST "${KALAMDB_URL}/v1/api/sql" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $ACCESS_TOKEN" \
        -d "{\"sql\": $(jq -Rs . <<< "$sql")}")
    
    local http_code
    http_code=$(echo "$response" | tail -1)
    local body
    body=$(echo "$response" | sed '$d')
    
    if [[ "$http_code" -ge 200 && "$http_code" -lt 300 ]]; then
        echo "$body"
        return 0
    else
        echo "$body" >&2
        return 1
    fi
}

execute_sql_file() {
    local file="$1"
    local sql_buffer=""
    
    log_info "Executing SQL from $file..."
    
    # Read file and send each statement
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip empty lines and comments
        [[ -z "$line" || "$line" =~ ^[[:space:]]*-- ]] && continue
        
        # Accumulate multi-line SQL
        sql_buffer="${sql_buffer}${line} "
        
        # Check if line ends with semicolon (end of statement)
        if [[ "$line" =~ \;[[:space:]]*$ ]]; then
            # Remove trailing semicolon and whitespace
            local stmt="${sql_buffer%;*}"
            stmt=$(echo "$stmt" | xargs)  # trim whitespace
            
            if [[ -n "$stmt" ]]; then
                log_info "Executing: ${stmt:0:80}..."
                if ! execute_sql "$stmt" >/dev/null 2>&1; then
                    log_warn "Statement may have failed (possibly already exists): ${stmt:0:60}..."
                fi
            fi
            sql_buffer=""
        fi
    done < "$file"
    
    log_success "SQL file executed"
}

create_user() {
    local username="$1"
    local role="$2"
    local password="$3"
    
    log_info "Creating user: $username (role: $role)..."
    
    local result
    if result=$(execute_sql "CREATE USER '$username' WITH PASSWORD '$password' ROLE $role" 2>&1); then
        log_success "User '$username' created"
        return 0
    else
        if echo "$result" | grep -qi "already exists"; then
            log_warn "User '$username' already exists (skipping)"
            return 0
        fi
        log_warn "User creation may have failed: $result"
        return 0
    fi
}

generate_env_file() {
    log_info "Generating .env.local file..."
    
    cat > "$ENV_FILE" << EOF
# KalamDB Connection (generated by setup.sh)
# Note: NEXT_PUBLIC_ prefix is required for browser-side access
NEXT_PUBLIC_KALAMDB_URL=$KALAMDB_URL
NEXT_PUBLIC_KALAMDB_USERNAME=admin
NEXT_PUBLIC_KALAMDB_PASSWORD=$ROOT_PASSWORD

# Service Credentials (for message-processor.ts backend service)
KALAMDB_URL=$KALAMDB_URL
KALAMDB_USERNAME=ai-service
KALAMDB_PASSWORD=service123

# Gemini API (required for AI replies)
# Set one key value below before running `npm run service`
GEMINI_API_KEY=
# GOOGLE_GENERATIVE_AI_API_KEY=
GEMINI_MODEL=gemini-2.5-flash
EOF
    
    log_success ".env.local file created"
}

check_server() {
    log_info "Checking KalamDB server at $KALAMDB_URL..."
    
    if ! curl -s -f "${KALAMDB_URL}/health" >/dev/null 2>&1; then
        log_error "Cannot connect to KalamDB server at $KALAMDB_URL"
        log_error "Please ensure:"
        log_error "  1. KalamDB server is running (cd backend && cargo run)"
        log_error "  2. Server URL is correct: $KALAMDB_URL"
        exit 1
    fi
    
    log_success "Server is healthy"
}

verify_setup() {
    log_info "Verifying setup..."
    
    # Check if tables exist
    local result
    result=$(execute_sql "SELECT * FROM chat.conversations LIMIT 1" 2>&1 || true)
    
    if echo "$result" | grep -qi "error"; then
        log_warn "Verification incomplete, but setup may have succeeded"
        return 0
    fi
    
    log_success "Setup verified - tables are accessible"
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
        --password)
            ROOT_PASSWORD="$2"
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
# Main
# ============================================================================

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         KalamDB Chat with AI - Setup Script                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

log_info "Configuration:"
log_info "  Server URL: $KALAMDB_URL"
log_info "  SQL File: $SQL_FILE"
log_info "  Env File: $ENV_FILE"
echo ""

# Step 1: Check server
check_server

# Step 2: Login to get JWT token
login

# Step 3: Create users
echo ""
log_info "Creating users..."
create_user "demo-user" "user" "demo123"
create_user "ai-service" "service" "service123"

# Step 4: Execute SQL schema
echo ""
if [[ -f "$SQL_FILE" ]]; then
    execute_sql_file "$SQL_FILE"
else
    log_error "SQL file not found: $SQL_FILE"
    exit 1
fi

# Step 5: Generate environment file
echo ""
generate_env_file

# Step 6: Verify setup
echo ""
verify_setup

# Done!
echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Setup Complete! 🎉                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
log_success "Chat application schema created"
log_success "Users created: demo-user, ai-service"
log_success "Environment file: .env.local"
echo ""
log_info "Next steps:"
log_info "  1. Install dependencies: npm install"
log_info "  2. Start message processor: npm run service"
log_info "  3. Start Next.js dev server: npm run dev"
log_info "  4. Open http://localhost:3000/design1 (or design2, design3)"
echo ""
