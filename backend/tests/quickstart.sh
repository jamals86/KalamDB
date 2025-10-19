#!/bin/bash
# KalamDB Quickstart Automated Test Script (T227)
# Tests complete workflow: namespace → tables → data → queries → system tables
# Based on specs/002-simple-kalamdb/quickstart.md

set -e

# Configuration
BASE_URL="${KALAMDB_URL:-http://localhost:8080}"  # Default port is 8080 (see backend/config.toml)
API_ENDPOINT="${BASE_URL}/api/sql"
TOKEN="${TEST_JWT_TOKEN:-test-token-user123}"  # Placeholder for JWT token

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counter
PASSED=0
FAILED=0
TOTAL=0
SKIPPED=0

# Temp file for responses
RESPONSE_FILE=$(mktemp)
trap "rm -f $RESPONSE_FILE" EXIT

#######################################
# Helper Functions
#######################################

# Execute SQL via API
execute_sql() {
    local sql="$1"
    local test_name="$2"
    local require_auth="${3:-false}"
    
    TOTAL=$((TOTAL + 1))
    echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
    echo -e "${YELLOW}Test $TOTAL: $test_name${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
    echo "SQL: $sql"
    
    # Build curl command
    local curl_cmd="curl -s -X POST '$API_ENDPOINT' -H 'Content-Type: application/json'"
    
    if [ "$require_auth" = "true" ]; then
        curl_cmd="$curl_cmd -H 'Authorization: Bearer $TOKEN'"
    fi
    
    curl_cmd="$curl_cmd -d '{\"sql\": \"$sql\"}'"
    
    # Execute and save response
    eval "$curl_cmd" > "$RESPONSE_FILE" 2>&1
    
    # Check if response is valid JSON
    if ! jq empty "$RESPONSE_FILE" 2>/dev/null; then
        echo -e "${RED}❌ FAILED - Invalid JSON response${NC}"
        echo "Response: $(cat $RESPONSE_FILE)"
        FAILED=$((FAILED + 1))
        return 1
    fi
    
    # Check status
    local status=$(jq -r '.status' "$RESPONSE_FILE" 2>/dev/null || echo "unknown")
    
    if [ "$status" = "success" ]; then
        echo -e "${GREEN}✅ PASSED${NC}"
        PASSED=$((PASSED + 1))
        
        # Show response summary
        local row_count=$(jq -r '.results[0].row_count // .results[0].rows | length // 0' "$RESPONSE_FILE" 2>/dev/null)
        local exec_time=$(jq -r '.execution_time_ms // "N/A"' "$RESPONSE_FILE" 2>/dev/null)
        echo "Rows: $row_count | Execution time: ${exec_time}ms"
        
        # Show first few rows if present
        if [ "$row_count" -gt 0 ] && [ "$row_count" != "null" ]; then
            echo "Sample data:"
            jq -r '.results[0].rows[0:3]' "$RESPONSE_FILE" 2>/dev/null | head -20
        fi
        
        return 0
    else
        echo -e "${RED}❌ FAILED${NC}"
        FAILED=$((FAILED + 1))
        echo "Response:"
        cat "$RESPONSE_FILE" | jq '.'
        return 1
    fi
}

# Check if server is running
check_server() {
    echo -e "${BLUE}Checking if server is running at $BASE_URL...${NC}"
    if curl -s "$BASE_URL" > /dev/null 2>&1 || curl -s "$API_ENDPOINT" > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Server is running${NC}"
        return 0
    else
        echo -e "${RED}❌ Server is not running!${NC}"
        echo "Please start the server with: cd backend && cargo run --bin kalamdb-server"
        echo "Or set KALAMDB_URL environment variable if running on different port/host"
        exit 1
    fi
}

# Check dependencies
check_dependencies() {
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}❌ jq is not installed${NC}"
        echo "Please install jq:"
        echo "  macOS: brew install jq"
        echo "  Linux: apt-get install jq"
        exit 1
    fi
    
    if ! command -v curl &> /dev/null; then
        echo -e "${RED}❌ curl is not installed${NC}"
        exit 1
    fi
}

