#!/bin/bash
set -e

# Build the server first
echo "Building server..."
cargo build -p kalamdb-server

# Start the server binary in the background
echo "Starting server..."
./target/debug/kalamdb-server > server_full.log 2>&1 &
SERVER_PID=$!

# Wait for the server to be ready
echo "Waiting for server to start..."
MAX_RETRIES=30
for i in $(seq 1 $MAX_RETRIES); do
    if curl -s http://localhost:8080/v1/api/healthcheck > /dev/null; then
        echo "Server is up!"
        break
    fi
    if [ $i -eq $MAX_RETRIES ]; then
        echo "Server failed to start"
        kill $SERVER_PID
        exit 1
    fi
    sleep 1
done

# Run all smoke tests
echo "Running all smoke tests..."
cargo test -p kalam-cli --test smoke -- --nocapture

# Kill the server
kill $SERVER_PID
