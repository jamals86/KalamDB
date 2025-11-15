# KalamDB Documentation

Welcome to KalamDB documentation! This folder contains guides for getting started with KalamDB development.

## 📖 Table of Contents

### Getting Started

- **[🚀 Quick Start Guide](QUICK_START.md)** – Build the server and run your first query
- **[📘 Development Setup Guide](DEVELOPMENT_SETUP.md)** – Full environment setup and troubleshooting

### Using KalamDB

- **[SQL Reference](SQL.md)** – SQL syntax, datatypes, and examples
- **[API Guide](API.md)** – HTTP and WebSocket endpoints
- **[CLI Guide](cli.md)** – Using the `kalam` command-line client

### Project Docs

- **[Backend README](../backend/README.md)** – Backend project structure and workflow
- **[Main README](../README.md)** – High-level overview and architecture

---

## 🎯 Quick Links by Role

### New Contributors

1. Read the [Main README](../README.md)
2. Follow the [Quick Start Guide](QUICK_START.md)
3. Skim the [SQL Reference](SQL.md) and [API Guide](API.md)

---

## 💡 Common Questions

### How do I get started with development?

Follow the [Quick Start Guide](QUICK_START.md) for your platform. If you encounter issues, see the [Troubleshooting section](DEVELOPMENT_SETUP.md#troubleshooting) in the full setup guide.

### Why do I need LLVM/Clang?

KalamDB depends on native libraries (RocksDB, Arrow, Parquet) written in C++. The Rust build process needs LLVM/Clang to compile these dependencies. See the [System Requirements](DEVELOPMENT_SETUP.md#system-requirements) section for details.

### What's the table-per-user architecture?

KalamDB stores each user's messages in isolated storage partitions instead of a shared table. This enables massive scalability for real-time subscriptions. Read more in the [Main README](../README.md#-what-makes-kalamdb-different).

### How do I write SQL queries against KalamDB?

Check out the [SQL Query Examples](../specs/001-build-a-rust/sql-query-examples.md) for common patterns like querying messages, filtering by conversation, and time-range queries.

### Where are the code documentation standards?

See [Principle VIII: Self-Documenting Code](../.specify/memory/constitution.md#viii-self-documenting-code) in the project constitution for comprehensive documentation requirements.

---

## 🛠️ Platform-Specific Guides

### Windows Development

- [Windows Setup Instructions](DEVELOPMENT_SETUP.md#windows-setup)
- Requirements: Visual Studio Build Tools, LLVM, Rust (MSVC toolchain)
- Common issues: [Windows Troubleshooting](DEVELOPMENT_SETUP.md#platform-specific-issues)

### macOS Development

- [macOS Setup Instructions](DEVELOPMENT_SETUP.md#macos-setup)
- Requirements: Xcode Command Line Tools, Homebrew, LLVM, Rust
- Common issues: [macOS Troubleshooting](DEVELOPMENT_SETUP.md#platform-specific-issues)

### Linux Development

- [Ubuntu/Debian Setup](DEVELOPMENT_SETUP.md#ubuntudebian)
- [Fedora/RHEL Setup](DEVELOPMENT_SETUP.md#fedorarhel)
- [Arch Linux Setup](DEVELOPMENT_SETUP.md#arch-linux)
- Common issues: [Linux Troubleshooting](DEVELOPMENT_SETUP.md#platform-specific-issues)

---

## 📝 Contributing to Documentation

When adding or updating documentation:

1. **Follow the constitution**: See [Principle VIII](../.specify/memory/constitution.md#viii-self-documenting-code) for documentation standards
2. **Keep it practical**: Include real examples and use cases
3. **Test your instructions**: Verify setup steps work on clean systems
4. **Update this index**: Add new documents to the relevant section above

---

## 🔗 External Resources

### Rust Ecosystem

- [Rust Book](https://doc.rust-lang.org/book/) - Learn Rust programming
- [Cargo Book](https://doc.rust-lang.org/cargo/) - Rust package manager guide
- [rustup](https://rustup.rs/) - Rust toolchain installer

### Dependencies

- [RocksDB](https://rocksdb.org/) - Embedded database for hot storage
- [Apache Arrow](https://arrow.apache.org/) - Columnar data format
- [Apache Parquet](https://parquet.apache.org/) - Columnar storage format
- [DataFusion](https://arrow.apache.org/datafusion/) - SQL query engine
- [Actix-web](https://actix.rs/) - Web framework
- [Tokio](https://tokio.rs/) - Async runtime

### Tools

- [LLVM Project](https://llvm.org/) - Compiler infrastructure
- [Visual Studio](https://visualstudio.microsoft.com/) - Windows C++ tools
- [Homebrew](https://brew.sh/) - macOS package manager

---

**Last Updated**: October 14, 2025  
**KalamDB Version**: 0.1.0

---

**Need help?** Check the [Troubleshooting Guide](DEVELOPMENT_SETUP.md#troubleshooting) or [open an issue](https://github.com/jamals86/KalamDB/issues).
