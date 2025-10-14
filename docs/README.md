# KalamDB Documentation

Welcome to KalamDB documentation! This folder contains guides for getting started with KalamDB development.

## 📖 Table of Contents

### Getting Started

- **[🚀 Quick Start Guide](QUICK_START.md)** - Get KalamDB running in under 10 minutes
  - Platform-specific quick starts for Windows, macOS, and Linux
  - Essential commands and verification steps
  - Basic usage examples

- **[📘 Development Setup Guide](DEVELOPMENT_SETUP.md)** - Comprehensive installation guide
  - Detailed prerequisites and system requirements
  - Step-by-step setup for Windows, macOS, and Linux
  - LLVM/Clang installation instructions
  - C++ compiler setup for each platform
  - Complete troubleshooting section

- **[🔧 Troubleshooting Checklist](TROUBLESHOOTING.md)** - Quick problem-solving reference
  - Pre-build verification checklist
  - Common error messages and solutions
  - Build performance tips
  - Environment diagnostic scripts

### Project Documentation

- **[Backend README](../backend/README.md)** - Backend project structure and development workflow
- **[Main README](../README.md)** - Project overview and architecture

### Architecture & Specifications

- **[Complete Specification](../specs/001-build-a-rust/SPECIFICATION-COMPLETE.md)** - Full design document
- **[Data Model](../specs/001-build-a-rust/data-model.md)** - Entities, schemas, and lifecycle
- **[API Architecture](../specs/001-build-a-rust/API-ARCHITECTURE.md)** - SQL-first design approach
- **[SQL Query Examples](../specs/001-build-a-rust/sql-query-examples.md)** - Query patterns cookbook

### Protocols & APIs

- **[REST API (OpenAPI)](../specs/001-build-a-rust/contracts/rest-api.yaml)** - HTTP endpoint specifications
- **[WebSocket Protocol](../specs/001-build-a-rust/contracts/websocket-protocol.md)** - Real-time streaming protocol

### Development Guidelines

- **[Project Constitution](../.specify/memory/constitution.md)** - Core principles and development standards
- **[Implementation Plan](../specs/001-build-a-rust/plan.md)** - Development roadmap and phases

---

## 🎯 Quick Links by Role

### New Contributors

Start here if you're new to KalamDB:

1. Read the [Main README](../README.md) to understand what KalamDB is
2. Follow the [Quick Start Guide](QUICK_START.md) to get it running
3. Review the [Project Constitution](../.specify/memory/constitution.md) for development principles
4. Check the [Implementation Plan](../specs/001-build-a-rust/plan.md) for current status

### Developers

Building features or fixing bugs:

1. Use the [Development Setup Guide](DEVELOPMENT_SETUP.md) for environment setup
2. Read the [Backend README](../backend/README.md) for project structure
3. Study the [Data Model](../specs/001-build-a-rust/data-model.md) for entities and relationships
4. Reference the [SQL Query Examples](../specs/001-build-a-rust/sql-query-examples.md) for query patterns

### API Integrators

Building clients or integrations:

1. Review the [REST API Specification](../specs/001-build-a-rust/contracts/rest-api.yaml)
2. Learn the [WebSocket Protocol](../specs/001-build-a-rust/contracts/websocket-protocol.md)
3. Check the [API Architecture](../specs/001-build-a-rust/API-ARCHITECTURE.md) for design decisions
4. Try the [SQL Query Examples](../specs/001-build-a-rust/sql-query-examples.md)

### Architects

Understanding design decisions:

1. Read the [Complete Specification](../specs/001-build-a-rust/SPECIFICATION-COMPLETE.md)
2. Study the [Table-Per-User Architecture](../README.md#-what-makes-kalamdb-different)
3. Review the [Project Constitution](../.specify/memory/constitution.md) for principles
4. Examine the [API Architecture](../specs/001-build-a-rust/API-ARCHITECTURE.md)

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
- [Actix-web](https://actix.rs/) - Web framework (future)
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
