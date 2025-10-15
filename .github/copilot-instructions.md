# KalamDB Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-10-15

## Active Technologies
- (001-build-a-rust)
- Rust 1.75+ (stable toolchain, edition 2021) (002-simple-kalamdb)
- RocksDB 0.21, Apache Arrow 50.0, Apache Parquet 50.0, DataFusion 35.0, Actix-Web 4.4 (002-simple-kalamdb)

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
- 2025-10-15: Tasks.md synchronized with plan.md (229 tasks, 100% aligned)
- 2025-10-15: Added testing tasks (T227-T229: quickstart script, benchmarks, integration tests)
- 2025-10-15: Fixed terminology: table_id → table_name for live query architecture
- 002-simple-kalamdb: Added Rust 1.75+ (stable toolchain, edition 2021)
- 001-build-a-rust: Added

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
