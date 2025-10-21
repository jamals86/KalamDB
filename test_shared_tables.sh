#!/bin/bash
# Phase 13 Integration Test Runner
# Quick-start script for testing shared tables via REST API

set -e

BASE_URL="http://localhost:3000"
API_ENDPOINT="${BASE_URL}/api/sql"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
PASSED=0
FAILED=0
TOTAL=0

# Function to execute SQL via API
execute_sql() {
    local sql="$1"
    local test_name="$2"
    
    TOTAL=$((TOTAL + 1))
    echo -e "\n${YELLOW}Test $TOTAL: $test_name${NC}"
    echo "SQL: $sql"
    
    response=$(curl -s -X POST "$API_ENDPOINT" \
        -H "Content-Type: application/json" \
        -d "{\"sql\": \"$sql\"}")
    
    status=$(echo "$response" | jq -r '.status')
    
    if [ "$status" = "success" ]; then
        echo -e "${GREEN}✅ PASSED${NC}"
        PASSED=$((PASSED + 1))
        echo "Response: $response" | jq '.'
        return 0
    else
        echo -e "${RED}❌ FAILED${NC}"
        FAILED=$((FAILED + 1))
        echo "Response: $response" | jq '.'
        return 1
    fi
}

# Function to check if server is running
check_server() {
    echo "Checking if server is running at $BASE_URL..."
    if curl -s "$BASE_URL" > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Server is running${NC}"
        return 0
    else
        echo -e "${RED}❌ Server is not running!${NC}"
        echo "Please start the server with: cargo run --bin kalamdb-server"
        exit 1
    fi
}

# Check dependencies
check_dependencies() {
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}❌ jq is not installed${NC}"
        echo "Please install jq: brew install jq (macOS) or apt-get install jq (Linux)"
        exit 1
    fi
    
    if ! command -v curl &> /dev/null; then
        echo -e "${RED}❌ curl is not installed${NC}"
        exit 1
    fi
}

echo "================================================"
echo "Phase 13: Shared Tables Integration Test Runner"
echo "================================================"

check_dependencies
check_server

echo -e "\n${YELLOW}Starting test suite...${NC}\n"

# Test 1: Create namespace
execute_sql "CREATE NAMESPACE test_shared" "Create namespace"

# Test 2: Create shared table
execute_sql "CREATE TABLE test_shared.conversations (conversation_id TEXT NOT NULL, title TEXT, participant_count BIGINT, status TEXT) LOCATION '/data/shared/conversations' FLUSH POLICY ROWS 1000 DELETED_RETENTION 7d" "Create shared table"

# Test 3: Insert single row
execute_sql "INSERT INTO test_shared.conversations (conversation_id, title, participant_count, status) VALUES ('conv001', 'Team Standup', 5, 'active')" "Insert single row"

# Test 4: Query inserted data
execute_sql "SELECT conversation_id, title, participant_count, status FROM test_shared.conversations WHERE conversation_id = 'conv001'" "Query inserted data"

# Test 5: Insert multiple rows (split into separate statements for simplicity)
execute_sql "INSERT INTO test_shared.conversations (conversation_id, title, participant_count, status) VALUES ('conv002', 'Planning Meeting', 8, 'active')" "Insert row 2"
execute_sql "INSERT INTO test_shared.conversations (conversation_id, title, participant_count, status) VALUES ('conv003', 'Sprint Review', 12, 'active')" "Insert row 3"
execute_sql "INSERT INTO test_shared.conversations (conversation_id, title, participant_count, status) VALUES ('conv004', 'Old Discussion', 3, 'archived')" "Insert row 4"

# Test 6: Query all rows
execute_sql "SELECT conversation_id, title, status FROM test_shared.conversations ORDER BY conversation_id" "Query all rows"

# Test 7: Query with WHERE clause
execute_sql "SELECT conversation_id, title FROM test_shared.conversations WHERE status = 'active' ORDER BY conversation_id" "Query with WHERE"

# Test 8: UPDATE row
execute_sql "UPDATE test_shared.conversations SET title = 'Updated Standup', status = 'archived' WHERE conversation_id = 'conv001'" "UPDATE row"

