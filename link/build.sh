#!/bin/bash
#
# WASM Build Script for kalam-link
# 
# This script compiles the Rust library to WebAssembly and generates
# TypeScript bindings for browser and Node.js usage.
#
# Requirements:
#   - Rust 1.75+ with wasm32-unknown-unknown target
#   - wasm-pack 0.12+
#
# Usage:
#   ./build.sh [--release] [--target <web|nodejs|bundler>]
#
# Outputs:
#   pkg/ - WASM module with JavaScript/TypeScript bindings

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
BUILD_TYPE="dev"
TARGET="web"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_TYPE="release"
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--release] [--target <web|nodejs|bundler>]"
            echo ""
            echo "Options:"
            echo "  --release       Build in release mode (optimized)"
            echo "  --target        Target platform (web, nodejs, bundler)"
            echo "  -h, --help      Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}  KalamDB WASM Build Script${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

if ! command -v rustc &> /dev/null; then
    echo -e "${RED}Error: Rust is not installed${NC}"
    echo "Install from: https://rustup.rs/"
    exit 1
fi

if ! command -v wasm-pack &> /dev/null; then
    echo -e "${RED}Error: wasm-pack is not installed${NC}"
    echo "Install with: cargo install wasm-pack"
    exit 1
fi

# Check wasm32-unknown-unknown target
if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo -e "${YELLOW}Installing wasm32-unknown-unknown target...${NC}"
    rustup target add wasm32-unknown-unknown
fi

echo -e "${GREEN}✓ Prerequisites OK${NC}"
echo ""

# Build WASM module
echo -e "${YELLOW}Building WASM module...${NC}"
echo "  Build type: $BUILD_TYPE"
echo "  Target: $TARGET"
echo ""

# Construct wasm-pack command  
WASM_PACK_CMD="wasm-pack build --target $TARGET --out-dir pkg --no-default-features --features wasm"

if [ "$BUILD_TYPE" = "release" ]; then
    WASM_PACK_CMD="$WASM_PACK_CMD --release"
fi

# Run wasm-pack
eval $WASM_PACK_CMD

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}✓ WASM build successful${NC}"
else
    echo -e "${RED}Error: WASM build failed${NC}"
    exit 1
fi

# Verify output
echo ""
echo -e "${YELLOW}Verifying output files...${NC}"

REQUIRED_FILES=(
    "pkg/kalam_link_bg.wasm"
    "pkg/kalam_link.js"
    "pkg/kalam_link.d.ts"
    "pkg/package.json"
)

ALL_PRESENT=true
for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$file" ]; then
        SIZE=$(du -h "$file" | cut -f1)
        echo -e "  ${GREEN}✓${NC} $file ($SIZE)"
    else
        echo -e "  ${RED}✗${NC} $file (missing)"
        ALL_PRESENT=false
    fi
done

if [ "$ALL_PRESENT" = false ]; then
    echo -e "${RED}Error: Some output files are missing${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}✓ All output files present${NC}"

# Enhance package.json with better metadata
echo ""
echo -e "${YELLOW}Enhancing package.json...${NC}"

# Create enhanced package.json
cat > pkg/package.json << 'EOF'
{
  "name": "@kalamdb/sdk",
  "version": "0.1.0",
  "description": "TypeScript/JavaScript SDK for KalamDB - WebAssembly-powered database client",
  "main": "kalam_link.js",
  "types": "kalam_link.d.ts",
  "type": "module",
  "files": [
    "kalam_link_bg.wasm",
    "kalam_link.js",
    "kalam_link.d.ts",
    "kalam_link_bg.wasm.d.ts",
    "package.json",
    "README.md"
  ],
  "keywords": [
    "kalamdb",
    "database",
    "client",
    "websocket",
    "wasm",
    "webassembly",
    "realtime",
    "typescript",
    "javascript"
  ],
  "author": "KalamDB Team",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/jamals86/KalamDB.git",
    "directory": "link"
  },
  "bugs": {
    "url": "https://github.com/jamals86/KalamDB/issues"
  },
  "homepage": "https://github.com/jamals86/KalamDB#readme",
  "engines": {
    "node": ">=18.0.0"
  }
}
EOF

echo -e "${GREEN}✓ package.json enhanced${NC}"

# Print usage instructions
echo ""
echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}  Build Complete!${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""
echo -e "Output location:"
echo -e "  - WASM SDK: ${GREEN}pkg/${NC}"
echo ""
echo -e "Usage in TypeScript/JavaScript:"
echo -e "${YELLOW}import init, { KalamClient } from './pkg/kalam_link.js';${NC}"
echo ""
echo -e "${YELLOW}await init();${NC}"
echo -e "${YELLOW}const client = new KalamClient('ws://localhost:8080', 'your-api-key');${NC}"
echo -e "${YELLOW}await client.connect();${NC}"
echo ""
echo -e "Next steps:"
echo -e "  1. Test the WASM module: ${GREEN}node test-wasm.mjs${NC}"
echo -e "  2. Publish to npm: ${GREEN}cd pkg && npm publish${NC}"
echo -e "  3. Use in your app: ${GREEN}npm install kalam-link${NC}"
echo ""
