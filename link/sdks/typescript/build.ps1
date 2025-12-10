# Build script for KalamDB TypeScript SDK (PowerShell)
# Run with: .\build.ps1

$ErrorActionPreference = "Stop"

Write-Host "🔨 Building KalamDB TypeScript SDK..." -ForegroundColor Cyan

# Get script directory
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

# Clean previous build
Write-Host "🧹 Cleaning previous build..." -ForegroundColor Yellow
if (Test-Path "dist") { Remove-Item -Recurse -Force "dist" }
if (Test-Path ".wasm-out") { Remove-Item -Recurse -Force ".wasm-out" }
if (Test-Path "src/wasm") { Remove-Item -Recurse -Force "src/wasm" }

# Install dependencies if needed
if (-not (Test-Path "node_modules")) {
    Write-Host "📦 Installing dependencies..." -ForegroundColor Yellow
    npm install
}

# Navigate to link crate root (parent of sdks/)
Set-Location (Join-Path $scriptDir "..\..") 

# Build WASM using wasm-pack (output to .wasm-out to avoid overwriting package.json)
Write-Host "📦 Compiling Rust to WASM..." -ForegroundColor Yellow
wasm-pack build `
  --target web `
  --out-dir sdks/typescript/.wasm-out `
  --features wasm `
  --no-default-features

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ WASM build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

# Return to SDK directory
Set-Location $scriptDir

# Copy WASM files to src/wasm (for TypeScript compilation) and dist/wasm (for output)
Write-Host "📁 Copying WASM files..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path "src/wasm" | Out-Null
New-Item -ItemType Directory -Force -Path "dist/wasm" | Out-Null
Get-ChildItem ".wasm-out" -File | Where-Object { $_.Name -notmatch "package\.json|\.gitignore" } | ForEach-Object {
    Copy-Item $_.FullName -Destination "src/wasm/"
    Copy-Item $_.FullName -Destination "dist/wasm/"
}

# Compile TypeScript
Write-Host "🔧 Compiling TypeScript..." -ForegroundColor Yellow
npx tsc

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ TypeScript compilation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

# Clean up src/wasm (not needed after compilation)
Remove-Item -Recurse -Force "src/wasm"

Write-Host "✅ Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Output files in dist/:" -ForegroundColor Cyan
Write-Host "  - index.js (TypeScript client)"
Write-Host "  - index.d.ts (TypeScript types)"
Write-Host "  - wasm/kalam_link.js (WASM bindings)"
Write-Host "  - wasm/kalam_link.d.ts (WASM TypeScript definitions)"
Write-Host "  - wasm/kalam_link_bg.wasm (WebAssembly module)"
Write-Host ""
Write-Host "To publish: npm publish" -ForegroundColor Cyan
