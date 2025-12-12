#!/bin/bash
set -e

echo "🔨 Building KalamDB TypeScript SDK..."

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Clean previous build
echo "🧹 Cleaning previous build..."
rm -rf dist .wasm-out src/wasm

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "📦 Installing dependencies..."
    npm install
fi

# Navigate to link crate root (parent of sdks/)
cd "$SCRIPT_DIR/../.."

# Build WASM using wasm-pack (output to .wasm-out to avoid overwriting package.json)
echo "📦 Compiling Rust to WASM..."
wasm-pack build \
  --target web \
  --out-dir sdks/typescript/.wasm-out \
  --features wasm \
  --no-default-features

# Return to SDK directory
cd "$SCRIPT_DIR"

# Copy WASM files to src/wasm (for TS compilation) and dist/wasm (for output)
echo "📁 Copying WASM files..."
mkdir -p src/wasm dist/wasm
for f in .wasm-out/*; do
    filename=$(basename "$f")
    if [[ "$filename" != "package.json" && "$filename" != ".gitignore" ]]; then
        cp "$f" src/wasm/
        cp "$f" dist/wasm/
    fi
done

# Compile TypeScript
echo "🔧 Compiling TypeScript..."
npx tsc

# Clean up src/wasm (not needed after compilation)
rm -rf src/wasm

echo ""
echo "✅ Build complete!"
echo ""
echo "Output files in dist/:"
echo "  - index.js (TypeScript client)"
echo "  - index.d.ts (TypeScript types)"
echo "  - wasm/kalam_link.js (WASM bindings)"
echo "  - wasm/kalam_link.d.ts (WASM TypeScript definitions)"
echo "  - wasm/kalam_link_bg.wasm (WebAssembly module)"
echo ""
echo "To publish: npm publish"
