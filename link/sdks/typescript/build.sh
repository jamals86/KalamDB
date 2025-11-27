#!/bin/bash
set -e

echo "🔨 Building KalamDB TypeScript SDK..."

# Navigate to link crate root (parent of sdks/)
cd "$(dirname "$0")/../.."

# Backup package.json (wasm-pack overwrites it)
if [ -f "sdks/typescript/package.json" ]; then
    cp sdks/typescript/package.json sdks/typescript/package.json.bak
    echo "📦 Backed up package.json"
fi

# Build WASM using wasm-pack
echo "📦 Compiling Rust to WASM..."
wasm-pack build \
  --target web \
  --out-dir sdks/typescript \
  --features wasm \
  --no-default-features

# Restore package.json
if [ -f "sdks/typescript/package.json.bak" ]; then
    mv sdks/typescript/package.json.bak sdks/typescript/package.json
    echo "📦 Restored package.json"
fi

# Compile TypeScript
echo "🔧 Compiling TypeScript..."
cd sdks/typescript
npx tsc

# Build example app
echo "📱 Building example app..."
cd example
npm install --silent
echo "✅ Example app ready"

cd ..
echo ""
echo "✅ Build complete!"
echo ""
echo "Output files:"
echo "  - kalam_link.js (WASM bindings)"
echo "  - kalam_link.d.ts (TypeScript definitions for WASM)"
echo "  - kalam_link_bg.wasm (WebAssembly module)"
echo "  - dist/index.js (TypeScript client)"
echo "  - dist/index.d.ts (TypeScript types)"
echo "  - example/ (Browser example app)"
