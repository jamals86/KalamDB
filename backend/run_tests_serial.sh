#!/bin/bash
# Run all tests serially to avoid state conflicts

set -e

echo "========================================="
echo "Running KalamDB Tests Serially"
echo "========================================="

# Clean up any existing test data
rm -rf data/rocksdb/* data/storage/* logs/*.jsonl 2>/dev/null || true

# Run tests one at a time
echo ""
echo "Running test_testserver..."
cargo test --test test_testserver -- --test-threads=1

echo ""
echo "Running test_scenarios..."
cargo test --test test_scenarios -- --test-threads=1

echo ""
echo "Running test_misc..."
cargo test --test test_misc -- --test-threads=1

echo ""
echo "========================================="
echo "All tests completed!"
echo "========================================="
