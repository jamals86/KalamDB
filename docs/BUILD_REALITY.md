# Build Speed Reality Check

## ⚠️ Understanding Build Times

### The Truth About `cargo clean`
**DON'T run `cargo clean` unless absolutely necessary!**

When you run `cargo clean`:
1. Deletes all compiled artifacts (target/ directory)
2. Clears sccache's ability to help (all compilations are "new")
3. Forces a complete rebuild of:
   - All dependencies (~800+ crates)
   - All workspace crates (9 crates)
   - All procedural macros
   - All build scripts

**Result**: Next build takes 4-6 minutes NO MATTER WHAT optimizations you have.

### When to Clean
Only run `cargo clean` when:
- Cargo.toml dependencies changed significantly
- Cargo.lock is corrupted
- Build artifacts are causing weird errors
- Switching between major Rust versions
- You need to free disk space

### Normal Development Workflow

```bash
# ❌ DON'T DO THIS REGULARLY
cargo clean && cargo build  # 6 minutes

# ✅ DO THIS INSTEAD
cargo build                  # 3-5 seconds (incremental)

# After git pull
cargo build                  # 30-60 seconds (only changed code)

# Check without building
cargo check                  # Even faster than build
```

## 📊 Real Build Time Examples

### Scenario 1: Fresh Checkout (First Time Ever)
```bash
git clone https://github.com/your/repo
cd repo
cargo build
# Time: 6-8 minutes (downloading + compiling everything)
```

### Scenario 2: Normal Development (Touch 1 File)
```bash
# Edit backend/src/main.rs
cargo build
# Time: 3-5 seconds ⚡
```

### Scenario 3: After Git Pull (Few Files Changed)
```bash
git pull origin main
# Changed: 3 files in kalamdb-core
cargo build
# Time: 30-60 seconds (recompile affected crates)
```

### Scenario 4: After `cargo clean` (Why?!)
```bash
cargo clean
cargo build
# Time: 6 minutes 😢
# sccache: 3% hit rate (most compilations are "new")
```

### Scenario 5: Second Build After Clean
```bash
# Build is already done, so:
cargo build
# Time: <1 second ⚡
# sccache: High hit rate (everything cached)
```

## 🎯 How to Actually Save Time

### 1. Use `cargo check` for Syntax Checking
```bash
# Instead of:
cargo build  # Compiles everything

# Use:
cargo check  # Only checks, doesn't produce binaries
# Time: 2-3 seconds vs 3-5 seconds
```

### 2. Build Only What You Need
```bash
# Instead of:
cargo build --workspace  # Builds all 9 crates

# Use:
cargo build -p kalamdb-core  # Build only core crate
# Time: Seconds instead of minutes
```

### 3. Use cargo-watch for Auto-Rebuild
```bash
# Install once
cargo install cargo-watch

# Auto-rebuild on file changes
cargo watch -x check
cargo watch -x "test test_name"
cargo watch -x "run"
```

### 4. Test Incrementally
```bash
# Instead of:
cargo test  # Runs ALL tests

# Use:
cargo nextest run -p kalamdb-core  # One crate
cargo nextest run test_specific    # One test
```

## 📈 sccache Hit Rate Explained

### After `cargo clean`:
```
Cache hits rate: 3.31%  😢
# Almost everything is a cache MISS
# First build is slow no matter what
```

### After Multiple Builds:
```
Cache hits rate: 85%+  🎉
# Most compilations reuse cache
# Incremental builds are fast
```

### sccache Works Best When:
- ✅ You build incrementally (don't clean)
- ✅ Dependencies don't change
- ✅ You work on same code repeatedly
- ✅ Multiple projects share dependencies

### sccache Doesn't Help With:
- ❌ First build after `cargo clean`
- ❌ Brand new dependencies
- ❌ Procedural macros (often non-cacheable)
- ❌ Build scripts that run every time

## 🔍 Checking Your Build Speed

### Before Doing Anything
```bash
# Check current cache
sccache --show-stats

# Look for:
# - Cache hits rate (should be high after initial build)
# - Cache size (grows over time)
# - Compile requests (total compilations)
```

### Measure Incremental Build
```bash
# Touch a file and rebuild
touch backend/src/main.rs
time cargo build

# Expected: 3-5 seconds
# If slower: Check sccache stats
```

### Compare with Check
```bash
touch backend/src/main.rs
time cargo check

# Should be slightly faster than build
```

## 💡 Pro Tips

### Tip 1: Use Workspace Commands
```bash
# Check all crates quickly
cargo check --workspace

# Build only changed crates
cargo build  # Smart, only builds what changed
```

### Tip 2: Profile Your Builds
```bash
# See what takes longest
cargo build --timings

# Opens HTML report showing:
# - Which crates took longest
# - Dependency graph
# - Parallel compilation usage
```

### Tip 3: Clean Selectively
```bash
# Instead of full clean:
cargo clean -p kalamdb-core  # Clean one crate

# Or clean target profiles:
rm -rf target/debug          # Keep release builds
rm -rf target/release        # Keep debug builds
```

### Tip 4: Keep Dependencies Stable
```bash
# Lock dependencies (recommended)
cargo update  # Only when needed

# Don't change Cargo.toml unnecessarily
# Every dependency change = longer build
```

## 📊 Benchmarking Your Setup

### Test 1: Incremental Speed
```bash
touch backend/src/main.rs
time cargo build
# Target: <5 seconds
```

### Test 2: No-Op Speed
```bash
time cargo build
# Target: <1 second (nothing to do)
```

### Test 3: Check Speed
```bash
touch backend/src/main.rs
time cargo check
# Target: <3 seconds
```

### Test 4: sccache Hit Rate
```bash
# Reset stats
sccache --zero-stats

# Build twice
cargo build
cargo build

# Check hit rate
sccache --show-stats
# Target: 95%+ on second build
```

## 🎯 Summary

| Action | Time | When to Use |
|--------|------|-------------|
| `cargo build` (incremental) | 3-5s | Normal development |
| `cargo check` | 2-3s | Syntax checking |
| `cargo build` (after clean) | 6min | After cleaning |
| `cargo build` (first time) | 6-8min | Fresh clone |
| `cargo test` (one test) | 5-10s | Test-driven dev |
| `cargo clippy` | 5-10s | Linting |

**Golden Rule**: Don't use `cargo clean` unless you have a specific reason!

---

**Last Updated**: December 20, 2025  
**Reality Check**: Clean builds are ALWAYS slow, incremental builds are fast
