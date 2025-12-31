# KalamDB Documentation

Welcome to KalamDB documentation! This folder is organized into the following sections:

## 📖 Documentation Structure

### [getting-started/](getting-started/)
Quick start guides for new users and contributors.

- **[quick-start.md](getting-started/quick-start.md)** – Build and run your first query
- **[cli.md](getting-started/cli.md)** – Using the `kalam` command-line client

### [reference/](reference/)
Core reference documentation for SQL syntax and behavior.

- **[sql.md](reference/sql.md)** – Complete SQL syntax reference
- **[identifiers.md](reference/identifiers.md)** – Case sensitivity and naming rules
- **[timestamp-formatting.md](reference/timestamp-formatting.md)** – Timestamp handling

### [api/](api/)
HTTP and WebSocket API documentation.

- **[api.md](api/api.md)** – Quick API overview
- **[api-reference.md](api/api-reference.md)** – Complete REST API reference
- **[websocket-protocol.md](api/websocket-protocol.md)** – WebSocket subscription protocol

### [sdk/](sdk/)
Client SDK documentation.

- **[sdk.md](sdk/sdk.md)** – TypeScript/JavaScript SDK guide

### [architecture/](architecture/)
System architecture and design decisions.

- **[decisions/](architecture/decisions/)** – Architecture Decision Records (ADRs)
- **[manifest.md](architecture/manifest.md)** – Manifest file format
- **[conversion-architecture.md](architecture/conversion-architecture.md)** – Data type conversion

### [development/](development/)
Contributor guides for building and developing KalamDB.

- **[build.md](development/build.md)** – Build guide (start here)
- **[development-setup.md](development/development-setup.md)** – Full development setup
- **[macos.md](development/macos.md)** / **[linux.md](development/linux.md)** / **[windows.md](development/windows.md)** – Platform guides
- **[testing-strategy.md](development/testing-strategy.md)** – Testing approach
- **[how-to-add-sql-statement.md](development/how-to-add-sql-statement.md)** – Adding new SQL statements

### [API-Kalam/](API-Kalam/)
Bruno API collection for testing endpoints.

---

## 🎯 Quick Links by Role

### New Users
1. [Quick Start](getting-started/quick-start.md) – Get KalamDB running
2. [SQL Reference](reference/sql.md) – Learn the SQL syntax
3. [CLI Guide](getting-started/cli.md) – Use the command-line client

### API Developers
1. [API Reference](api/api-reference.md) – REST endpoints
2. [WebSocket Protocol](api/websocket-protocol.md) – Real-time subscriptions
3. [SDK Guide](sdk/sdk.md) – TypeScript/JavaScript client

### Contributors
1. [Build Guide](development/build.md) – Build the project
2. [Development Setup](development/development-setup.md) – Full environment setup
3. [Architecture Decisions](architecture/decisions/) – Understand design choices

---

## 💡 Common Questions

### How do I get started with development?

Follow the [Quick Start Guide](getting-started/quick-start.md) for your platform. If you encounter issues, see the [Troubleshooting section](development/development-setup.md#troubleshooting) in the full setup guide.

### Why do I need LLVM/Clang?

KalamDB depends on native libraries (RocksDB, Arrow, Parquet) written in C++. The Rust build process needs LLVM/Clang to compile these dependencies. See the [Build Guide](development/build.md) for details.

### What's the table-per-user architecture?

KalamDB stores each user's messages in isolated storage partitions instead of a shared table. This enables massive scalability for real-time subscriptions. Read more in the [Main README](../README.md#-what-makes-kalamdb-different).

---

## 📚 Related Documentation

- **[AGENTS.md](../AGENTS.md)** – AI/Agent coding guidelines
- **[Backend README](../backend/README.md)** – Backend project structure
- **[Main README](../README.md)** – Project overview

---

**Last Updated**: December 2025  
**KalamDB Version**: 0.1.x
