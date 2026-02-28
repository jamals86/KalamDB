#!/usr/bin/env bash
# Helper script to run CLI tests with custom server URL and authentication
#
# Usage:
#   ./run-tests.sh                                    # Run all workspace tests + CLI e2e (default)
#   ./run-tests.sh --url http://localhost:3000        # Custom URL
#   ./run-tests.sh --password mypass                  # Custom password
#   ./run-tests.sh --url http://localhost:3000 --password mypass --test smoke
#
# Examples:
#   ./run-tests.sh --test smoke                       # Run smoke tests only
#   ./run-tests.sh --url http://localhost:3000        # Test on port 3000
#   ./run-tests.sh --test "smoke_test_core" --nocapture # Run specific test with output

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default values
SERVER_URL="${KALAMDB_SERVER_URL:-http://127.0.0.1:8080}"
ROOT_PASSWORD="${KALAMDB_ROOT_PASSWORD:-}"
TEST_FILTER=""
NOCAPTURE=""
SHOW_HELP=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -u|--url)
            SERVER_URL="$2"
            shift 2
            ;;
        -p|--password)
            ROOT_PASSWORD="$2"
            shift 2
            ;;
        -t|--test)
            TEST_FILTER="$2"
            shift 2
            ;;
        --nocapture)
            NOCAPTURE="--nocapture"
            shift
            ;;
        -h|--help)
            SHOW_HELP=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            SHOW_HELP=true
            shift
            ;;
    esac
done

if [ "$SHOW_HELP" = true ]; then
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Default: runs all workspace tests via cargo nextest, with CLI e2e tests enabled"
    echo "         using feature: kalam-cli/e2e-tests"
    echo ""
    echo "Options:"
    echo "  -u, --url <URL>          Server URL (default: http://127.0.0.1:8080)"
    echo "  -p, --password <PASS>    Root password (default: empty)"
    echo "  -t, --test <FILTER>      Test filter (e.g., 'smoke', 'smoke_test_core')"
    echo "  --nocapture              Pass through test stdout/stderr (--no-capture)"
    echo "  -h, --help               Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --test smoke --nocapture"
    echo "  $0 --url http://localhost:3000 --password mypass"
    echo "  $0 --url http://localhost:3000 --test smoke"
    exit 0
fi

# Display configuration
echo "================================================"
echo "Running KalamDB Tests (cargo nextest)"
echo "================================================"
echo "Server URL:      $SERVER_URL"
echo "Root Password:   $([ -z "$ROOT_PASSWORD" ] && echo '(empty)' || echo '***')"
echo "Test Filter:     $([ -z "$TEST_FILTER" ] && echo '(all tests)' || echo "$TEST_FILTER")"
echo "Mode:            workspace + CLI e2e feature"
echo "================================================"
echo ""

# Export environment variables
export KALAMDB_SERVER_URL="$SERVER_URL"
export KALAMDB_ROOT_PASSWORD="$ROOT_PASSWORD"

# Ensure nextest is available
if ! cargo nextest --version >/dev/null 2>&1; then
    echo "Error: cargo-nextest is not installed."
    echo "Install it with: cargo install cargo-nextest"
    exit 1
fi

# Build nextest command
# Default behavior: run all workspace tests and include CLI e2e tests.
TEST_CMD=(
    cargo nextest run
    --workspace
    --all-targets
    --features "kalam-cli/e2e-tests"
)

if [ -n "$TEST_FILTER" ]; then
    if [[ "$TEST_FILTER" == smoke* ]]; then
        TEST_CMD+=(--test smoke)
        if [[ "$TEST_FILTER" != "smoke" ]]; then
            TEST_CMD+=("$TEST_FILTER")
        fi
    else
        TEST_CMD+=("$TEST_FILTER")
    fi
fi

if [ -n "$NOCAPTURE" ]; then
    TEST_CMD+=(--no-capture)
fi

# Run tests from workspace root
cd "$REPO_ROOT"

echo "Executing: ${TEST_CMD[*]}"
echo ""
"${TEST_CMD[@]}"
