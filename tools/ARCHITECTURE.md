# Multi-Platform Release System

## Overview

The KalamDB release system uses Docker-based cross-compilation to build binaries for all supported platforms from a single machine.

## Architecture

```
tools/
├── Dockerfile.builder          # Docker image with all cross-compilation toolchains
├── cargo-config.toml           # Cargo linker configuration for each target
├── release-multiplatform.sh    # Main release script (all platforms)
├── release.sh                  # Simple release script (host platform only)
├── build-docker-image.sh       # Helper to pre-build Docker image
└── README.md                   # Documentation
```

## Workflow

### 1. One-time Setup

```bash
# Install Docker
# Install GitHub CLI and authenticate
gh auth login

# Pre-build the Docker image (optional, but recommended)
./tools/build-docker-image.sh
```

### 2. Create a Release

```bash
# Build for all platforms and create GitHub release
./tools/release-multiplatform.sh v0.1.0

# Or specify platforms
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64
```

### 3. What Happens

1. Docker image is built (if not cached)
2. For each platform:
   - CLI is compiled in Docker container
   - Server is compiled in Docker container
   - Binaries are extracted to `dist/`
3. Archives are created (`.tar.gz` or `.zip`)
4. SHA256 checksums are generated
5. GitHub release is created with all artifacts

## Supported Platforms

| Platform | Rust Target | Binary Extension | Archive Format |
|----------|-------------|------------------|----------------|
| Linux x64 | `x86_64-unknown-linux-gnu` | none | `.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | none | `.tar.gz` |
| macOS Intel | `x86_64-apple-darwin` | none | `.tar.gz` |
| macOS Apple Silicon | `aarch64-apple-darwin` | none | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-gnu` | `.exe` | `.zip` |

## Docker Image Details

### Base Image
- Debian Bookworm with Rust 1.75

### Installed Toolchains

1. **Windows (MinGW)**
   - `gcc-mingw-w64-x86-64`
   - Produces native Windows executables

2. **Linux ARM (aarch64)**
   - `gcc-aarch64-linux-gnu`
   - `g++-aarch64-linux-gnu`
   - For ARM servers (Raspberry Pi, AWS Graviton, etc.)

3. **macOS (OSXCross)**
   - Uses macOS 12.3 SDK
   - Supports both Intel and Apple Silicon
   - Most complex cross-compilation target

### Size
- Image: ~4-5 GB
- Build time: 5-10 minutes (first time only)

## GitHub Release Structure

Each release includes:

```
kalamdb-v0.1.0/
├── kalam-cli-v0.1.0-linux-x86_64.tar.gz
├── kalam-cli-v0.1.0-linux-aarch64.tar.gz
├── kalam-cli-v0.1.0-darwin-x86_64.tar.gz
├── kalam-cli-v0.1.0-darwin-aarch64.tar.gz
├── kalam-cli-v0.1.0-windows-x86_64.zip
├── kalamdb-server-v0.1.0-linux-x86_64.tar.gz
├── kalamdb-server-v0.1.0-linux-aarch64.tar.gz
├── kalamdb-server-v0.1.0-darwin-x86_64.tar.gz
├── kalamdb-server-v0.1.0-darwin-aarch64.tar.gz
├── kalamdb-server-v0.1.0-windows-x86_64.zip
└── SHA256SUMS
```

## Advanced Usage

### Build Specific Platforms

```bash
# Only Linux
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,linux-aarch64

# Only macOS
./tools/release-multiplatform.sh v0.1.0 --platforms=darwin-x86_64,darwin-aarch64

# Only Windows
./tools/release-multiplatform.sh v0.1.0 --platforms=windows-x86_64
```

### Draft and Pre-releases

```bash
# Create draft (not published)
./tools/release-multiplatform.sh v0.1.0 --draft

# Mark as pre-release
./tools/release-multiplatform.sh v0.2.0-beta --prerelease

# Both
./tools/release-multiplatform.sh v0.2.0-rc1 --draft --prerelease
```

### Local Testing

To test cross-compilation without creating a release:

```bash
# Build the Docker image
./tools/build-docker-image.sh

# Manually compile for a specific target
docker run --rm \
  -v "$(pwd):/workspace" \
  -w /workspace/backend \
  kalamdb-builder \
  cargo build --release --target x86_64-pc-windows-gnu
```

## CI/CD Integration

For production releases, consider using GitHub Actions with native runners for each platform. This provides:
- Better macOS support (native compilation)
- Faster builds (parallel execution)
- No local Docker requirement

The multi-platform script is ideal for:
- Quick local testing
- Internal releases
- When you need all platforms quickly

## Troubleshooting

### Docker Issues

**Problem**: Docker daemon not running
**Solution**: Start Docker Desktop

**Problem**: Permission denied (Linux)
**Solution**: Add user to docker group: `sudo usermod -aG docker $USER`

### Build Failures

**Problem**: macOS build fails
**Solution**: macOS cross-compilation is fragile. Use `--platforms` to skip it, or use GitHub Actions

**Problem**: Out of memory
**Solution**: Increase Docker memory limit in Docker Desktop settings (>= 4GB recommended)

### Release Issues

**Problem**: Release already exists
**Solution**: The script will prompt to delete and recreate

**Problem**: GitHub authentication failed
**Solution**: Run `gh auth login` and re-authenticate

## Maintenance

### Updating Rust Version

Edit `Dockerfile.builder`:
```dockerfile
FROM rust:1.75-bookworm  # Change version here
```

Rebuild: `./tools/build-docker-image.sh`

### Adding New Platforms

1. Add Rust target: `rustup target add <target-triple>`
2. Add to `ALL_PLATFORMS` array in `release-multiplatform.sh`
3. Add linker configuration to `cargo-config.toml`
4. Install cross-compiler in `Dockerfile.builder`

## Security

### Checksums

Always verify downloads:
```bash
wget https://github.com/jamals86/KalamDB/releases/download/v0.1.0/SHA256SUMS
sha256sum -c SHA256SUMS
```

### Reproducible Builds

The Docker-based approach ensures reproducible builds:
- Same Docker image → same binaries
- Same source code → same output
- Can verify builds independently

## Performance

| Platform | Build Time (approx) |
|----------|---------------------|
| Linux x64 | 2-3 min |
| Linux ARM64 | 2-3 min |
| Windows x64 | 3-4 min |
| macOS Intel | 3-4 min |
| macOS ARM64 | 3-4 min |
| **Total** | **15-20 min** |

(After Docker image is cached)

## Future Improvements

- [ ] GitHub Actions workflow for automated releases
- [ ] Code signing for macOS/Windows binaries
- [ ] ARM32 support (Raspberry Pi 32-bit)
- [ ] FreeBSD/NetBSD support
- [ ] Static linking for better portability
- [ ] Stripped binaries for smaller size
- [ ] Compressed binaries (UPX)
