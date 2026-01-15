# KalamDB Tools

This directory contains the build tool for KalamDB.

## Build Tool

The `build` script is an interactive tool for building releases, binaries, and Docker images.

### Usage

```bash
./tools/build              # Interactive mode
./tools/build --help       # Show help
```

### Features

- **Auto-detects version** from `Cargo.toml`
- **Multi-platform builds**: Linux, macOS, Windows
- **GitHub release creation** with checksums
- **Docker Hub publishing**

### Interactive Workflow

When you run `./tools/build`, it will:

1. **Detect version** from `Cargo.toml` automatically
2. **Ask for version tag** (e.g., `v0.1.1`)
3. **Platform selection** - multi-select which platforms to build:
   - `linux-x86_64` - Linux Intel/AMD 64-bit
   - `linux-aarch64` - Linux ARM 64-bit
   - `macos-x86_64` - macOS Intel
   - `macos-aarch64` - macOS Apple Silicon
   - `windows-x86_64` - Windows 64-bit
4. **GitHub release** - optionally create a release with all artifacts
5. **Docker build** - optionally build and push Docker images

### Prerequisites

| Feature | Requirement |
|---------|-------------|
| Binary builds | Rust toolchain |
| Cross-compilation | `cargo install cross` |
| GitHub releases | `brew install gh && gh auth login` |
| Docker builds | Docker Desktop |

### Example Session

```
╔════════════════════════════════════════════════════════════╗
║              KalamDB Build Tool                            ║
╚════════════════════════════════════════════════════════════╝

ℹ Detected version from Cargo.toml: 0.1.1

? Version tag for release [v0.1.1]: 

? Select target platforms:
  (Use arrow keys to move, space to toggle, enter to confirm)

  ▸ ◉ linux-x86_64
    ◉ linux-aarch64
    ◉ macos-x86_64
    ◉ macos-aarch64
    ◉ windows-x86_64

? Create GitHub release? [y/N]: y
? Build Docker image? [y/N]: n

════════════════════════════════════════════════════════════
Build Summary
════════════════════════════════════════════════════════════
  Version:          0.1.1
  Tag:              v0.1.1
  Platforms:        2 selected
                    • macos-aarch64
                    • linux-x86_64
  GitHub Release:   true
  Docker Build:     false
  Profile:          release-dist
════════════════════════════════════════════════════════════

? Proceed with build? [Y/n]: 
```

### Build Profile

The build uses a custom `release-dist` profile optimized for distribution:

```toml
[profile.release-dist]
inherits = "release"
opt-level = "z"       # Optimize for size
lto = true            # Link Time Optimization
codegen-units = 1     # Better optimization
panic = "abort"       # Smaller binary
strip = true          # Strip debug symbols
```

### Output

Binaries are output to `dist/<version>/`:

```
dist/0.1.1/
├── kalam-0.1.1-macos-aarch64
├── kalam-0.1.1-macos-aarch64.tar.gz
├── kalamdb-server-0.1.1-macos-aarch64
├── kalamdb-server-0.1.1-macos-aarch64.tar.gz
└── SHA256SUMS
```

### Docker

For Docker-specific builds, see:
- `docker/build/Dockerfile` - Production Dockerfile
- `docker/run/single/docker-compose.yml` - Compose configuration
- `docker/README.md` - Docker deployment guide

## Other Files

| File | Purpose |
|------|---------|
| `Dockerfile.builder` | Cross-compilation Docker image (for CI) |
| `cargo-config.toml` | Cargo configuration for cross-compilation |
| `ARCHITECTURE.md` | Build system architecture documentation |

## Troubleshooting

### "gh: command not found"

Install the GitHub CLI:
```bash
brew install gh
gh auth login
```

### "Not logged in to GitHub"

Run `gh auth login` and follow the authentication flow.

### Permission denied

Make the script executable:
```bash
chmod +x tools/build
```

### "Library not loaded: libclang.dylib" on macOS

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Or install LLVM via Homebrew
brew install llvm
```

### Cross-compilation fails

Install the `cross` tool:
```bash
cargo install cross
```
