# macOS libclang.dylib Fix - Documentation Update

## Problem Summary

When building KalamDB on macOS, the RocksDB compilation fails with:

```
dyld[xxxxx]: Library not loaded: @rpath/libclang.dylib
  Referenced from: .../build-script-build
  Reason: tried: ... (no such file)
```

## Root Cause

The `librocksdb-sys` crate uses `bindgen` to generate Rust bindings for C++ code. During the build process, bindgen needs to load `libclang.dylib` to parse C++ headers. On macOS, the dynamic linker (`dyld`) cannot find this library in the expected locations.

## Solution

Set the `DYLD_FALLBACK_LIBRARY_PATH` environment variable to point to the Xcode toolchain library directory:

```bash
export DYLD_FALLBACK_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:$DYLD_FALLBACK_LIBRARY_PATH"
```

### Make it Permanent

Add to `~/.zshrc` (or `~/.bash_profile`):

```bash
echo 'export DYLD_FALLBACK_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:$DYLD_FALLBACK_LIBRARY_PATH"' >> ~/.zshrc
source ~/.zshrc
```

## Documentation Updates

### 1. Created Platform-Specific Guides

**Location:** `backend/docs/`

- ✅ **[macos.md](../backend/docs/macos.md)** - Complete macOS setup guide
  - Includes detailed explanation of the libclang issue
  - Step-by-step fix with verification commands
  - Architecture-specific notes (Apple Silicon vs Intel)
  - Comprehensive troubleshooting section

- ✅ **[linux.md](../backend/docs/linux.md)** - Linux setup for multiple distributions
  - Ubuntu/Debian, Fedora/RHEL, Arch, openSUSE
  - Distribution-specific package commands
  - Performance optimization tips

- ✅ **[windows.md](../backend/docs/windows.md)** - Windows setup guide
  - Visual Studio Build Tools installation
  - LLVM/Clang configuration
  - PowerShell-specific instructions

- ✅ **[README.md](../backend/docs/README.md)** - Navigation guide for platform docs

### 2. Updated QUICK_START.md

**Location:** `docs/QUICK_START.md`

**Changes:**
- ✅ Added `DYLD_FALLBACK_LIBRARY_PATH` to macOS quick setup
- ✅ Added references to platform-specific guides at the top
- ✅ Updated troubleshooting section with links to detailed guides
- ✅ Added specific mention of libclang.dylib issue for macOS

### 3. Updated DEVELOPMENT_SETUP.md

**Location:** `docs/DEVELOPMENT_SETUP.md`

**Changes:**
- ✅ Added "Platform-Specific Setup Guides" section in TOC
- ✅ Added prominent links to new platform guides at the beginning
- ✅ Updated macOS Step 2 to include `DYLD_FALLBACK_LIBRARY_PATH` configuration
- ✅ Enhanced libclang troubleshooting with macOS-specific solution
- ✅ Updated "Getting Help" section to reference platform-specific guides first

## Files Created/Modified

### Created
1. `/Users/jamal/git/KalamDB/backend/docs/macos.md` (new)
2. `/Users/jamal/git/KalamDB/backend/docs/linux.md` (new)
3. `/Users/jamal/git/KalamDB/backend/docs/windows.md` (new)
4. `/Users/jamal/git/KalamDB/backend/docs/README.md` (new)

### Modified
1. `/Users/jamal/git/KalamDB/docs/QUICK_START.md` (updated)
2. `/Users/jamal/git/KalamDB/docs/DEVELOPMENT_SETUP.md` (updated)

## Verification

Build succeeds with the fix:

```bash
cd /Users/jamal/git/KalamDB/backend
DYLD_FALLBACK_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:$DYLD_FALLBACK_LIBRARY_PATH" cargo build
```

Output: ✅ Successful compilation in 1m 24s

## Key Features of New Documentation

### Comprehensive Coverage
- **Quick Setup**: Get running fast with copy-paste commands
- **Detailed Steps**: Understand what each command does
- **Troubleshooting**: Extensive problem/solution pairs
- **Verification**: Checklists to confirm setup

### Platform-Specific
- **macOS**: Apple Silicon vs Intel considerations
- **Linux**: Multiple distributions with specific commands
- **Windows**: PowerShell vs CMD, Visual Studio variants

### User-Friendly
- **Clear Headers**: Easy navigation
- **Code Blocks**: Ready to copy and paste
- **Explanations**: Why each step is needed
- **Links**: Cross-references between guides

## Future Improvements

Potential enhancements:
- [ ] Add video tutorial links
- [ ] Include Docker setup as alternative
- [ ] Add CI/CD setup examples
- [ ] Create troubleshooting flowchart

---

**Date:** October 14, 2025  
**Issue:** RocksDB libclang.dylib loading on macOS  
**Status:** ✅ Documented and Resolved
