#!/bin/bash
# Start the benchmark results viewer

echo "🚀 Starting KalamDB Benchmark Viewer..."
echo ""

cd benchmark
cargo run --bin benchmark-viewer --release