# Test 9: Verify update
execute_sql "SELECT conversation_id, title, status FROM test_shared.conversations WHERE conversation_id = 'conv001'" "Verify UPDATE"

# Test 10: Query system columns
execute_sql "SELECT conversation_id, title, _updated, _deleted FROM test_shared.conversations WHERE conversation_id = 'conv001'" "Query system columns"

# Test 11: DELETE row (soft delete)
execute_sql "DELETE FROM test_shared.conversations WHERE conversation_id = 'conv002'" "DELETE row (soft delete)"

# Test 12: Verify deletion
execute_sql "SELECT conversation_id FROM test_shared.conversations ORDER BY conversation_id" "Verify DELETE filtered out"

# Test 13: COUNT aggregation
execute_sql "SELECT COUNT(*) as total_conversations FROM test_shared.conversations" "COUNT aggregation"

# Test 14: GROUP BY
execute_sql "SELECT status, COUNT(*) as count FROM test_shared.conversations GROUP BY status ORDER BY status" "GROUP BY query"

# Test 15: IF NOT EXISTS
execute_sql "CREATE TABLE IF NOT EXISTS test_shared.conversations (conversation_id TEXT NOT NULL, title TEXT) LOCATION '/data/shared/conversations'" "IF NOT EXISTS (duplicate)"

# Test 16: Create tables with different flush policies
execute_sql "CREATE TABLE test_shared.test_rows (id TEXT NOT NULL, data TEXT) LOCATION '/data/shared/test_rows' FLUSH POLICY ROWS 500" "FLUSH POLICY ROWS"
execute_sql "CREATE TABLE test_shared.test_time (id TEXT NOT NULL, data TEXT) LOCATION '/data/shared/test_time' FLUSH POLICY TIME 300s" "FLUSH POLICY TIME"
execute_sql "CREATE TABLE test_shared.test_combined (id TEXT NOT NULL, data TEXT) LOCATION '/data/shared/test_combined' FLUSH POLICY ROWS 1000 OR TIME 600s" "FLUSH POLICY Combined"

# Test 17: Multi-type table
execute_sql "CREATE TABLE test_shared.test_types (id TEXT NOT NULL, count BIGINT, price DOUBLE, is_active BOOLEAN) LOCATION '/data/shared/test_types'" "Create multi-type table"
execute_sql "INSERT INTO test_shared.test_types (id, count, price, is_active) VALUES ('item1', 42, 99.99, true)" "Insert with multiple types"
execute_sql "SELECT id, count, price, is_active FROM test_shared.test_types WHERE id = 'item1'" "Query multiple types"

# Test 18: Query empty table
execute_sql "CREATE TABLE test_shared.empty_table (id TEXT NOT NULL, data TEXT) LOCATION '/data/shared/empty_table'" "Create empty table"
execute_sql "SELECT * FROM test_shared.empty_table" "Query empty table"

# Cleanup
echo -e "\n${YELLOW}Cleanup: Dropping test tables...${NC}"
execute_sql "DROP TABLE test_shared.conversations" "Drop conversations"
execute_sql "DROP TABLE test_shared.test_rows" "Drop test_rows"
execute_sql "DROP TABLE test_shared.test_time" "Drop test_time"
execute_sql "DROP TABLE test_shared.test_combined" "Drop test_combined"
execute_sql "DROP TABLE test_shared.test_types" "Drop test_types"
execute_sql "DROP TABLE test_shared.empty_table" "Drop empty_table"

# Drop namespace
execute_sql "DROP NAMESPACE test_shared" "Drop namespace"

# Summary
echo -e "\n================================================"
echo "Test Summary"
echo "================================================"
echo -e "Total tests: $TOTAL"
echo -e "${GREEN}Passed: $PASSED${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
else
    echo -e "Failed: $FAILED"
fi
echo "================================================"

if [ $FAILED -eq 0 ]; then
    echo -e "\n${GREEN}🎉 All tests passed! Phase 13 shared tables are working!${NC}\n"
    exit 0
else
    echo -e "\n${RED}⚠️  Some tests failed. Check the output above for details.${NC}\n"
    exit 1
fi
