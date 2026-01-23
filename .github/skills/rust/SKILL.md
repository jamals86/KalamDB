---
name: rust
description: 'Rust development for KalamDB: apply project conventions, error handling, async patterns, dependency rules, and performance guidelines. Keywords: rust, cargo, tokio, Actix-Web, Arrow, DataFusion, Parquet, RocksDB.'
---

Use this skill for Rust tasks in KalamDB, including core services, storage, SQL handlers, system tables, and job executors.

Step-by-step guidance:
1) Read AGENTS.md first for project-wide rules, especially model separation, AppContext usage, dependency rules, and storage boundaries.
2) Identify the target crate in backend/crates and keep changes scoped to the correct layer (core, store, filestore, auth, api).
3) Prefer type-safe IDs and enums (NamespaceId, TableId, UserId, Role, TableType) instead of raw strings.
4) Use Arc for shared state and avoid cloning large data. Prefer DashMap for concurrent maps.
5) Keep errors typed (Result<T, KalamDbError>) and avoid panics. Log with log macros.
6) For async code, use tokio and async/await; avoid blocking in async context.
7) When adding dependencies, update root Cargo.toml under [workspace.dependencies] and reference with { workspace = true }.

Common patterns:
- AppContext-first APIs: take Arc<AppContext> rather than passing multiple services.
- Avoid importing within functions; place use statements at the top of the file.
- If converting Option<Uuid> to UserId, import UserId at file top and use UserId::from instead of inline map.
- When handling storage, keep RocksDB specifics in kalamdb-store and filesystem operations in kalamdb-filestore.

Edge cases to watch:
- Avoid leaking raw strings for enums or IDs in API/SQL paths.
- Don’t mix multiple models in one file (one model per file).
- Avoid repeated cargo check loops; batch fixes and rerun once.

When uncertain about where a change belongs, scan for existing abstractions (StorageBackend, EntityStore, schema_registry, live). Use or extend them instead of creating new parallel logic.