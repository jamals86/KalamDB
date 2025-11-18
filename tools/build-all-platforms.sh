#!/usr/bin/env bash

# KalamDB Multi-Platform Builder
# Builds CLI and Server binaries for all supported platforms
#
# Usage:
#   ./tools/build-all-platforms.sh <version>
#
# Example:
#   ./tools/build-all-platforms.sh 0.1.0

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
TOOLS_DIR="${ROOT_DIR}/tools"
DIST_DIR="dist/${VERSION}"

# All supported platforms
PLATFORMS=(
    "linux-x86_64"
    "linux-aarch64"
    "macos-x86_64"
    "macos-aarch64"
    "windows-x86_64"
)

# Usage message
usage() {
    echo "Usage: $0 <version>"
    echo ""
    echo "This script builds binaries for all supported platforms:"
    for platform in "${PLATFORMS[@]}"; do
        echo "  - $platform"
    done
    echo ""
    echo "Example:"
    echo "  $0 0.1.0"
    exit 1
}

# Validate arguments
if [ -z "$VERSION" ]; then
    echo -e "${RED}Error: Version is required${NC}"
    usage
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}KalamDB Multi-Platform Builder${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Version:${NC} $VERSION"
echo -e "${GREEN}Platforms:${NC} ${#PLATFORMS[@]}"
echo ""

# Track build results
SUCCESSFUL_BUILDS=()
FAILED_BUILDS=()

# Build for each platform
for platform in "${PLATFORMS[@]}"; do
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Building for: $platform${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    
    if "${TOOLS_DIR}/build-platform.sh" "$VERSION" "$platform"; then
        SUCCESSFUL_BUILDS+=("$platform")
        echo -e "${GREEN}✓ $platform build successful${NC}"
    else
        FAILED_BUILDS+=("$platform")
        echo -e "${RED}✗ $platform build failed${NC}"
    fi
done

# Generate combined checksums
echo ""
echo -e "${YELLOW}Generating combined checksums...${NC}"
cd "$DIST_DIR"
if command -v sha256sum &> /dev/null; then
    sha256sum * > SHA256SUMS 2>/dev/null || true
elif command -v shasum &> /dev/null; then
    shasum -a 256 * > SHA256SUMS 2>/dev/null || true
fi
cd - > /dev/null

# Summary
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Build Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

if [ ${#SUCCESSFUL_BUILDS[@]} -gt 0 ]; then
    echo -e "${GREEN}Successful builds (${#SUCCESSFUL_BUILDS[@]}):${NC}"
    for platform in "${SUCCESSFUL_BUILDS[@]}"; do
        echo -e "  ${GREEN}✓${NC} $platform"
    done
    echo ""
fi

if [ ${#FAILED_BUILDS[@]} -gt 0 ]; then
    echo -e "${RED}Failed builds (${#FAILED_BUILDS[@]}):${NC}"
    for platform in "${FAILED_BUILDS[@]}"; do
        echo -e "  ${RED}✗${NC} $platform"
    done
    echo ""
fi

# List all output files
echo -e "${BLUE}All output files in ${DIST_DIR}:${NC}"
ls -lh "$DIST_DIR"
echo ""

# Calculate total size
TOTAL_SIZE=$(du -sh "$DIST_DIR" | cut -f1)
echo -e "${BLUE}Total size:${NC} $TOTAL_SIZE"
echo ""

# Show next steps
if [ ${#SUCCESSFUL_BUILDS[@]} -gt 0 ]; then
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. Test the binaries:"
    echo "   ./dist/${VERSION}/kalam-${VERSION}-linux-x86_64 --version"
    echo "   ./dist/${VERSION}/kalamdb-server-${VERSION}-macos-aarch64 --version"
    echo ""
    echo "2. Upload to GitHub releases:"
    echo "   gh release create v${VERSION} ./dist/${VERSION}/*"
    echo ""
fi

# Exit with error if any builds failed
if [ ${#FAILED_BUILDS[@]} -gt 0 ]; then
    echo -e "${RED}Some builds failed. Please check the output above.${NC}"
    exit 1
fi

echo -e "${GREEN}All builds completed successfully!${NC}"