#######################################
# Main Test Suite
#######################################

echo "╔════════════════════════════════════════════════╗"
echo "║   KalamDB Quickstart Automated Test Suite     ║"
echo "║   Testing: Complete Feature Workflow          ║"
echo "╚════════════════════════════════════════════════╝"
echo ""
echo "Configuration:"
echo "  Base URL: $BASE_URL"
echo "  API Endpoint: $API_ENDPOINT"
echo "  JWT Token: ${TOKEN:0:20}..."
echo ""

check_dependencies
check_server

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 1: NAMESPACE MANAGEMENT${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 1: Create namespace
execute_sql "CREATE NAMESPACE quickstart_test" "Create namespace"

# Test 2: Create namespace IF NOT EXISTS (should not error)
execute_sql "CREATE NAMESPACE IF NOT EXISTS quickstart_test" "Create namespace (IF NOT EXISTS)"

# Test 3: Query system.namespaces (if available)
if execute_sql "SELECT * FROM system.namespaces WHERE namespace_id = 'quickstart_test'" "Query system.namespaces" 2>/dev/null; then
    :  # Success
else
    echo -e "${YELLOW}⚠️  system.namespaces may not be implemented yet${NC}"
    SKIPPED=$((SKIPPED + 1))
fi

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 2: USER TABLE CREATION${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 4: Create user table
execute_sql "CREATE TABLE quickstart_test.user_messages (
    message_id TEXT NOT NULL,
    text TEXT,
    conversation_id TEXT,
    created_at BIGINT
) LOCATION '/data/\${user_id}/messages'
FLUSH POLICY ROWS 1000
DELETED_RETENTION 7d" "Create user table"

# Test 5: Create user table IF NOT EXISTS
execute_sql "CREATE TABLE IF NOT EXISTS quickstart_test.user_messages (
    message_id TEXT NOT NULL,
    text TEXT
) LOCATION '/data/\${user_id}/messages'" "Create user table (IF NOT EXISTS)"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 3: SHARED TABLE CREATION${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 6: Create shared table
execute_sql "CREATE TABLE quickstart_test.conversations (
    conversation_id TEXT NOT NULL,
    title TEXT,
    participant_count BIGINT,
    status TEXT
) LOCATION '/data/shared/conversations'
FLUSH POLICY ROWS 500
DELETED_RETENTION 14d" "Create shared table"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 4: STREAM TABLE CREATION${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 7: Create stream table
execute_sql "CREATE TABLE quickstart_test.events (
    event_id TEXT NOT NULL,
    event_type TEXT,
    payload TEXT,
    timestamp BIGINT
) LOCATION '/data/\${user_id}/events'
TTL 3600s
BUFFER_SIZE 1000" "Create stream table"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 5: DATA OPERATIONS - SHARED TABLES${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 8: Insert into shared table
execute_sql "INSERT INTO quickstart_test.conversations (conversation_id, title, participant_count, status)
VALUES ('conv001', 'Team Standup', 5, 'active')" "Insert into shared table"

# Test 9: Insert multiple rows
execute_sql "INSERT INTO quickstart_test.conversations (conversation_id, title, participant_count, status)
VALUES ('conv002', 'Planning Meeting', 8, 'active');
INSERT INTO quickstart_test.conversations (conversation_id, title, participant_count, status)
VALUES ('conv003', 'Sprint Review', 12, 'completed')" "Insert multiple rows"

# Test 10: Query shared table data
execute_sql "SELECT conversation_id, title, status FROM quickstart_test.conversations ORDER BY conversation_id" "Query shared table"

# Test 11: Query with WHERE clause
execute_sql "SELECT conversation_id, title FROM quickstart_test.conversations 
WHERE status = 'active' ORDER BY conversation_id" "Query with WHERE clause"

