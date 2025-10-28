#!/usr/bin/env bash

# KalamDB Release Builder
# Builds CLI and Server binaries and uploads them to GitHub Releases
#
# Usage:
#   ./tools/release.sh <version> [--draft] [--prerelease]
#
# Example:
#   ./tools/release.sh v0.1.0
#   ./tools/release.sh v0.2.0-beta --prerelease

set -e  # Exit on error
set -u  # Exit on undefined variable

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO_OWNER="jamals86"
REPO_NAME="KalamDB"
BUILD_DIR="target/release"
DIST_DIR="dist"

# Parse arguments
VERSION="${1:-}"
DRAFT_FLAG=""
PRERELEASE_FLAG=""

if [ -z "$VERSION" ]; then
    echo -e "${RED}Error: Version is required${NC}"
    echo "Usage: $0 <version> [--draft] [--prerelease]"
    echo "Example: $0 v0.1.0"
    exit 1
fi

# Parse optional flags
shift
while [ $# -gt 0 ]; do
    case "$1" in
        --draft)
            DRAFT_FLAG="--draft"
            shift
            ;;
        --prerelease)
            PRERELEASE_FLAG="--prerelease"
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Detect OS and architecture
detect_platform() {
    local os=""
    local arch=""
    
    case "$(uname -s)" in
        Linux*)     os="linux";;
        Darwin*)    os="darwin";;
        CYGWIN*|MINGW*|MSYS*) os="windows";;
        *)          os="unknown";;
    esac
    
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64";;
        aarch64|arm64) arch="aarch64";;
        *)            arch="unknown";;
    esac
    
    echo "${os}-${arch}"
}

