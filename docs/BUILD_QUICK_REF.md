# Build Speed Quick Reference

Quick commands for checking and using build optimizations.

## 🚀 Daily Commands

### Check Build Speed
```bash
# Quick incremental build
time cargo build

# Full workspace build
time cargo build --workspace

# Release build
time cargo build --release
```

### Check Cache Stats
```bash
# View sccache statistics
sccache --show-stats

# Expected output:
# Compile requests: XXX
# Cache hits (Rust): XX% (should increase over time)
# Cache location: ~/.cache/sccache
```

### Fast Testing
```bash
# Use nextest (faster than cargo test)
cargo nextest run

# Run specific package
cargo nextest run -p kalamdb-core

# Run with output
cargo nextest run --nocapture
```

## 🔧 Maintenance

### Clear Cache
```bash
# Stop sccache server
sccache --stop-server

# Clear cache directory
rm -rf ~/.cache/sccache

# Restart sccache
sccache --start-server
```

### Update Dependencies
```bash
# Update with cache benefits
cargo update
cargo build  # sccache reuses unchanged crates
```

## 📊 Benchmarking

### Measure Build Times
```bash
# Clean build
time (cargo clean && cargo build)
# Target: ~4 minutes

# Incremental (touch file)
touch backend/src/main.rs
time cargo build
# Target: <1 minute
```

### Cache Hit Rate
```bash
# Reset stats
sccache --zero-stats

# Build
cargo build

# Check hit rate
sccache --show-stats
# Good: >50% cache hit rate on second build
```

## 🎯 Targets

| Scenario | Target Time | Notes |
|----------|-------------|-------|
| Clean build | 4 minutes | First time or after `cargo clean` |
| Incremental | <1 minute | Single file change |
| After `git pull` | 1-2 minutes | Depends on changes |
| Tests (nextest) | Variable | Faster than `cargo test` |

## ⚠️ Troubleshooting

### Slow Builds?
```bash
# 1. Check wrapper is set
echo $RUSTC_WRAPPER  # Should be: sccache

# 2. Check sccache is running
sccache --show-stats

# 3. Restart if needed
sccache --stop-server && sccache --start-server
```

### Tests Failing?
```bash
# Storage tests need serialization
cargo test --test test_storage_management -- --test-threads=1

# Or use nextest (handles serial automatically)
cargo nextest run --test test_storage_management
```

## 📁 Documentation

- **Complete Guide**: [BUILD_OPTIMIZATION.md](BUILD_OPTIMIZATION.md)
- **Summary**: [BUILD_IMPROVEMENTS.md](BUILD_IMPROVEMENTS.md)
- **This File**: Quick reference for daily use

---

**Tip**: Add `alias cb='time cargo build'` to your shell for quick timing!
