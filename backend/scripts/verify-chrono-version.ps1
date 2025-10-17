#!/usr/bin/env pwsh
# Verify chrono version to prevent regression of the Arrow 52.2.0 conflict
# Run this script in CI or before commits to ensure chrono stays pinned to 0.4.39

$ErrorActionPreference = "Stop"

Write-Host "🔍 Checking chrono version..." -ForegroundColor Cyan

# Get chrono version from dependency tree
$chronoVersion = cargo tree -i chrono | Select-String "^chrono v" | Select-Object -First 1

if ($chronoVersion -match "chrono v0\.4\.39") {
    Write-Host "✅ SUCCESS: chrono is correctly pinned to 0.4.39" -ForegroundColor Green
    
    # Check for duplicates
    $chronoDuplicates = cargo tree -i chrono -d 2>&1
    if ($chronoDuplicates -match "warning: nothing to print") {
        Write-Host "✅ SUCCESS: No duplicate chrono versions found" -ForegroundColor Green
        Write-Host ""
        Write-Host "All checks passed! Arrow 52.2.0 conflict is resolved." -ForegroundColor Green
        exit 0
    } else {
        Write-Host "⚠️  WARNING: Multiple chrono versions detected!" -ForegroundColor Yellow
        Write-Host $chronoDuplicates
        exit 1
    }
} elseif ($chronoVersion -match "chrono v0\.4\.([4-9][0-9]|40)") {
    Write-Host "❌ FAILED: chrono is at version 0.4.40+ which conflicts with arrow-arith 52.2.0" -ForegroundColor Red
    Write-Host "   Current version: $chronoVersion" -ForegroundColor Red
    Write-Host ""
    Write-Host "To fix, run:" -ForegroundColor Yellow
    Write-Host "  cargo update -p chrono --precise 0.4.39" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "See backend/KNOWN_ISSUES.md for details." -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "⚠️  WARNING: Unexpected chrono version detected" -ForegroundColor Yellow
    Write-Host "   Current version: $chronoVersion" -ForegroundColor Yellow
    Write-Host "   Expected: chrono v0.4.39" -ForegroundColor Yellow
    exit 1
}
