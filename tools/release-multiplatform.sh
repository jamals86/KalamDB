#!/usr/bin/env bash

# KalamDB Multi-Platform Release Builder
# Builds CLI and Server binaries for Linux, macOS, and Windows using Docker
#
# Usage:
#   ./tools/release-multiplatform.sh <version> [--draft] [--prerelease] [--platforms=<list>]
#
# Examples:
#   ./tools/release-multiplatform.sh v0.1.0
#   ./tools/release-multiplatform.sh v0.2.0-beta --prerelease
#   ./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,windows-x86_64

set -e  # Exit on error
set -u  # Exit on undefined variable

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
REPO_OWNER="jamals86"
REPO_NAME="KalamDB"
DIST_DIR="dist"
DOCKER_IMAGE="kalamdb-builder"

# All supported platforms
ALL_PLATFORMS=(
    "linux-x86_64:x86_64-unknown-linux-gnu"
    "linux-aarch64:aarch64-unknown-linux-gnu"
    "windows-x86_64:x86_64-pc-windows-gnu"
    "darwin-x86_64:x86_64-apple-darwin"
    "darwin-aarch64:aarch64-apple-darwin"
)

# Parse arguments
VERSION="${1:-}"
DRAFT_FLAG=""
PRERELEASE_FLAG=""
SELECTED_PLATFORMS=()