PLATFORM=$(detect_platform)

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}KalamDB Release Builder${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Version:${NC} $VERSION"
echo -e "${GREEN}Platform:${NC} $PLATFORM"
echo -e "${GREEN}Draft:${NC} ${DRAFT_FLAG:-no}"
echo -e "${GREEN}Prerelease:${NC} ${PRERELEASE_FLAG:-no}"
echo ""

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo -e "${RED}Error: GitHub CLI (gh) is not installed${NC}"
    echo "Install it from: https://cli.github.com/"
    exit 1
fi

# Check if logged in to GitHub
if ! gh auth status &> /dev/null; then
    echo -e "${RED}Error: Not logged in to GitHub${NC}"
    echo "Run: gh auth login"
    exit 1
fi

# Clean previous builds
echo -e "${YELLOW}Cleaning previous builds...${NC}"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# Build CLI
echo -e "${YELLOW}Building CLI...${NC}"
cd cli
cargo build --release
cd ..

# Build Server
echo -e "${YELLOW}Building Server...${NC}"
cd backend
cargo build --release
cd ..

# Determine binary extensions and names
if [[ "$PLATFORM" == *"windows"* ]]; then
    CLI_BIN="kalam-cli.exe"
    SERVER_BIN="kalamdb-server.exe"
else
    CLI_BIN="kalam-cli"
    SERVER_BIN="kalamdb-server"
fi

# Copy binaries to dist
echo -e "${YELLOW}Copying binaries to dist...${NC}"

if [ -f "cli/$BUILD_DIR/$CLI_BIN" ]; then
    cp "cli/$BUILD_DIR/$CLI_BIN" "$DIST_DIR/kalam-cli-${VERSION}-${PLATFORM}${CLI_BIN##*.exe}"
    if [[ "$PLATFORM" != *"windows"* ]]; then
        chmod +x "$DIST_DIR/kalam-cli-${VERSION}-${PLATFORM}"
    fi
    echo -e "${GREEN}✓ CLI binary copied${NC}"
else
    echo -e "${RED}Error: CLI binary not found at cli/$BUILD_DIR/$CLI_BIN${NC}"
    exit 1
fi

if [ -f "backend/$BUILD_DIR/$SERVER_BIN" ]; then
    cp "backend/$BUILD_DIR/$SERVER_BIN" "$DIST_DIR/kalamdb-server-${VERSION}-${PLATFORM}${SERVER_BIN##*.exe}"
    if [[ "$PLATFORM" != *"windows"* ]]; then
        chmod +x "$DIST_DIR/kalamdb-server-${VERSION}-${PLATFORM}"
    fi
    echo -e "${GREEN}✓ Server binary copied${NC}"
else
    echo -e "${RED}Error: Server binary not found at backend/$BUILD_DIR/$SERVER_BIN${NC}"
    exit 1
fi

# Create archives
echo -e "${YELLOW}Creating archives...${NC}"
cd "$DIST_DIR"

if [[ "$PLATFORM" == *"windows"* ]]; then
    # Use zip for Windows
    if command -v zip &> /dev/null; then
        zip "kalam-cli-${VERSION}-${PLATFORM}.zip" "kalam-cli-${VERSION}-${PLATFORM}.exe"
        zip "kalamdb-server-${VERSION}-${PLATFORM}.zip" "kalamdb-server-${VERSION}-${PLATFORM}.exe"
        echo -e "${GREEN}✓ ZIP archives created${NC}"
    else
        echo -e "${YELLOW}Warning: zip not found, skipping archive creation${NC}"
    fi
else
    # Use tar.gz for Unix-like systems
    tar -czf "kalam-cli-${VERSION}-${PLATFORM}.tar.gz" "kalam-cli-${VERSION}-${PLATFORM}"
    tar -czf "kalamdb-server-${VERSION}-${PLATFORM}.tar.gz" "kalamdb-server-${VERSION}-${PLATFORM}"
    echo -e "${GREEN}✓ TAR.GZ archives created${NC}"
fi

cd ..

# Generate checksums
echo -e "${YELLOW}Generating checksums...${NC}"
cd "$DIST_DIR"
if command -v sha256sum &> /dev/null; then
    sha256sum * > SHA256SUMS
elif command -v shasum &> /dev/null; then
    shasum -a 256 * > SHA256SUMS
else
    echo -e "${YELLOW}Warning: sha256sum not found, skipping checksums${NC}"
fi
cd ..

# List files to upload
echo -e "${YELLOW}Files to upload:${NC}"
ls -lh "$DIST_DIR"

# Check if release already exists
echo -e "${YELLOW}Checking if release $VERSION exists...${NC}"
if gh release view "$VERSION" --repo "$REPO_OWNER/$REPO_NAME" &> /dev/null; then
    echo -e "${YELLOW}Release $VERSION already exists${NC}"
    read -p "Do you want to delete and recreate it? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${YELLOW}Deleting existing release...${NC}"
        gh release delete "$VERSION" --repo "$REPO_OWNER/$REPO_NAME" --yes
    else
        echo -e "${RED}Aborted${NC}"
        exit 1
    fi
fi

# Create GitHub release
echo -e "${YELLOW}Creating GitHub release...${NC}"

RELEASE_NOTES="## KalamDB $VERSION

### Binaries

This release includes pre-built binaries for:
- Linux (x86_64, aarch64)
- macOS (x86_64, aarch64)
- Windows (x86_64)

### Installation

#### Linux/macOS
\`\`\`bash
# Download the appropriate binary for your platform
wget https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/$VERSION/kalamdb-server-$VERSION-<platform>.tar.gz
tar -xzf kalamdb-server-$VERSION-<platform>.tar.gz

# Make executable and move to PATH
chmod +x kalamdb-server-$VERSION-<platform>
sudo mv kalamdb-server-$VERSION-<platform> /usr/local/bin/kalamdb-server
\`\`\`

#### Windows
Download the .zip file and extract it to a directory in your PATH.

### Checksums

SHA256 checksums are available in the \`SHA256SUMS\` file.

### Changes

See the [CHANGELOG](https://github.com/$REPO_OWNER/$REPO_NAME/blob/main/CHANGELOG.md) for details.
"

# Create the release
gh release create "$VERSION" \
    --repo "$REPO_OWNER/$REPO_NAME" \
    --title "KalamDB $VERSION" \
    --notes "$RELEASE_NOTES" \
    $DRAFT_FLAG \
    $PRERELEASE_FLAG \
    "$DIST_DIR"/*

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}✓ Release $VERSION created successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}View the release at:${NC}"
echo "https://github.com/$REPO_OWNER/$REPO_NAME/releases/tag/$VERSION"
echo ""

# Clean up
read -p "Do you want to clean up the dist directory? (Y/n): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Nn]$ ]]; then
    echo -e "${YELLOW}Cleaning up...${NC}"
    rm -rf "$DIST_DIR"
    echo -e "${GREEN}✓ Cleaned up${NC}"
fi

echo -e "${GREEN}Done!${NC}"
