# Build Scripts Comparison

This document compares all KalamDB build and release scripts.

## Quick Reference Table

| Script | Platforms | GitHub Upload | Version Format | Output Dir | Use Case |
|--------|-----------|---------------|----------------|------------|----------|
| `build-platform.sh` | 1 (specified) | ❌ | `0.1.0` | `dist/<version>/` | Test specific platform |
| `build-all-platforms.sh` | 5 (all) | ❌ | `0.1.0` | `dist/<version>/` | Local build all platforms |
| `release.sh` | 1 (current) | ✅ | `v0.1.0` | `dist/` | Quick release (current OS) |
| `release-multiplatform.sh` | 1-5 (Docker) | ✅ | `v0.1.0` | `dist/` | Official multi-platform release |

## Feature Comparison

### ✅ `build-platform.sh` ⭐ NEW!

**Command:**
```bash
./tools/build-platform.sh 0.1.0 linux-x86_64
```

**Features:**
- ✅ Single platform build
- ✅ Conventional naming: `kalam-<version>-<platform>`
- ✅ Versioned output directory: `dist/<version>/`
- ✅ Auto-installs `cross` for cross-compilation
- ✅ Per-platform checksums
- ✅ Fast - only builds what you need
- ❌ No GitHub upload
- ❌ Manual platform selection required

**Best for:**
- Testing builds for specific platforms
- Quick iteration during development
- Verifying cross-compilation works

---

### ✅ `build-all-platforms.sh` ⭐ NEW!

**Command:**
```bash
./tools/build-all-platforms.sh 0.1.0
```

**Features:**
- ✅ Builds all 5 platforms automatically
- ✅ Conventional naming for all binaries
- ✅ Versioned output directory: `dist/<version>/`
- ✅ Build summary (success/failure tracking)
- ✅ Combined + per-platform checksums
- ❌ No GitHub upload
- ❌ Requires local cross-compilation tools

**Best for:**
- Pre-release validation
- Building all platforms before manual upload
- CI/CD pipelines (combine with `gh release create`)

**Platforms Built:**
- `linux-x86_64` (Linux Intel/AMD)
- `linux-aarch64` (Linux ARM)
- `macos-x86_64` (macOS Intel)
- `macos-aarch64` (macOS Apple Silicon)
- `windows-x86_64` (Windows)

---

### ✅ `release.sh`

**Command:**
```bash
./tools/release.sh v0.1.0
```

**Features:**
- ✅ Auto-detects current platform
- ✅ Uploads to GitHub Releases
- ✅ Generates release notes
- ✅ Draft/prerelease support
- ✅ Fast - single platform only
- ❌ Only builds current OS
- ❌ Requires GitHub CLI

**Best for:**
- Quick releases from development machine
- Testing the release workflow
- Platform-specific releases

---

### ✅ `release-multiplatform.sh`

**Command:**
```bash
./tools/release-multiplatform.sh v0.1.0
```

**Features:**
- ✅ Docker-based cross-compilation
- ✅ Builds all platforms (or select subset)
- ✅ Uploads to GitHub Releases
- ✅ Consistent build environment
- ✅ Draft/prerelease support
- ✅ Best for official releases
- ❌ Requires Docker (~5GB image)
- ❌ Slower initial setup

**Best for:**
- Official production releases
- Consistent build environment
- Building from non-native platforms

---

## Output Comparison

### New Scripts (build-platform.sh / build-all-platforms.sh)

```
dist/
└── 0.1.0/                                    # Versioned directory
    ├── kalam-0.1.0-linux-x86_64
    ├── kalam-0.1.0-linux-x86_64.tar.gz
    ├── kalamdb-server-0.1.0-linux-x86_64
    ├── kalamdb-server-0.1.0-linux-x86_64.tar.gz
    ├── kalam-0.1.0-macos-aarch64
    ├── kalam-0.1.0-macos-aarch64.tar.gz
    ├── kalamdb-server-0.1.0-macos-aarch64
    ├── kalamdb-server-0.1.0-macos-aarch64.tar.gz
    ├── SHA256SUMS                            # Combined
    └── SHA256SUMS-linux-x86_64               # Per-platform
```

