# Build script for KalamDB TypeScript SDK (PowerShell)
# Run with: .\build.ps1

$ErrorActionPreference = "Stop"

Write-Host "🔨 Building KalamDB TypeScript SDK..." -ForegroundColor Cyan

# Navigate to link crate root (parent of sdks/)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location (Join-Path $scriptDir "..\..") 

# Backup package.json (wasm-pack overwrites it)
$pkgJson = Join-Path $scriptDir "package.json"
$pkgBackup = Join-Path $scriptDir "package.json.bak"
if (Test-Path $pkgJson) {
    Copy-Item $pkgJson $pkgBackup -Force
    Write-Host "📦 Backed up package.json" -ForegroundColor Yellow
}

# Build WASM using wasm-pack
Write-Host "📦 Compiling Rust to WASM..." -ForegroundColor Yellow
wasm-pack build `
  --target web `
  --out-dir sdks/typescript `
  --features wasm `
  --no-default-features

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ WASM build failed!" -ForegroundColor Red
    # Restore package.json
    if (Test-Path $pkgBackup) {
        Move-Item $pkgBackup $pkgJson -Force
    }
    exit $LASTEXITCODE
}

# Restore package.json
if (Test-Path $pkgBackup) {
    Move-Item $pkgBackup $pkgJson -Force
    Write-Host "📦 Restored package.json" -ForegroundColor Yellow
}

# Compile TypeScript
Write-Host "🔧 Compiling TypeScript..." -ForegroundColor Yellow
Set-Location sdks/typescript
npx tsc

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ TypeScript compilation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "✅ Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Output files:" -ForegroundColor Cyan
Write-Host "  - kalam_link.js (WASM bindings)"
Write-Host "  - kalam_link.d.ts (TypeScript definitions for WASM)"
Write-Host "  - kalam_link_bg.wasm (WebAssembly module)"
Write-Host "  - dist/index.js (TypeScript client)"
Write-Host "  - dist/index.d.ts (TypeScript types)"
