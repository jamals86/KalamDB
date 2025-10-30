#!/bin/bash
# Test script for CLI improvements

echo "=== Testing CLI Improvements ==="
echo ""

# Test 1: Show version with build info
echo "Test 1: CLI Version with Build Info"
echo "-----------------------------------"
./target/debug/kalam --version
echo ""

# Test 2: Try connecting to localhost (should auto-authenticate with root)
echo "Test 2: Localhost Auto-Authentication"
echo "--------------------------------------"
echo "Attempting to connect to localhost without explicit credentials..."
echo "Expected: Should auto-authenticate with 'root' user"
echo ""
echo "\\info" | ./target/debug/kalam -u http://localhost:8080 --no-color
echo ""

# Test 3: Show help (should include new \info command)
echo "Test 3: Help Text (should include \\info command)"
echo "-------------------------------------------------"
echo "\\help" | ./target/debug/kalam -u http://localhost:8080 --no-color | grep -A 2 "\\\\info"
echo ""

echo "=== Tests Complete ==="