if [ -z "$VERSION" ]; then
    echo -e "${RED}Error: Version is required${NC}"
    echo "Usage: $0 <version> [--draft] [--prerelease] [--platforms=<list>]"
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
        --platforms=*)
            IFS=',' read -ra SELECTED_PLATFORMS <<< "${1#*=}"
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# If no platforms selected, use all
if [ ${#SELECTED_PLATFORMS[@]} -eq 0 ]; then
    for platform in "${ALL_PLATFORMS[@]}"; do
        SELECTED_PLATFORMS+=("${platform%%:*}")
    done
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}KalamDB Multi-Platform Release Builder${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Version:${NC} $VERSION"
echo -e "${GREEN}Platforms:${NC} ${SELECTED_PLATFORMS[*]}"
echo -e "${GREEN}Draft:${NC} ${DRAFT_FLAG:-no}"
echo -e "${GREEN}Prerelease:${NC} ${PRERELEASE_FLAG:-no}"
echo ""

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    echo "Install it from: https://docs.docker.com/get-docker/"
    exit 1
fi

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

# Build Docker image
echo -e "${YELLOW}Building Docker cross-compilation image...${NC}"
echo -e "${CYAN}This may take several minutes on first run...${NC}"
cd "$(dirname "$0")"
docker build -t "$DOCKER_IMAGE" -f Dockerfile.builder .
cd ..

# Clean previous builds
echo -e "${YELLOW}Cleaning previous builds...${NC}"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# Function to build for a specific target
build_target() {
    local platform_name="$1"
    local rust_target="$2"
    local cli_binary="kalam-cli"
    local server_binary="kalamdb-server"
    
    # Add .exe extension for Windows
    if [[ "$platform_name" == *"windows"* ]]; then
        cli_binary="${cli_binary}.exe"
        server_binary="${server_binary}.exe"
    fi
    
    echo -e "${CYAN}Building for $platform_name ($rust_target)...${NC}"
    
    # Build CLI
    echo -e "  ${YELLOW}→ Building CLI...${NC}"
    docker run --rm \
        -v "$(pwd):/workspace" \
        -w /workspace/cli \
        "$DOCKER_IMAGE" \
        cargo build --release --target "$rust_target"
    
    # Build Server
    echo -e "  ${YELLOW}→ Building Server...${NC}"
    docker run --rm \
        -v "$(pwd):/workspace" \
        -w /workspace/backend \
        "$DOCKER_IMAGE" \
        cargo build --release --target "$rust_target"
    
    # Copy binaries
    local cli_src="cli/target/${rust_target}/release/${cli_binary}"
    local server_src="backend/target/${rust_target}/release/${server_binary}"
    local cli_dest="${DIST_DIR}/kalam-cli-${VERSION}-${platform_name}"
    local server_dest="${DIST_DIR}/kalamdb-server-${VERSION}-${platform_name}"
    
    # Add .exe extension for Windows destinations
    if [[ "$platform_name" == *"windows"* ]]; then
        cli_dest="${cli_dest}.exe"
        server_dest="${server_dest}.exe"
    fi
    
    if [ -f "$cli_src" ]; then
        cp "$cli_src" "$cli_dest"
        echo -e "  ${GREEN}✓ CLI binary: $cli_dest${NC}"
    else
        echo -e "  ${RED}✗ CLI binary not found: $cli_src${NC}"
        return 1
    fi
    
    if [ -f "$server_src" ]; then
        cp "$server_src" "$server_dest"
        echo -e "  ${GREEN}✓ Server binary: $server_dest${NC}"
    else
        echo -e "  ${RED}✗ Server binary not found: $server_src${NC}"
        return 1
    fi
    
    # Make binaries executable on Unix-like systems
    if [[ "$platform_name" != *"windows"* ]]; then
        chmod +x "$cli_dest" "$server_dest"
    fi
    
    echo ""
}

# Build for all selected platforms
for platform in "${ALL_PLATFORMS[@]}"; do
    platform_name="${platform%%:*}"
    rust_target="${platform##*:}"
    
    # Check if this platform is selected
    if [[ " ${SELECTED_PLATFORMS[*]} " =~ " ${platform_name} " ]]; then
        if ! build_target "$platform_name" "$rust_target"; then
            echo -e "${RED}Failed to build for $platform_name${NC}"
            exit 1
        fi
    fi
done

# Create archives
echo -e "${YELLOW}Creating archives...${NC}"
cd "$DIST_DIR"

for file in kalam-cli-* kalamdb-server-*; do
    if [ -f "$file" ] && [[ ! "$file" =~ \.(tar\.gz|zip)$ ]]; then
        if [[ "$file" == *".exe" ]]; then
            # Create zip for Windows binaries
            zip "${file%.exe}.zip" "$file"
            echo -e "${GREEN}✓ Created ${file%.exe}.zip${NC}"
        else
            # Create tar.gz for Unix binaries
            tar -czf "${file}.tar.gz" "$file"
            echo -e "${GREEN}✓ Created ${file}.tar.gz${NC}"
        fi
    fi
done

# Generate checksums
echo -e "${YELLOW}Generating checksums...${NC}"
if command -v sha256sum &> /dev/null; then
    sha256sum *.tar.gz *.zip 2>/dev/null > SHA256SUMS || true
elif command -v shasum &> /dev/null; then
    shasum -a 256 *.tar.gz *.zip 2>/dev/null > SHA256SUMS || true
fi
echo -e "${GREEN}✓ Checksums generated${NC}"

cd ..

# List files to upload
echo -e "${YELLOW}Files to upload:${NC}"
ls -lh "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.zip 2>/dev/null || true

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

### Pre-built Binaries

This release includes pre-built binaries for multiple platforms:

#### Linux
- **x86_64** (AMD/Intel 64-bit): \`kalamdb-server-$VERSION-linux-x86_64.tar.gz\`
- **aarch64** (ARM 64-bit): \`kalamdb-server-$VERSION-linux-aarch64.tar.gz\`

#### macOS
- **x86_64** (Intel Macs): \`kalamdb-server-$VERSION-darwin-x86_64.tar.gz\`
- **aarch64** (Apple Silicon): \`kalamdb-server-$VERSION-darwin-aarch64.tar.gz\`

#### Windows
- **x86_64** (64-bit): \`kalamdb-server-$VERSION-windows-x86_64.zip\`

### Installation

#### Linux/macOS
\`\`\`bash
# Download the appropriate binary for your platform
wget https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/$VERSION/kalamdb-server-$VERSION-<platform>.tar.gz

# Extract
tar -xzf kalamdb-server-$VERSION-<platform>.tar.gz

# Make executable and move to PATH
chmod +x kalamdb-server-$VERSION-<platform>
sudo mv kalamdb-server-$VERSION-<platform> /usr/local/bin/kalamdb-server

# Same for CLI
wget https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/$VERSION/kalam-cli-$VERSION-<platform>.tar.gz
tar -xzf kalam-cli-$VERSION-<platform>.tar.gz
chmod +x kalam-cli-$VERSION-<platform>
sudo mv kalam-cli-$VERSION-<platform> /usr/local/bin/kalam
\`\`\`

#### Windows
1. Download the \`.zip\` file for your platform
2. Extract to a directory (e.g., \`C:\\Program Files\\KalamDB\`)
3. Add the directory to your PATH environment variable

### Checksums

Verify downloads with SHA256:
\`\`\`bash
# Download checksums
wget https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/$VERSION/SHA256SUMS

# Verify (Linux/macOS)
sha256sum -c SHA256SUMS
\`\`\`

### Changes

See the [CHANGELOG](https://github.com/$REPO_OWNER/$REPO_NAME/blob/main/CHANGELOG.md) for details.

---
Built with ❤️ using Docker cross-compilation
"

# Upload all archives and checksums
UPLOAD_FILES=("$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.zip "$DIST_DIR"/SHA256SUMS)

# Create the release
gh release create "$VERSION" \
    --repo "$REPO_OWNER/$REPO_NAME" \
    --title "KalamDB $VERSION" \
    --notes "$RELEASE_NOTES" \
    $DRAFT_FLAG \
    $PRERELEASE_FLAG \
    "${UPLOAD_FILES[@]}"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}✓ Release $VERSION created successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}View the release at:${NC}"
echo "https://github.com/$REPO_OWNER/$REPO_NAME/releases/tag/$VERSION"
echo ""
echo -e "${CYAN}Platforms built:${NC}"
for platform in "${SELECTED_PLATFORMS[@]}"; do
    echo -e "  ✓ $platform"
done
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
