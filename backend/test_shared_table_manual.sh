#!/bin/bash
# Manual test script to diagnose shared table issue

set -e

echo "🧪 Testing shared table INSERT/SELECT manually"
echo ""

# Get CLI path (workspace target directory)
CLI="../target/debug/kalam"
if [ ! -f "$CLI" ]; then
    echo "❌ CLI not found at $CLI"
    echo "Building CLI..."
    (cd .. && cargo build -p kalam-cli)
fi

# Test namespace and table
NS="test_shared_$(date +%s)"
TABLE="test_table"
FULL_TABLE="$NS.$TABLE"

echo "📝 Using table: $FULL_TABLE"
echo ""

# Cleanup
echo "🧹 Cleaning up..."
$CLI -c "DROP NAMESPACE IF EXISTS $NS CASCADE" 2>/dev/null || true
sleep 0.2

# Create namespace
echo "📦 Creating namespace..."
$CLI -c "CREATE NAMESPACE $NS"
sleep 0.2

# Create shared table
echo "📋 Creating SHARED table..."
$CLI -c "CREATE SHARED TABLE $FULL_TABLE (id INT AUTO_INCREMENT, name VARCHAR NOT NULL, value INT NOT NULL)"
sleep 0.2

# Insert 5 rows
echo "➕ Inserting 5 rows..."
for i in {1..5}; do
    echo "  Inserting row $i..."
    $CLI -c "INSERT INTO $FULL_TABLE (name, value) VALUES ('Row $i', $i)"
done
sleep 0.5

# Query count
echo ""
echo "🔍 Querying row count..."
$CLI -c "SELECT COUNT(*) as total FROM $FULL_TABLE"

# Query all rows
echo ""
echo "🔍 Querying all rows..."
$CLI -c "SELECT * FROM $FULL_TABLE ORDER BY value"

# Cleanup
echo ""
echo "🧹 Cleaning up..."
$CLI -c "DROP NAMESPACE $NS CASCADE"

echo ""
echo "✅ Test completed"
