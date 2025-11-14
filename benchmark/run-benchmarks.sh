#!/bin/bash
# Quick start script for running KalamDB benchmarks

echo "🚀 KalamDB Benchmark Quick Start"
echo ""

# Check if server is running
echo "Checking if KalamDB server is running..."
if ! nc -z localhost 8080 2>/dev/null; then
    echo "❌ KalamDB server is not running on port 8080"
    echo ""
    echo "Please start the server first:"
    echo "  cd backend"
    echo "  cargo run --release"
    echo ""
    exit 1
fi

echo "✅ Server is running"
echo ""

# Run benchmarks
echo "Running benchmarks..."
echo ""

cd benchmark

# Run all tests
cargo test --release

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Benchmarks completed successfully!"
    echo ""
    echo "📊 View results:"
    echo "  1. Open view/index.html in your browser"
    echo "  2. Drag JSON files from results/ into the viewer"
    echo ""
    
    # List generated files
    if ls results/*.json 1> /dev/null 2>&1; then
        echo "Generated files:"
        ls -1 results/*.json | grep -v "sample-bench" | sed 's/^/  - /'
    fi
else
    echo ""
    echo "❌ Benchmarks failed"
    echo "Check the output above for errors"
fi

cd ..
