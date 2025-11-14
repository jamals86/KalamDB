#!/usr/bin/env pwsh
# Start the benchmark results viewer

Write-Host "🚀 Starting KalamDB Benchmark Viewer..." -ForegroundColor Cyan
Write-Host ""

Set-Location benchmark
cargo run --bin benchmark-viewer --release