**Benefits:**
- ✅ Multiple versions can coexist
- ✅ Clear version organization
- ✅ Easy to archive specific versions
- ✅ Conventional naming

### Old Scripts (release.sh / release-multiplatform.sh)

```
dist/
├── kalam-v0.1.0-darwin-aarch64
├── kalam-v0.1.0-darwin-aarch64.tar.gz
├── kalamdb-server-v0.1.0-darwin-aarch64
├── kalamdb-server-v0.1.0-darwin-aarch64.tar.gz
└── SHA256SUMS
```

**Benefits:**
- ✅ Simple flat structure
- ✅ Works with existing workflows

---

## Workflow Examples

### Scenario 1: Development - Test Linux Build

```bash
# Build just Linux x86_64
./tools/build-platform.sh 0.1.0 linux-x86_64

# Test it
./dist/0.1.0/kalamdb-server-0.1.0-linux-x86_64 --version
```

---

### Scenario 2: Pre-Release - Validate All Platforms

```bash
# Build all platforms locally
./tools/build-all-platforms.sh 0.1.0

# Check output
ls -lh dist/0.1.0/

# Verify checksums
cd dist/0.1.0 && sha256sum -c SHA256SUMS

# Test each binary
./kalam-0.1.0-linux-x86_64 --version
./kalam-0.1.0-macos-aarch64 --version
# etc.
```

---

### Scenario 3: Quick Release - Current OS Only

```bash
# Build and upload (macOS only in this example)
./tools/release.sh v0.1.0

# View release
gh release view v0.1.0
```

---

### Scenario 4: Official Release - All Platforms

```bash
# Build all platforms with Docker and upload
./tools/release-multiplatform.sh v0.1.0

# Or specific platforms only
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64
```

---

### Scenario 5: CI/CD - Build Then Upload

```bash
# Build all platforms (no upload)
./tools/build-all-platforms.sh ${VERSION}

# Run tests on binaries
./dist/${VERSION}/kalam-${VERSION}-linux-x86_64 --version

# Upload to GitHub
gh release create v${VERSION} \
  ./dist/${VERSION}/* \
  --title "KalamDB v${VERSION}" \
  --notes "Release notes here"
```

---

## Migration Guide

### From Old to New Scripts

**Old workflow:**
```bash
./tools/release.sh v0.1.0
```

**New equivalent (build only):**
```bash
./tools/build-platform.sh 0.1.0 macos-aarch64
```

**New equivalent (build all):**
```bash
./tools/build-all-platforms.sh 0.1.0
```

**Upload separately:**
```bash
gh release create v0.1.0 ./dist/0.1.0/* --title "Release v0.1.0"
```

---

## Platform Identifiers

| Platform | Identifier | Rust Target | Notes |
|----------|-----------|-------------|-------|
| Linux Intel/AMD | `linux-x86_64` | `x86_64-unknown-linux-gnu` | Native or cross |
| Linux ARM | `linux-aarch64` | `aarch64-unknown-linux-gnu` | Cross-compile |
| macOS Intel | `macos-x86_64` | `x86_64-apple-darwin` | Native build |
| macOS Apple Silicon | `macos-aarch64` | `aarch64-apple-darwin` | Native build |
| Windows | `windows-x86_64` | `x86_64-pc-windows-gnu` | Cross-compile |

---

## Dependencies

### Build-Only Scripts
- ✅ Rust toolchain
- ✅ `cross` (auto-installed)
- ✅ `tar`, `gzip` or `zip`
- ✅ `sha256sum` or `shasum`

### Release Scripts (+ GitHub)
- ✅ All of the above
- ✅ GitHub CLI (`gh`)
- ✅ Docker (multiplatform only)

---

## Summary

**Use `build-platform.sh` when:**
- Testing a specific platform build
- Quick iteration during development
- Don't need GitHub upload

**Use `build-all-platforms.sh` when:**
- Pre-release validation
- Building locally before manual upload
- CI/CD with separate upload step

**Use `release.sh` when:**
- Quick release from current OS
- Testing release workflow
- Single-platform release

**Use `release-multiplatform.sh` when:**
- Official production releases
- Need all platforms
- Want consistent Docker builds
