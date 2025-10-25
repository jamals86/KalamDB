#!/bin/bash
# run_integration_tests.sh
# Helper script to run CLI integration tests
#
# Usage:
#   ./run_integration_tests.sh          # Run all tests
#   ./run_integration_tests.sh cli      # Run only CLI tests
#   ./run_integration_tests.sh ws       # Run only WebSocket tests
#   ./run_integration_tests.sh link     # Run only kalam-link tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if server is running
check_server() {
    if curl -s http://localhost:8080/v1/api/healthcheck > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Server is running${NC}"
        return 0
    else
        echo -e "${YELLOW}⚠️  Server is not running at http://localhost:8080${NC}"
        echo ""
        echo "To start the server:"
        echo "  cd backend && cargo run --bin kalamdb-server"
        echo ""
        echo "Tests will be skipped if server is not running."
        return 1
    fi
}

# Run all tests
run_all() {
    echo "Running all CLI integration tests..."
    cargo test --workspace -- --test-threads=1 --nocapture
}

# Run CLI tests only
run_cli() {
    echo "Running CLI integration tests..."
    cargo test --test test_cli_integration -- --test-threads=1 --nocapture
}

# Run WebSocket tests only
run_ws() {
    echo "Running WebSocket integration tests..."
    cd kalam-link
    cargo test --test test_websocket_integration -- --test-threads=1 --nocapture
    cd ..
}

# Run kalam-link tests only
run_link() {
    echo "Running kalam-link library tests..."
    cd kalam-link
    cargo test --test integration_tests -- --test-threads=1 --nocapture
    cd ..
}

# Main
cd "$(dirname "$0")"

echo ""
echo "=== KalamDB CLI Integration Tests ==="
echo ""

check_server
SERVER_RUNNING=$?

echo ""

case "${1:-all}" in
    all)
        run_all
        ;;
    cli)
        run_cli
        ;;
    ws|websocket)
        run_ws
        ;;
    link|kalam-link)
        run_link
        ;;
    *)
        echo "Usage: $0 [all|cli|ws|link]"
        exit 1
        ;;
esac

echo ""
if [ $SERVER_RUNNING -eq 0 ]; then
    echo -e "${GREEN}✅ Tests completed successfully${NC}"
else
    echo -e "${YELLOW}⚠️  Tests completed (some may have been skipped)${NC}"
    echo ""
    echo "Start the server to run all tests:"
    echo "  cd ../backend && cargo run --bin kalamdb-server"
fi
echo ""
