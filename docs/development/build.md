# Build Guide

This guide covers build configuration, optimization, and troubleshooting for KalamDB development.

## Quick Reference

### Common Commands

```bash
# Development build
cargo build

# Release build
cargo build --release

# Check without building (faster)
cargo check

# Run tests
cargo nextest run
# or
cargo test

# View sccache stats
sccache --show-stats
```

### Expected Build Times

| Scenario | Time |
|----------|------|
| Clean build (first time) | 4-6 minutes |
| Incremental (1 file changed) | 3-5 seconds |
| No changes | < 1 second |
| After `git pull` | 30-60 seconds |

> ⚠️ **Don't run `cargo clean` unless necessary** - it wipes the cache and forces a full rebuild.

---

## Build Optimization

### 1. sccache (Recommended)

sccache caches compiled artifacts for faster rebuilds.

**Installation:**
```bash
# macOS
brew install sccache

# Other platforms
cargo install sccache
```

**Configuration** (add to `~/.zshrc` or `~/.bashrc`):
```bash
export RUSTC_WRAPPER=sccache
```

**Verification:**
```bash
sccache --show-stats
```

### 2. cargo-nextest (Faster Tests)

```bash
cargo install cargo-nextest
cargo nextest run
```

### 3. Cargo Configuration

The project includes optimized `.cargo/config.toml`:
- Sparse registry protocol for faster dependency resolution
- macOS-specific optimizations (`split-debuginfo = "unpacked"`)
- Incremental builds enabled

---

## Platform Setup

See platform-specific guides:
- [macOS](macos.md)
- [Linux](linux.md)  
- [Windows](windows.md)

### Common Requirement: LLVM/Clang

KalamDB depends on RocksDB and Arrow which require C++ compilation.

**macOS:**
```bash
brew install llvm
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt install llvm clang libclang-dev
```

**Windows:**
Download from https://github.com/llvm/llvm-project/releases

---

## Troubleshooting

### "libclang.dylib not found" (macOS)

```bash
export DYLD_FALLBACK_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:$DYLD_FALLBACK_LIBRARY_PATH"
```

For more detail (including alternative fixes), see the [macOS libclang troubleshooting section](macos.md#issue-library-not-loaded-rpathlibclangdylib).

### Slow Clean Builds

This is expected! After `cargo clean`, sccache has no cache to use. The second build will be fast.

### CI/CD Build Issues

The project uses sccache in GitHub Actions for all platforms. See `.github/workflows/` for configuration.

---

## Further Reading

For runtime performance tips, see:
- [performance-optimizations.md](performance-optimizations.md)
