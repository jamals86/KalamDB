#!/bin/bash
# Test script to verify file descriptor leak fixes

echo "=== Testing File Descriptor Leak Fixes ==="
echo ""

# 1. Check current file descriptor limit
echo "1. Current file descriptor limit:"
ulimit -n
echo ""

# 2. Build the server
echo "2. Building server..."
cd /Users/jamal/git/KalamDB/backend
cargo build --bin kalamdb-server 2>&1 | tail -5
echo ""

# 3. Start server and check startup logs
echo "3. Starting server and checking startup logs..."
# Kill any existing server
pkill -f kalamdb-server 2>/dev/null
sleep 2

# Clear old logs
> logs/server.log

# Start server in background
cargo run --bin kalamdb-server > /tmp/server_startup.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
sleep 10

# Check for file descriptor warning and health metrics
echo ""
echo "=== File Descriptor Check (from startup) ==="
grep -A10 "File descriptor" logs/server.log || echo "No file descriptor check found"
echo ""

echo "=== Health Metrics (from running server) ==="
grep "Health metrics" logs/server.log | tail -3
echo ""

# Show process info
echo "=== Process File Descriptors ==="
lsof -p $SERVER_PID 2>/dev/null | wc -l
echo "Open files for PID $SERVER_PID"
echo ""

# Cleanup
echo "Stopping server (PID: $SERVER_PID)..."
kill $SERVER_PID 2>/dev/null
sleep 2

echo ""
echo "=== Test Complete ==="
