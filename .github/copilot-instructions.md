# KalamDB Development Guidelines

**⚠️ PRIMARY DOCUMENTATION**: See [AGENTS.md](../AGENTS.md) in the root directory for complete development guidelines.

This file references the main `AGENTS.md` which contains:
- System Table Models Architecture
- Dependency Management (workspace dependencies)
- Active Technologies
- Project Structure
- SDK Architecture Principles
- Commands and Code Style
- Recent Changes

Always refer to `AGENTS.md` for the most up-to-date guidelines and best practices.

## Active Technologies
- Rust 1.90 (edition 2021) + DataFusion 40, Apache Arrow 52, Apache Parquet 52, Actix-Web 4, `kalamdb-store` EntityStore traits, `kalamdb-commons` system models (007-user-auth)
- RocksDB 0.24 for buffered writes, Parquet files for flushed segments via StorageBackend abstraction (007-user-auth)

## Recent Changes
- 007-user-auth: Added Rust 1.90 (edition 2021) + DataFusion 40, Apache Arrow 52, Apache Parquet 52, Actix-Web 4, `kalamdb-store` EntityStore traits, `kalamdb-commons` system models
