#!/usr/bin/env bash

# KalamDB Platform-Specific Builder
# Builds CLI and Server binaries for a specific platform with conventional naming
#
# Usage:
#   ./tools/build-platform.sh <version> <platform>
#
# Platforms:
#   linux-x86_64, linux-aarch64
#   macos-x86_64, macos-aarch64
#   windows-x86_64
#
# Example:
#   ./tools/build-platform.sh 0.1.0 linux-x86_64
#   ./tools/build-platform.sh 0.1.0 macos-aarch64
#   ./tools/build-platform.sh 0.1.0 windows-x86_64

set -e  # Exit on error
set -u  # Exit on undefined variable

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
ROOT_DIR="$(pwd)"
VERSION="${1:-}"
PLATFORM="${2:-}"

# Usage message
usage() {
    echo "Usage: $0 <version> <platform>"
    echo ""
    echo "Platforms:"
    echo "  linux-x86_64      Linux 64-bit (Intel/AMD)"
    echo "  linux-aarch64     Linux ARM64"
    echo "  macos-x86_64      macOS Intel"
    echo "  macos-aarch64     macOS Apple Silicon"
    echo "  windows-x86_64    Windows 64-bit"
    echo ""
    echo "Example:"
    echo "  $0 0.1.0 linux-x86_64"
    exit 1
}

# Validate arguments
if [ -z "$VERSION" ] || [ -z "$PLATFORM" ]; then
    echo -e "${RED}Error: Version and platform are required${NC}"
    usage
fi

# Validate platform
case "$PLATFORM" in
    linux-x86_64|linux-aarch64|macos-x86_64|macos-aarch64|windows-x86_64)
        ;;
    *)
        echo -e "${RED}Error: Invalid platform '$PLATFORM'${NC}"
        usage
        ;;
esac

# Determine target triple and build configuration
TARGET=""
CROSS_COMPILE=false
FILE_EXT=""

case "$PLATFORM" in
    linux-x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        FILE_EXT=""
        ;;
    linux-aarch64)
        TARGET="aarch64-unknown-linux-gnu"
        CROSS_COMPILE=true
        FILE_EXT=""
        ;;
    macos-x86_64)
        TARGET="x86_64-apple-darwin"
        FILE_EXT=""
        ;;
    macos-aarch64)
        TARGET="aarch64-apple-darwin"
        FILE_EXT=""
        ;;
    windows-x86_64)
        TARGET="x86_64-pc-windows-gnu"
        CROSS_COMPILE=true
        FILE_EXT=".exe"
        ;;
esac

# Determine binary names based on platform
if [ "$PLATFORM" == "windows-x86_64" ]; then
    CLI_BIN="kalam.exe"
    SERVER_BIN="kalamdb-server.exe"
else
    CLI_BIN="kalam"
    SERVER_BIN="kalamdb-server"
fi