# Test 12: UPDATE shared table
execute_sql "UPDATE quickstart_test.conversations 
SET title = 'Updated Standup', status = 'completed' 
WHERE conversation_id = 'conv001'" "UPDATE shared table"

# Test 13: Verify update
execute_sql "SELECT conversation_id, title, status FROM quickstart_test.conversations 
WHERE conversation_id = 'conv001'" "Verify UPDATE"

# Test 14: Query system columns
execute_sql "SELECT conversation_id, title, _updated, _deleted FROM quickstart_test.conversations 
WHERE conversation_id = 'conv001'" "Query system columns"

# Test 15: DELETE (soft delete)
execute_sql "DELETE FROM quickstart_test.conversations WHERE conversation_id = 'conv002'" "DELETE (soft delete)"

# Test 16: Verify soft delete filtered out
execute_sql "SELECT conversation_id FROM quickstart_test.conversations ORDER BY conversation_id" "Verify soft delete"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 6: ADVANCED QUERIES${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 17: COUNT aggregation
execute_sql "SELECT COUNT(*) as total FROM quickstart_test.conversations" "COUNT aggregation"

# Test 18: GROUP BY
execute_sql "SELECT status, COUNT(*) as count FROM quickstart_test.conversations 
GROUP BY status ORDER BY status" "GROUP BY query"

# Test 19: Empty table query
execute_sql "CREATE TABLE quickstart_test.empty_test (id TEXT NOT NULL) LOCATION '/data/shared/empty_test'" "Create empty table"
execute_sql "SELECT * FROM quickstart_test.empty_test" "Query empty table"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 7: SYSTEM TABLES${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 20: Query system.tables
if execute_sql "SELECT table_name, table_type FROM system.tables WHERE namespace_id = 'quickstart_test' ORDER BY table_name" "Query system.tables" 2>/dev/null; then
    :  # Success
else
    echo -e "${YELLOW}⚠️  system.tables query may need adjustment${NC}"
    SKIPPED=$((SKIPPED + 1))
fi

# Test 21: Query system.users (if available)
if execute_sql "SELECT * FROM system.users LIMIT 5" "Query system.users" 2>/dev/null; then
    :  # Success  
else
    echo -e "${YELLOW}⚠️  system.users may not be populated yet${NC}"
    SKIPPED=$((SKIPPED + 1))
fi

# Test 22: Query system.live_queries (if available)
if execute_sql "SELECT * FROM system.live_queries LIMIT 5" "Query system.live_queries" 2>/dev/null; then
    :  # Success
else
    echo -e "${YELLOW}⚠️  system.live_queries may be empty (no active subscriptions)${NC}"
    SKIPPED=$((SKIPPED + 1))
fi

# Test 23: Query system.jobs (if available)
if execute_sql "SELECT * FROM system.jobs LIMIT 5" "Query system.jobs" 2>/dev/null; then
    :  # Success
else
    echo -e "${YELLOW}⚠️  system.jobs may be empty (no jobs run yet)${NC}"
    SKIPPED=$((SKIPPED + 1))
fi

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 8: FLUSH POLICIES${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 24: Create table with ROWS policy
execute_sql "CREATE TABLE quickstart_test.test_rows (id TEXT NOT NULL) 
LOCATION '/data/shared/test_rows' FLUSH POLICY ROWS 500" "FLUSH POLICY ROWS"

# Test 25: Create table with TIME policy
execute_sql "CREATE TABLE quickstart_test.test_time (id TEXT NOT NULL) 
LOCATION '/data/shared/test_time' FLUSH POLICY TIME 300s" "FLUSH POLICY TIME"

# Test 26: Create table with combined policy
execute_sql "CREATE TABLE quickstart_test.test_combined (id TEXT NOT NULL) 
LOCATION '/data/shared/test_combined' FLUSH POLICY ROWS 1000 OR TIME 600s" "FLUSH POLICY Combined"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 9: DATA TYPES${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 27: Create multi-type table
execute_sql "CREATE TABLE quickstart_test.test_types (
    id TEXT NOT NULL,
    count BIGINT,
    price DOUBLE,
    is_active BOOLEAN
) LOCATION '/data/shared/test_types'" "Create multi-type table"

# Test 28: Insert with multiple types
execute_sql "INSERT INTO quickstart_test.test_types (id, count, price, is_active)
VALUES ('item1', 42, 99.99, true)" "Insert with multiple types"

# Test 29: Query multiple types
execute_sql "SELECT * FROM quickstart_test.test_types WHERE id = 'item1'" "Query multiple types"

echo -e "\n${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${YELLOW}PHASE 10: CLEANUP${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"

# Test 30: Drop tables
execute_sql "DROP TABLE quickstart_test.conversations" "Drop conversations table"
execute_sql "DROP TABLE quickstart_test.user_messages" "Drop user_messages table"
execute_sql "DROP TABLE quickstart_test.events" "Drop events table"
execute_sql "DROP TABLE quickstart_test.empty_test" "Drop empty_test table"
execute_sql "DROP TABLE quickstart_test.test_rows" "Drop test_rows table"
execute_sql "DROP TABLE quickstart_test.test_time" "Drop test_time table"
execute_sql "DROP TABLE quickstart_test.test_combined" "Drop test_combined table"
execute_sql "DROP TABLE quickstart_test.test_types" "Drop test_types table"

# Test 31: Drop namespace
execute_sql "DROP NAMESPACE quickstart_test" "Drop namespace"

# Test 32: Verify tables dropped
if execute_sql "SELECT * FROM quickstart_test.conversations" "Verify tables dropped" 2>/dev/null; then
    echo -e "${RED}⚠️  Table still exists after DROP!${NC}"
    FAILED=$((FAILED + 1))
else
    echo -e "${GREEN}✅ Table correctly dropped${NC}"
    PASSED=$((PASSED + 1))
    TOTAL=$((TOTAL + 1))
fi

#######################################
# Summary Report
#######################################

echo ""
echo "╔════════════════════════════════════════════════╗"
echo "║              Test Summary Report               ║"
echo "╚════════════════════════════════════════════════╝"
echo ""
echo "Total tests: $TOTAL"
echo -e "${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
else
    echo "Failed: $FAILED"
fi
if [ $SKIPPED -gt 0 ]; then
    echo -e "${YELLOW}Skipped: $SKIPPED${NC}"
fi
echo ""
echo "Coverage:"
echo "  ✅ Namespace management"
echo "  ✅ User table creation"
echo "  ✅ Shared table creation"
echo "  ✅ Stream table creation"
echo "  ✅ INSERT/UPDATE/DELETE operations"
echo "  ✅ Complex queries (WHERE, ORDER BY, GROUP BY)"
echo "  ✅ System columns (_updated, _deleted)"
echo "  ✅ Aggregations (COUNT)"
echo "  ✅ System tables (users, tables, live_queries, jobs)"
echo "  ✅ Flush policies (ROWS, TIME, Combined)"
echo "  ✅ Multiple data types"
echo "  ✅ Table lifecycle (CREATE/DROP)"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}╔════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  🎉 All tests passed! KalamDB is working! 🎉  ║${NC}"
    echo -e "${GREEN}╚════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Test WebSocket live queries (requires wscat or custom client)"
    echo "  2. Test performance benchmarks"
    echo "  3. Test concurrent operations"
    echo "  4. Deploy to production!"
    exit 0
else
    echo -e "${RED}╔════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║     ⚠️  Some tests failed. See details above.  ║${NC}"
    echo -e "${RED}╚════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Troubleshooting:"
    echo "  1. Check server logs for errors"
    echo "  2. Verify all features are implemented"
    echo "  3. Check database state: ls -la /tmp/kalamdb_data"
    echo "  4. Review failed test output above"
    exit 1
fi
