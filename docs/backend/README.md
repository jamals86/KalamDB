# Platform-Specific Setup Guides

This directory contains detailed, platform-specific setup guides for KalamDB development.

## Available Guides

### 🍎 [macOS Setup Guide](macos.md)
Complete setup instructions for macOS (both Apple Silicon and Intel).

**Highlights:**
- Xcode Command Line Tools installation
- Homebrew and LLVM setup
- **Critical:** `libclang.dylib` loading issue fix
- Architecture-specific notes (M1/M2/M3 vs Intel)
- Comprehensive troubleshooting

**When to use:** If you're developing on macOS and encountering build issues, especially `libclang.dylib` errors.

---

### 🐧 [Linux Setup Guide](linux.md)
Covers multiple Linux distributions with specific package installation commands.

**Supported Distributions:**
- Ubuntu / Debian / Pop!_OS / Linux Mint
- Fedora / RHEL / CentOS Stream / Rocky Linux
- Arch Linux / Manjaro / EndeavourOS
- openSUSE Leap / Tumbleweed

**Highlights:**
- Distribution-specific package managers
- LLVM and build tools setup
- Performance optimization tips
- WSL2 compatibility notes

---

### 🪟 [Windows Setup Guide](windows.md)
Complete Windows setup with Visual Studio Build Tools and LLVM.

**Highlights:**
- Visual Studio Build Tools vs full IDE
- LLVM installation and PATH configuration
- Environment variable setup
- PowerShell commands and troubleshooting
- Windows Terminal recommendations
- WSL2 alternative approach

---

## Quick Navigation

**Problem:** Build failing with `libclang` errors?
- **macOS:** See [macOS Guide - Troubleshooting](macos.md#issue-library-not-loaded-rpathlibc langdylib)
- **Linux:** See [Linux Guide - Troubleshooting](linux.md#issue-unable-to-find-libclang)
- **Windows:** See [Windows Guide - Troubleshooting](windows.md#issue-unable-to-find-libclang--could-not-find-libclangdll)

**Problem:** Slow compilation?
- All platforms: Check respective guide's troubleshooting section

**Problem:** Out of memory during build?
- All platforms: See section on limiting parallel jobs

---

## General Documentation

For general setup information and quick start:
- [Quick Start Guide](../../docs/QUICK_START.md) - Get started in under 10 minutes
- [Development Setup Guide](../../docs/DEVELOPMENT_SETUP.md) - Overview of all platforms

---

## Contributing

When updating these guides:
1. Test commands on actual target platform
2. Include error messages users might see
3. Provide multiple solutions where applicable
4. Update "Last Updated" date at bottom of guide
5. Add version information for tested configurations

---

**Last Updated:** October 14, 2025
