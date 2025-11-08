#!/bin/bash
# Quick test to verify information_schema.columns is registered

cd /Users/jamal/git/KalamDB/backend

# Start server in background
cargo run --bin kalamdb-server > /tmp/kalamdb.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
echo "Waiting for server to start..."
sleep 10

# Test information_schema.columns query
echo "Testing information_schema.columns query..."
curl -s -X POST http://localhost:8080/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Basic $(echo -n 'sys_root:changeme' | base64)" \
  -d '{"sql": "SELECT * FROM information_schema.columns WHERE table_name = '\''jobs'\'' ORDER BY ordinal_position LIMIT 5"}' \
  | python3 -m json.tool

# Kill server
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo "Test complete!"
