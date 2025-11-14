#!/usr/bin/env pwsh
# Quick start script for running KalamDB benchmarks

Write-Host "🚀 KalamDB Benchmark Quick Start" -ForegroundColor Cyan
Write-Host ""

# Check if server is running
Write-Host "Checking if KalamDB server is running..." -ForegroundColor Yellow
$serverRunning = Test-NetConnection -ComputerName localhost -Port 8080 -InformationLevel Quiet -WarningAction SilentlyContinue

if (-not $serverRunning) {
    Write-Host "❌ KalamDB server is not running on port 8080" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please start the server first:" -ForegroundColor Yellow
    Write-Host "  cd backend" -ForegroundColor White
    Write-Host "  cargo run --release" -ForegroundColor White
    Write-Host ""
    exit 1
}

Write-Host "✅ Server is running" -ForegroundColor Green
Write-Host ""

# Run benchmarks
Write-Host "Running benchmarks..." -ForegroundColor Yellow
Write-Host ""

Set-Location benchmark

# Run all tests
cargo test --release

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "✅ Benchmarks completed successfully!" -ForegroundColor Green
    Write-Host ""
    Write-Host "📊 View results:" -ForegroundColor Cyan
    Write-Host "  1. Open view/index.html in your browser" -ForegroundColor White
    Write-Host "  2. Drag JSON files from results/ into the viewer" -ForegroundColor White
    Write-Host ""
    
    # List generated files
    $jsonFiles = Get-ChildItem -Path results -Filter "*.json" | Where-Object { $_.Name -ne "sample-bench-0.2.0--012-full-dml-support-2025-11-14-1.json" }
    if ($jsonFiles.Count -gt 0) {
        Write-Host "Generated files:" -ForegroundColor Cyan
        foreach ($file in $jsonFiles) {
            Write-Host "  - results/$($file.Name)" -ForegroundColor White
        }
    }
} else {
    Write-Host ""
    Write-Host "❌ Benchmarks failed" -ForegroundColor Red
    Write-Host "Check the output above for errors" -ForegroundColor Yellow
}

Set-Location ..
