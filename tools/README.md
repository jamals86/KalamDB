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

### `build-platform.sh` (Platform-Specific Builder)

**NEW!** Builds CLI and server binaries for a **specific platform** with conventional naming.

**Prerequisites:**
- Rust toolchain installed
- `cross` tool (auto-installed if needed for cross-compilation)
- `tar` and `gzip` (for Linux/macOS) or `zip` (for Windows)

**Usage:**

```bash
./tools/build-platform.sh <version> <platform>
```

**Supported Platforms:**

- `linux-x86_64` - Linux Intel/AMD 64-bit
- `linux-aarch64` - Linux ARM 64-bit
- `macos-x86_64` - macOS Intel
- `macos-aarch64` - macOS Apple Silicon
- `windows-x86_64` - Windows 64-bit

**Examples:**

```bash
# Build for Linux x86_64
./tools/build-platform.sh 0.1.0 linux-x86_64

# Build for macOS Apple Silicon
./tools/build-platform.sh 0.1.0 macos-aarch64

# Build for Windows
./tools/build-platform.sh 0.1.0 windows-x86_64
```

**What it does:**

1. 🔍 Validates version and platform arguments
2. 🎯 Adds Rust target for the specified platform
3. 🔨 Builds CLI and Server using `cargo` or `cross` (for cross-compilation)
4. 📁 Outputs to `dist/<version>/` with conventional naming:
   - `kalam-<version>-<platform>` / `kalam-<version>-<platform>.exe`
   - `kalamdb-server-<version>-<platform>` / `kalamdb-server-<version>-<platform>.exe`
5. 📦 Creates platform-specific archives (`.tar.gz` or `.zip`)
6. 🔐 Generates SHA256 checksums for the platform

**Output Structure:**

```
dist/0.1.0/
├── kalam-0.1.0-linux-x86_64
├── kalam-0.1.0-linux-x86_64.tar.gz
├── kalamdb-server-0.1.0-linux-x86_64
├── kalamdb-server-0.1.0-linux-x86_64.tar.gz
└── SHA256SUMS-linux-x86_64
```

**Cross-Compilation:**

The script automatically uses `cross` for cross-compilation when building:
- ARM targets from x86_64
- Windows targets from Linux/macOS

---

### `build-all-platforms.sh` (Multi-Platform Builder)

**NEW!** Builds CLI and server binaries for **all supported platforms** in one command.

**Prerequisites:**
- Same as `build-platform.sh`
- Sufficient disk space for multiple platform builds (~2GB)

**Usage:**

```bash
./tools/build-all-platforms.sh <version>
```

**Examples:**

```bash
# Build for all platforms
./tools/build-all-platforms.sh 0.1.0
```

**What it does:**

1. 🔄 Iterates through all 5 supported platforms
2. 🔨 Calls `build-platform.sh` for each platform
3. 📊 Tracks successful and failed builds
4. 📦 Generates combined `SHA256SUMS` file
5. 📈 Shows build summary and total size

**Output Structure:**

```
dist/0.1.0/
├── kalam-0.1.0-linux-x86_64
├── kalam-0.1.0-linux-x86_64.tar.gz
├── kalam-0.1.0-linux-aarch64
├── kalam-0.1.0-linux-aarch64.tar.gz
├── kalam-0.1.0-macos-x86_64
├── kalam-0.1.0-macos-x86_64.tar.gz
├── kalam-0.1.0-macos-aarch64
├── kalam-0.1.0-macos-aarch64.tar.gz
├── kalam-0.1.0-windows-x86_64.exe
├── kalam-0.1.0-windows-x86_64.zip
├── kalamdb-server-0.1.0-linux-x86_64
├── kalamdb-server-0.1.0-linux-x86_64.tar.gz
├── kalamdb-server-0.1.0-linux-aarch64
├── kalamdb-server-0.1.0-linux-aarch64.tar.gz
├── kalamdb-server-0.1.0-macos-x86_64
├── kalamdb-server-0.1.0-macos-x86_64.tar.gz
├── kalamdb-server-0.1.0-macos-aarch64
├── kalamdb-server-0.1.0-macos-aarch64.tar.gz
├── kalamdb-server-0.1.0-windows-x86_64.exe
├── kalamdb-server-0.1.0-windows-x86_64.zip
├── SHA256SUMS (combined)
└── SHA256SUMS-<platform> (per-platform)
```

---

### `release.sh` (Single Platform + GitHub Release)

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
- This only builds for your current OS - use `build-all-platforms.sh` for multi-platform builds

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

### Build for Specific Platform

```bash
# Build for Linux x86_64
./tools/build-platform.sh 0.1.0 linux-x86_64

# Build for macOS Apple Silicon
./tools/build-platform.sh 0.1.0 macos-aarch64

# Build for Windows
./tools/build-platform.sh 0.1.0 windows-x86_64
```

### Build for All Platforms

```bash
# Build everything (5 platforms)
./tools/build-all-platforms.sh 0.1.0
```

### Multi-Platform Release (with GitHub Upload)

```bash
# Build everything with Docker and upload to GitHub
./tools/release-multiplatform.sh v0.1.0

# Or build specific platforms only
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64
```

### Single Platform Release (Current OS only)

```bash
# Build only for your current OS and upload to GitHub
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

### "Library not loaded: libclang.dylib" on macOS

The RocksDB dependency requires libclang for building. The script automatically detects it from:
1. Xcode Command Line Tools (preferred)
2. Homebrew LLVM

If you see this error:
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Or install LLVM via Homebrew
brew install llvm
```

The script will automatically set `LIBCLANG_PATH` and `DYLD_LIBRARY_PATH` for macOS builds.

## Contributing

When adding new tools or scripts to this directory:

1. Use bash for maximum compatibility
2. Add error handling (`set -e`)
3. Include usage documentation
4. Update this README
5. Test on multiple platforms if possible
