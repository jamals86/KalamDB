# KalamDB Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-10-15

## Active Technologies
- (001-build-a-rust)
- Rust 1.75+ (stable toolchain, edition 2021) (002-simple-kalamdb)
- RocksDB 0.21, Apache Arrow 50.0, Apache Parquet 50.0, DataFusion 35.0, Actix-Web 4.4 (002-simple-kalamdb)
- RocksDB for write path (<1ms), Parquet for flushed storage (compressed columnar format) (004-system-improvements-and)
- Rust 1.75+ (backend), TypeScript/JavaScript ES2020+ (frontend), Bash (scripts) (006-docker-wasm-examples)
- RocksDB (existing), browser localStorage (new) (006-docker-wasm-examples)

## Project Structure
```
backend/crates/kalamdb-core/    # Core library (embeddable)
backend/crates/kalamdb-api/     # REST API and WebSocket
backend/crates/kalamdb-server/  # Server binary
specs/002-simple-kalamdb/       # Complete planning docs
```

## Commands
# Build: cargo build
# Test: cargo test
# Run: cargo run --bin kalamdb-server

## Code Style
Rust 2021 edition: Follow standard conventions, use type-safe wrappers (NamespaceId, TableName, UserId, TableType enum)

## Recent Changes
- 006-docker-wasm-examples: Added Rust 1.75+ (backend), TypeScript/JavaScript ES2020+ (frontend), Bash (scripts)
- 004-system-improvements-and: Added Rust 1.75+ (stable toolchain, edition 2021)
- 2025-10-15: Tasks.md synchronized with plan.md (229 tasks, 100% aligned)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
