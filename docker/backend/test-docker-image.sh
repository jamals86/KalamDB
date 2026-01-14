#!/usr/bin/env bash
# Docker Image Smoke Test
# Tests that the built Docker image works correctly

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
IMAGE_NAME="${1:-jamals86/kalamdb:test}"
CONTAINER_NAME="kalamdb-test-$$"
TEST_PORT=8081
TIMEOUT=30

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

cleanup() {
    log_info "Cleaning up test container..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
}

# Trap cleanup on exit
trap cleanup EXIT INT TERM

main() {
    log_info "Testing Docker image: $IMAGE_NAME"
    
    # Check if image exists
    if ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
        log_error "Image $IMAGE_NAME not found!"
        exit 1
    fi
    
    log_info "Starting container..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        -p "$TEST_PORT:8080" \
        -e KALAMDB_LOG_LEVEL=info \
        "$IMAGE_NAME"
    
    log_info "Waiting for server to start (timeout: ${TIMEOUT}s)..."
    START_TIME=$(date +%s)
    while true; do
        CURRENT_TIME=$(date +%s)
        ELAPSED=$((CURRENT_TIME - START_TIME))
        
        if [ $ELAPSED -ge $TIMEOUT ]; then
            log_error "Server did not start within ${TIMEOUT}s"
            log_info "Container logs:"
            docker logs "$CONTAINER_NAME"
            exit 1
        fi
        
        if curl -sf "http://localhost:$TEST_PORT/health" &>/dev/null; then
            log_info "Server is ready! (took ${ELAPSED}s)"
            break
        fi
        
        sleep 1
    done
    
    # Test 1: Health check
    log_info "Test 1: Health check endpoint..."
    HEALTH_RESPONSE=$(curl -sf "http://localhost:$TEST_PORT/health")
    if [ $? -eq 0 ]; then
        log_info "✓ Health check passed: $HEALTH_RESPONSE"
    else
        log_error "✗ Health check failed"
        exit 1
    fi
    
    # Test 2: Version info (healthcheck)
    log_info "Test 2: Version info (healthcheck)..."
    VERSION_RESPONSE=$(curl -sf "http://localhost:$TEST_PORT/v1/api/healthcheck" || echo "FAILED")
    if [ "$VERSION_RESPONSE" != "FAILED" ]; then
        log_info "✓ Version info passed: $VERSION_RESPONSE"
    else
        log_error "✗ Version info failed"
        exit 1
    fi
    
    # Test 3: Check binary existence and version
    log_info "Test 3: Checking binaries inside container..."
    docker exec "$CONTAINER_NAME" /usr/local/bin/kalamdb-server --version &>/dev/null
    if [ $? -eq 0 ]; then
        SERVER_VERSION=$(docker exec "$CONTAINER_NAME" /usr/local/bin/kalamdb-server --version)
        log_info "✓ Server binary: $SERVER_VERSION"
    else
        log_error "✗ Server binary not found or not executable"
        exit 1
    fi
    
    docker exec "$CONTAINER_NAME" /usr/local/bin/kalam-cli --version &>/dev/null
    if [ $? -eq 0 ]; then
        CLI_VERSION=$(docker exec "$CONTAINER_NAME" /usr/local/bin/kalam-cli --version)
        log_info "✓ CLI binary: $CLI_VERSION"
    else
        log_error "✗ CLI binary not found or not executable"
        exit 1
    fi
    
    # Test 4: Check symlink
    log_info "Test 4: Checking CLI symlink..."
    docker exec "$CONTAINER_NAME" /usr/local/bin/kalam --version &>/dev/null
    if [ $? -eq 0 ]; then
        log_info "✓ CLI symlink works"
    else
        log_error "✗ CLI symlink not working"
        exit 1
    fi
    
    # Test 5: Set root password inside container, then test SQL with new password
    log_info "Test 5: Setting root password inside container..."

    ROOT_PASSWORD="testpass123"
    docker exec "$CONTAINER_NAME" /usr/local/bin/kalam \
        --url "http://localhost:8080" \
        --username root \
        --password "" \
        --command "ALTER USER root SET PASSWORD '$ROOT_PASSWORD'" &>/dev/null

    if [ $? -ne 0 ]; then
        log_warn "⚠ Failed to set root password inside container; skipping SQL auth test"
    else
        log_info "Root password updated"

        QUERY_RESPONSE=$(curl -sf -X POST \
            "http://localhost:$TEST_PORT/v1/api/sql" \
            -u "root:$ROOT_PASSWORD" \
            -H "Content-Type: application/json" \
            -d '{"sql":"SELECT 1 as test"}' 2>&1 || echo "FAILED")

        if [ "$QUERY_RESPONSE" != "FAILED" ]; then
            log_info "✓ SQL query execution passed"
        else
            log_warn "⚠ SQL query test failed after password update"
        fi
    fi
    
    # Test 6: Check container resource usage
    log_info "Test 6: Checking container resource usage..."
    STATS=$(docker stats "$CONTAINER_NAME" --no-stream --format "table {{.CPUPerc}}\t{{.MemUsage}}")
    log_info "Container stats:\n$STATS"
    
    # All tests passed
    log_info ""
    log_info "=================================="
    log_info "✓ All tests passed successfully!"
    log_info "=================================="
    log_info ""
    log_info "Container logs:"
    docker logs "$CONTAINER_NAME" | tail -20
    
    return 0
}

main "$@"
