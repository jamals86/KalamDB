# Quick Start - KalamDB Development

Get KalamDB up and running in under 10 minutes!

## Prerequisites Checklist

Before you begin, ensure you have:

- [ ] **Git** installed
- [ ] **Rust 1.75+** installed
- [ ] **LLVM/Clang** installed
- [ ] **C++ Compiler** installed

**Don't have these?** See the [Full Setup Guide](DEVELOPMENT_SETUP.md) for detailed installation instructions.

**Platform-Specific Guides:**
- 📖 [macOS Setup Guide](../backend/docs/macos.md) - Detailed instructions for macOS (Intel & Apple Silicon)
- 📖 [Linux Setup Guide](../backend/docs/linux.md) - Covers Ubuntu, Fedora, Arch, and more
- 📖 [Windows Setup Guide](../backend/docs/windows.md) - Complete Windows setup with troubleshooting

---

## 🚀 Platform-Specific Quick Start

### Windows

```powershell
# 1. Install Visual Studio Build Tools (if not already installed)
# Download from: https://visualstudio.microsoft.com/downloads/
# Select: "Desktop development with C++" workload

# 2. Install LLVM (if not already installed)
# Download from: https://github.com/llvm/llvm-project/releases
# During install: Check "Add LLVM to system PATH"

# 3. Install Rust (if not already installed)
# Download from: https://rustup.rs/
# Run rustup-init.exe and follow prompts

# 4. Verify prerequisites
clang --version
cargo --version

# 5. Clone and build
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB\backend
cargo build

# 6. Run
cargo run
```

### macOS

```bash
# 1. Install Xcode Command Line Tools
xcode-select --install

# 2. Install Homebrew (if not already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# 3. Install LLVM and Rust
brew install llvm rust

# 4. Set up library paths for RocksDB compilation
# Add LLVM to PATH and configure dynamic linker
echo 'export PATH="/opt/homebrew/opt/llvm/bin:$PATH"' >> ~/.zshrc
echo 'export DYLD_FALLBACK_LIBRARY_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib:$DYLD_FALLBACK_LIBRARY_PATH"' >> ~/.zshrc
source ~/.zshrc

# 5. Verify prerequisites
clang --version
cargo --version

# 6. Clone and build
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend
cargo build

# 7. Run
cargo run
```

### Linux (Ubuntu/Debian)

```bash
# 1. Install all prerequisites
sudo apt update
sudo apt install -y build-essential llvm clang libclang-dev cmake pkg-config libssl-dev git curl

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Verify prerequisites
clang --version
cargo --version

# 4. Clone and build
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend
cargo build

# 5. Run
cargo run
```

### Linux (Fedora/RHEL)

```bash
# 1. Install all prerequisites
sudo dnf groupinstall -y "Development Tools"
sudo dnf install -y llvm clang clang-devel cmake pkgconfig openssl-devel git

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Verify prerequisites
clang --version
cargo --version

# 4. Clone and build
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend
cargo build

# 5. Run
cargo run
```

---

## ✅ Verify Installation

After the server starts, you should see:

```
INFO  kalamdb_server > KalamDB Server starting...
INFO  kalamdb_server > Storage path: ./data
INFO  kalamdb_server > Server listening on http://127.0.0.1:8080
```

Test the server:

```bash
# Health check
curl http://localhost:8080/health

# Expected response:
# {"status":"ok"}
```

---

## 🎯 Common Tasks

### Run Tests

```bash
cd backend
cargo test
```

### Configure Server

```bash
# Copy example configuration
cp config.example.toml config.toml

# Edit with your settings
# Default: http://127.0.0.1:8080
```

### Build Release Version

```bash
# Optimized build (slower to compile, faster to run)
cargo build --release

# Run release build
./target/release/kalamdb-server
```

### Check Code Quality

```bash
# Run linter
cargo clippy

# Format code
cargo fmt
```

---

## 📚 Next Steps

### Learn KalamDB

1. **API Documentation**: See [Backend README](../backend/README.md#api-endpoints)
2. **Architecture**: Read [Feature Specification](../specs/001-build-a-rust/spec.md)
3. **Data Model**: Understand [Table-Per-User Architecture](../README.md#-what-makes-kalamdb-different)

### Development Workflow

1. **Code Style**: Follow [Constitution Principles](../.specify/memory/constitution.md)
2. **Testing**: Write tests for all new features
3. **Documentation**: Add rustdoc comments and inline explanations

### Try Examples

```bash
# Insert a message
curl -X POST http://localhost:8080/api/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "userId": "user123",
    "conversationId": "conv456",
    "content": "Hello, KalamDB!",
    "timestamp": "2025-10-14T10:30:00Z"
  }'

# Query messages
curl "http://localhost:8080/api/v1/messages?userId=user123&conversationId=conv456"
```

---

## 🆘 Troubleshooting

### Build Fails

**"linker not found"**: Install C++ compiler (see platform-specific guides below)

**"Unable to find libclang"**: Install LLVM/Clang (see platform-specific guides below)

**"RocksDB build failed"**: Install CMake and C++ build tools

**macOS: "Library not loaded: libclang.dylib"**: See the [macOS Setup Guide](../backend/docs/macos.md#troubleshooting) for the solution

### Compilation is Slow

**First build**: Takes 10-20 minutes (compiles RocksDB, Arrow, Parquet from source)

**Subsequent builds**: Much faster (incremental compilation)

**Speed up**: Use `cargo check` instead of `cargo build` for error checking

### Need More Help?

See the platform-specific detailed guides:
- **macOS:** [macOS Setup Guide](../backend/docs/macos.md)
- **Linux:** [Linux Setup Guide](../backend/docs/linux.md)  
- **Windows:** [Windows Setup Guide](../backend/docs/windows.md)

Or see the [Full Troubleshooting Guide](DEVELOPMENT_SETUP.md#troubleshooting) for comprehensive solutions.

---

## 📖 Full Documentation

For detailed setup instructions, advanced configuration, and comprehensive troubleshooting:

**[📘 Complete Development Setup Guide →](DEVELOPMENT_SETUP.md)**

---

**Platform Tested On**:
- ✅ Windows 11 (MSVC toolchain)
- ✅ macOS 13+ (Apple Silicon & Intel)
- ✅ Ubuntu 22.04 LTS
- ✅ Fedora 38+
- ✅ Arch Linux

**Last Updated**: October 14, 2025  
**KalamDB Version**: 0.1.0
