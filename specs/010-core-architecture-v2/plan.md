# Implementation Plan: Core Architecture Refactoring v2

**Branch**: `010-core-architecture-v2` | **Date**: 2025-11-06 | **Spec**: [spec.md](./spec.md)

## Summary

This refactoring consolidates KalamDB's core architecture around AppContext singleton pattern, improves schema lookup performance through Arrow schema memoization (50-100× speedup), and unifies scattered components into cohesive managers. The work must follow strict implementation order: AppContext centralization → schema_registry rename → Arrow memoization → executor migration → LiveQueryManager → system tables → views.

## Technical Context

**Language/Version**: Rust 1.90+ (stable toolchain, edition 2021)  
**Primary Dependencies**: DataFusion 40.0, Apache Arrow 52.0, Apache Parquet 52.0, RocksDB 0.24, DashMap (lock-free concurrent HashMap)  
**Storage**: RocksDB for write path (<1ms), Parquet for flushed storage (compressed columnar format)  
**Testing**: cargo test (477 existing tests in kalamdb-core, must maintain 100% pass rate)  
**Target Platform**: Linux/Windows server, embedded library (kalamdb-core is embeddable)  
**Project Type**: Single Rust workspace with multiple crates  
**Performance Goals**: 50-100× speedup for schema access (75μs → 1.5μs), 99% cache hit rate, <2MB memory overhead for 1000 tables  
**Constraints**: Zero breaking changes to existing 477 tests, maintain backward compatibility during refactoring, strict implementation order to avoid rework  
**Scale/Scope**: 11 TableProvider implementations, 8 system tables, ~10 major refactoring phases across 5 crates

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: ✅ **PASS** (Refactoring aligns with established architecture patterns)

### Principles Verification

1. **Library-First**: ✅ kalamdb-core remains embeddable, changes isolated within existing crate boundaries
2. **Test-First**: ✅ Existing 477 tests provide regression safety, new functionality requires benchmark tests
3. **Observability**: ✅ NodeId centralization improves distributed logging correlation
4. **Simplicity**: ✅ Consolidation reduces complexity (LiveQueryManager merges 3 structs, SchemaCache unifies dual caches)
5. **Performance**: ✅ Arrow memoization eliminates redundant computation, cache-first patterns throughout

### Architectural Consistency

- **AppContext Pattern**: Already established in Phase 5, this extends it to NodeId ownership
- **SchemaRegistry**: Phase 5 foundation complete, this adds Arrow memoization layer
- **DashMap Usage**: Consistent with existing Phase 10 cache implementation
- **StorageBackend Abstraction**: System tables use existing pattern from shared tables

**No violations requiring justification.**

## Project Structure

### Documentation (this feature)

```text
specs/010-core-architecture-v2/
├── spec.md                              # Feature specification (complete)
├── plan.md                              # This file
├── research.md                          # Phase 0 output (to be generated)
├── data-model.md                        # Phase 1 output (to be generated)
├── quickstart.md                        # Phase 1 output (to be generated)
├── contracts/                           # Phase 1 output (N/A for internal refactoring)
├── checklists/
│   └── requirements.md                  # Validation checklist (complete)
└── DATAFUSION_ARCHITECTURE_ANALYSIS.md  # Performance analysis (complete)
```

### Source Code (repository root)

```text
backend/crates/
├── kalamdb-core/                        # PRIMARY: Core refactoring target
│   ├── src/
│   │   ├── app_context.rs               # MODIFY: Add NodeId ownership (FR-000)
│   │   ├── schema_registry/             # RENAME from schema/ (FR-001)
│   │   │   ├── registry.rs          # MODIFY: Add arrow_schemas DashMap (FR-002-004)
│   │   │   ├── arrow_schema.rs          # Schema utilities (existing)
│   │   │   └── system_columns.rs        # System column injection (existing)
│   │   ├── tables/
│   │   │   ├── base_table_provider.rs   # MODIFY: Add arrow_schema() method (FR-005)
│   │   │   ├── user_tables/
│   │   │   │   └── user_table_provider.rs  # MODIFY: Use memoized schemas (FR-006)
│   │   │   ├── shared_tables/
│   │   │   │   └── shared_table_provider.rs # MODIFY: Use memoized schemas (FR-006)
│   │   │   ├── stream_tables/
│   │   │   │   └── stream_table_provider.rs # MODIFY: Use memoized schemas (FR-006)
│   │   │   └── system/                  # MODIFY: 8 providers use memoized schemas (FR-006)
│   │   │       ├── users/users_provider.rs
│   │   │       ├── jobs/jobs_provider.rs
│   │   │       ├── namespaces/namespaces_provider.rs
│   │   │       ├── storages/storages_provider.rs
│   │   │       ├── live_queries/live_queries_provider.rs
│   │   │       ├── tables/tables_provider.rs
│   │   │       ├── audit_logs/audit_logs_provider.rs
│   │   │       └── stats.rs
│   │   ├── sql/
│   │   │   └── executor/                # REFACTOR after FR-000 to FR-006 (FR-015-016)
│   │   │       ├── mod.rs               # PRIMARY: Routing orchestrator with AppContext
│   │   │       └── handlers/            # All handlers receive &AppContext
│   │   ├── live_query/                  # REFACTOR: Consolidate (FR-007-008)
│   │   │   └── manager.rs               # Merge UserConnections, UserTableChangeDetector
│   │   └── test_helpers.rs              # MODIFY: Update test fixtures for new patterns
│   └── tests/                           # MAINTAIN: 477 tests must pass (FR-017)
├── kalamdb-commons/                     # System table models (no changes)
│   └── src/
│       ├── system_tables.rs             # User, Job, Namespace, Storage (existing)
│       └── models/schemas/              # TableDefinition, ColumnDefinition (existing)
├── kalamdb-store/                       # RocksDB storage layer (no changes)
├── kalamdb-sql/                         # SQL parsing (no changes)
├── kalamdb-auth/                        # Authentication (no changes)
└── kalamdb-api/                         # REST API (no changes)

backend/src/
├── config.rs                            # MODIFY: Ensure node_id config field exists
└── lifecycle.rs                         # MODIFY: Pass config.node_id to AppContext.init()

specs/010-core-architecture-v2/          # Feature documentation (see above)
```

**Structure Decision**: Single Rust workspace structure maintained. Primary changes in `kalamdb-core` crate (11 files modified for Arrow memoization, extensive refactoring in sql/executor/ and live_query/). Zero changes to kalamdb-store, kalamdb-sql, kalamdb-auth crates. Configuration changes minimal (node_id loading).

## Complexity Tracking

> **No violations requiring justification**

All refactoring aligns with established patterns from Phase 5 (SchemaRegistry), Phase 10 (Cache Consolidation), and Phase 7 (Handler-based executor). The strict implementation order prevents the complexity of partial migrations that were attempted previously.

