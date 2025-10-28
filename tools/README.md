# KalamDB Tools

This directory contains scripts and tools for building, testing, and releasing KalamDB.

## Scripts

### `release-multiplatform.sh` (Recommended)

**Multi-platform release builder** that uses Docker to cross-compile binaries for Linux, macOS, and Windows from any host OS.

**Prerequisites:**
- [Docker](https://docs.docker.com/get-docker/) installed and running
- [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated
- Rust toolchain installed (for the build container)
- At least 10GB free disk space (for Docker image)

**Usage:**

```bash
./tools/release-multiplatform.sh <version> [--draft] [--prerelease] [--platforms=<list>]
```

**Examples:**

```bash
# Build for ALL platforms (Linux x64/ARM, macOS x64/ARM, Windows x64)
./tools/release-multiplatform.sh v0.1.0

# Build only for Linux and Windows
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,windows-x86_64

# Create a draft pre-release for all platforms
./tools/release-multiplatform.sh v0.2.0-rc1 --draft --prerelease
```

**Supported Platforms:**

- `linux-x86_64` - Linux AMD/Intel 64-bit
- `linux-aarch64` - Linux ARM 64-bit
- `darwin-x86_64` - macOS Intel
- `darwin-aarch64` - macOS Apple Silicon
- `windows-x86_64` - Windows 64-bit

**What it does:**

1. 🐳 Builds a Docker image with cross-compilation toolchains (first run only)
2. 🔨 Compiles CLI and Server for all selected platforms
3. 📦 Creates platform-specific archives (`.tar.gz` or `.zip`)
4. 🔐 Generates SHA256 checksums
5. 🚀 Uploads everything to GitHub Releases
6. 📝 Generates comprehensive release notes

**First Run:**

The first time you run this script, it will build a Docker image (~5-10 minutes). This includes:
- Rust toolchain
- Cross-compilation tools for Windows (MinGW)
- Cross-compilation tools for ARM Linux
- OSXCross for macOS binaries

Subsequent runs are much faster as they reuse the cached image.

---

### `release.sh` (Single Platform)

Automated release script that builds both the CLI and server binaries for your **current platform** and uploads them to GitHub Releases.

**Prerequisites:**
- [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated
- Rust toolchain installed
- `tar` and `gzip` (for Linux/macOS) or `zip` (for Windows)
- `sha256sum` or `shasum` for generating checksums

**Usage:**

```bash
./tools/release.sh <version> [--draft] [--prerelease]
```

**Examples:**

```bash
# Create a stable release
./tools/release.sh v0.1.0

# Create a draft release (not published immediately)
./tools/release.sh v0.1.0 --draft

# Create a pre-release (beta, rc, etc.)
./tools/release.sh v0.2.0-beta --prerelease

# Create a draft pre-release
./tools/release.sh v0.2.0-rc1 --draft --prerelease
```

**What it does:**

1. ✅ Validates GitHub CLI installation and authentication
2. 🧹 Cleans previous builds
3. 🔨 Builds CLI (`cli/` directory)
4. 🔨 Builds Server (`backend/` directory)
5. 📦 Creates platform-specific archives (`.tar.gz` or `.zip`)
6. 🔐 Generates SHA256 checksums
7. 🚀 Creates GitHub release with all artifacts
8. 📝 Generates release notes with installation instructions

**Output:**

The script creates a `dist/` directory containing:
- `kalam-cli-<version>-<platform>` - CLI binary
- `kalamdb-server-<version>-<platform>` - Server binary
- `kalam-cli-<version>-<platform>.tar.gz` or `.zip` - CLI archive
- `kalamdb-server-<version>-<platform>.tar.gz` or `.zip` - Server archive
- `SHA256SUMS` - Checksums file

**Platform Detection:**

The script automatically detects your platform:
- Linux: `linux-x86_64`, `linux-aarch64`
- macOS: `darwin-x86_64`, `darwin-aarch64`
- Windows: `windows-x86_64`

**Notes:**

- The script will prompt before deleting an existing release with the same version tag
- Use `--draft` to review the release before publishing
- The `dist/` directory can be cleaned up after the release is created
- This only builds for your current OS - use `release-multiplatform.sh` for cross-platform builds

## Docker-based Cross-Compilation

The `release-multiplatform.sh` script uses Docker to build binaries for all platforms from a single machine.

### How it Works

1. **Docker Image** (`Dockerfile.builder`): Contains all cross-compilation toolchains
   - MinGW for Windows binaries
   - aarch64-linux-gnu for ARM Linux
   - OSXCross for macOS binaries (both Intel and Apple Silicon)

2. **Cargo Configuration** (`cargo-config.toml`): Tells Rust which linker to use for each target

3. **Build Process**: Mounts your code into the Docker container and builds for each platform

### Benefits

- ✅ Build for all platforms from Linux/macOS/Windows
- ✅ Consistent build environment (reproducible builds)
- ✅ No need to install cross-compilation tools on your host
- ✅ Same binaries every time

### Technical Details

The Docker image includes:
- **Base**: Rust 1.75 on Debian Bookworm
- **Windows**: MinGW-w64 GCC
- **Linux ARM**: GCC aarch64-linux-gnu
- **macOS**: OSXCross with macOS 12.3 SDK

Total image size: ~4-5 GB (built once, cached afterward)

## GitHub CLI Setup

If you haven't set up the GitHub CLI yet:

1. **Install GitHub CLI:**
   - **macOS:** `brew install gh`
   - **Linux:** See [installation instructions](https://github.com/cli/cli/blob/trunk/docs/install_linux.md)
   - **Windows:** `winget install --id GitHub.cli` or download from [releases](https://github.com/cli/cli/releases)

2. **Authenticate:**
   ```bash
   gh auth login
   ```
   
   Follow the prompts to authenticate with your GitHub account.

3. **Verify:**
   ```bash
   gh auth status
   ```

## Quick Start

### Multi-Platform Release (Recommended)

```bash
# Build everything with Docker
./tools/release-multiplatform.sh v0.1.0

# Or build specific platforms only
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64
```

### Single Platform Release

```bash
# Build only for your current OS
./tools/release.sh v0.1.0
```

## Troubleshooting

### "gh: command not found"

Install the GitHub CLI as described in the setup section above.

### "Not logged in to GitHub"

Run `gh auth login` and follow the authentication flow.

### "Binary not found"

Ensure the build succeeded. Check for errors in the cargo build output.

### Permission denied on Unix-like systems

Make the script executable:
```bash
chmod +x tools/release-multiplatform.sh
chmod +x tools/release.sh
```

### "Docker: command not found"

Install Docker Desktop from: https://docs.docker.com/get-docker/

### Docker build fails on macOS download

The OSXCross SDK download may fail occasionally. If this happens:
1. Manually download the SDK from: https://github.com/joseluisq/macosx-sdks/releases
2. The Dockerfile will retry the download automatically

### Docker build is very slow

The first build takes 5-10 minutes to set up all toolchains. Subsequent builds are much faster as Docker caches the image layers.

### "Out of disk space" when building Docker image

The Docker image requires ~4-5 GB. Free up disk space or clean old Docker images:
```bash
docker system prune -a
```

### Cross-compilation fails for macOS

macOS cross-compilation is complex. If it fails, you can:
1. Build only other platforms: `--platforms=linux-x86_64,windows-x86_64`
2. Use GitHub Actions to build on native macOS runners (recommended for production)

## Contributing

When adding new tools or scripts to this directory:

1. Use bash for maximum compatibility
2. Add error handling (`set -e`)
3. Include usage documentation
4. Update this README
5. Test on multiple platforms if possible