# Output directory
DIST_DIR="dist/${VERSION}"
BUILD_DIR="${ROOT_DIR}/target/${TARGET}/release"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}KalamDB Platform Builder${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Version:${NC}      $VERSION"
echo -e "${GREEN}Platform:${NC}     $PLATFORM"
echo -e "${GREEN}Target:${NC}       $TARGET"
echo -e "${GREEN}Output Dir:${NC}   $DIST_DIR"
echo -e "${GREEN}Cross-compile:${NC} $CROSS_COMPILE"
echo ""

# Create dist directory
echo -e "${YELLOW}Creating output directory...${NC}"
mkdir -p "$DIST_DIR"

# Check if cross-compilation is needed and if cross is installed
if [ "$CROSS_COMPILE" = true ]; then
    if ! command -v cross &> /dev/null; then
        echo -e "${YELLOW}Cross-compilation required but 'cross' is not installed${NC}"
        echo -e "${YELLOW}Installing cross...${NC}"
        cargo install cross --git https://github.com/cross-rs/cross
    fi
    BUILD_CMD="cross"
else
    BUILD_CMD="cargo"
fi

# Add target if not already installed
echo -e "${YELLOW}Adding Rust target $TARGET...${NC}"
rustup target add "$TARGET" || true

# Set up environment for macOS builds (RocksDB requires libclang)
if [[ "$PLATFORM" == macos-* ]] && [[ "$(uname -s)" == "Darwin" ]]; then
    echo -e "${YELLOW}Configuring macOS build environment...${NC}"
    
    # Try to find libclang from Xcode or Homebrew
    if [ -d "$(xcode-select -p 2>/dev/null)/Toolchains/XcodeDefault.xctoolchain/usr/lib" ]; then
        XCODE_LIB="$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib"
        export DYLD_LIBRARY_PATH="${XCODE_LIB}:${DYLD_LIBRARY_PATH:-}"
        export LIBCLANG_PATH="${XCODE_LIB}"
        export BINDGEN_EXTRA_CLANG_ARGS="-I$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/include"
        echo -e "${GREEN}✓ Using Xcode libclang at ${XCODE_LIB}${NC}"
    elif [ -d "/opt/homebrew/opt/llvm/lib" ]; then
        export DYLD_LIBRARY_PATH="/opt/homebrew/opt/llvm/lib:${DYLD_LIBRARY_PATH:-}"
        export LIBCLANG_PATH="/opt/homebrew/opt/llvm/lib"
        export BINDGEN_EXTRA_CLANG_ARGS="-I/opt/homebrew/opt/llvm/include"
        echo -e "${GREEN}✓ Using Homebrew LLVM${NC}"
    elif [ -d "/usr/local/opt/llvm/lib" ]; then
        export DYLD_LIBRARY_PATH="/usr/local/opt/llvm/lib:${DYLD_LIBRARY_PATH:-}"
        export LIBCLANG_PATH="/usr/local/opt/llvm/lib"
        export BINDGEN_EXTRA_CLANG_ARGS="-I/usr/local/opt/llvm/include"
        echo -e "${GREEN}✓ Using Homebrew LLVM (Intel)${NC}"
    else
        echo -e "${YELLOW}Warning: Could not find libclang, build may fail${NC}"
        echo -e "${YELLOW}Install Xcode Command Line Tools: xcode-select --install${NC}"
        echo -e "${YELLOW}Or install LLVM via Homebrew: brew install llvm${NC}"
    fi
fi

# Build the workspace for the target platform
echo -e "${YELLOW}Building workspace for $PLATFORM...${NC}"
$BUILD_CMD build --release --target "$TARGET"

# Check if binaries exist
if [ ! -f "$BUILD_DIR/$CLI_BIN" ]; then
    echo -e "${RED}Error: CLI binary not found at $BUILD_DIR/$CLI_BIN${NC}"
    exit 1
fi

if [ ! -f "$BUILD_DIR/$SERVER_BIN" ]; then
    echo -e "${RED}Error: Server binary not found at $BUILD_DIR/$SERVER_BIN${NC}"
    exit 1
fi

# Copy and rename binaries with conventional naming
echo -e "${YELLOW}Copying binaries with conventional naming...${NC}"

CLI_OUTPUT="kalam-${VERSION}-${PLATFORM}${FILE_EXT}"
SERVER_OUTPUT="kalamdb-server-${VERSION}-${PLATFORM}${FILE_EXT}"

cp "$BUILD_DIR/$CLI_BIN" "$DIST_DIR/$CLI_OUTPUT"
cp "$BUILD_DIR/$SERVER_BIN" "$DIST_DIR/$SERVER_OUTPUT"

# Make executable on Unix-like systems
if [ -z "$FILE_EXT" ]; then
    chmod +x "$DIST_DIR/$CLI_OUTPUT"
    chmod +x "$DIST_DIR/$SERVER_OUTPUT"
fi

echo -e "${GREEN}✓ CLI binary:    $CLI_OUTPUT${NC}"
echo -e "${GREEN}✓ Server binary: $SERVER_OUTPUT${NC}"

# Create archives
echo -e "${YELLOW}Creating archives...${NC}"
cd "$DIST_DIR"

if [ "$PLATFORM" == "windows-x86_64" ]; then
    # Use zip for Windows
    if command -v zip &> /dev/null; then
        zip "${CLI_OUTPUT%.exe}.zip" "$CLI_OUTPUT"
        zip "${SERVER_OUTPUT%.exe}.zip" "$SERVER_OUTPUT"
        echo -e "${GREEN}✓ ZIP archives created${NC}"
    else
        echo -e "${YELLOW}Warning: zip not found, skipping archive creation${NC}"
    fi
else
    # Use tar.gz for Unix-like systems
    tar -czf "${CLI_OUTPUT}.tar.gz" "$CLI_OUTPUT"
    tar -czf "${SERVER_OUTPUT}.tar.gz" "$SERVER_OUTPUT"
    echo -e "${GREEN}✓ TAR.GZ archives created${NC}"
fi

cd - > /dev/null

# Generate checksums for this platform
echo -e "${YELLOW}Generating checksums...${NC}"
cd "$DIST_DIR"
if command -v sha256sum &> /dev/null; then
    sha256sum kalam-${VERSION}-${PLATFORM}* kalamdb-server-${VERSION}-${PLATFORM}* > "SHA256SUMS-${PLATFORM}"
elif command -v shasum &> /dev/null; then
    shasum -a 256 kalam-${VERSION}-${PLATFORM}* kalamdb-server-${VERSION}-${PLATFORM}* > "SHA256SUMS-${PLATFORM}"
else
    echo -e "${YELLOW}Warning: No checksum tool found, skipping checksums${NC}"
fi
cd - > /dev/null

# List output files
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Build complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Output files in ${DIST_DIR}:${NC}"
ls -lh "$DIST_DIR" | grep "$PLATFORM"
echo ""
echo -e "${BLUE}Binary sizes:${NC}"
du -h "$DIST_DIR/$CLI_OUTPUT"
du -h "$DIST_DIR/$SERVER_OUTPUT"
echo ""

# Show next steps
echo -e "${BLUE}Next steps:${NC}"
echo "1. Test the binaries:"
echo "   $DIST_DIR/$CLI_OUTPUT --version"
echo "   $DIST_DIR/$SERVER_OUTPUT --version"
echo ""
echo "2. To build for all platforms, run:"
echo "   ./tools/build-all-platforms.sh $VERSION"
echo ""
echo -e "${GREEN}Done!${NC}"
