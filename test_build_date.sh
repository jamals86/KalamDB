#!/bin/bash
# Quick test of the \info command

echo "Testing CLI with server build date display..."
echo ""

# Send \info command to CLI and show output
echo "\\info
\\quit" | ./target/debug/kalam --no-color 2>&1 | grep -A 50 "Session Information"
