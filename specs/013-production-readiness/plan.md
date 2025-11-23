# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]
**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This feature implements full production readiness for KalamDB by finalizing DML support (UPDATE/DELETE) with DataFusion integration, adding full DDL support (ALTER TABLE), ensuring data durability via manifest persistence, and enhancing system observability. It also involves a significant refactoring of table providers to reduce duplication and enforce architectural standards.

## Technical Context

**Language/Version**: Rust 1.90+ (edition 2021)
**Primary Dependencies**: DataFusion 40.0, Apache Arrow 52.0, RocksDB 0.24, Actix-Web 4.4
**Storage**: RocksDB (hot path), Parquet (cold path), manifest.json (metadata)
**Testing**: cargo test, integration tests (smoke tests)
**Target Platform**: Linux/macOS (server)
**Project Type**: Backend Database
**Performance Goals**: <1ms write path, efficient scans via manifest pruning
**Constraints**: Durability across restarts, strict DataFusion integration, no direct storage access for queries
**Scale/Scope**: Core architecture enhancement, affecting all table types

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- [x] **Library-First**: Implemented within `kalamdb-core` and `kalamdb-store`.
- [x] **CLI Interface**: SQL interface via CLI/API.
- [x] **Test-First**: Spec defines independent tests and acceptance scenarios.
- [x] **Integration Testing**: Required for durability and restart scenarios.
- [x] **Observability**: Enhances system tables (`live_queries`, `options`) for better observability.

## Project Structure

### Documentation (this feature)

```text
specs/013-production-readiness/
├── plan.md              # This file
├── research.md          # Research findings
├── data-model.md        # Schema definitions
├── quickstart.md        # Usage guide
├── contracts/           # API contracts
│   └── api.ts
```

### Source Code (repository root)

```text
backend/crates/
├── kalamdb-core/
│   ├── src/
│   │   ├── storage/
│   │   │   └── manifest_service.rs       # NEW: Manifest management
│   │   ├── sql/executor/handlers/
│   │   │   ├── ddl/
│   │   │   │   └── alter_table.rs        # NEW: ALTER TABLE handler
│   │   │   └── dml/
│   │   │   │   ├── update.rs             # REFACTOR: DataFusion integration
│   │   │   │   └── delete.rs             # REFACTOR: Consistency
│   │   └── tables/
│   │       ├── system/
│   │       │   ├── tables.rs             # UPDATE: Add options column
│   │       │   └── live_queries.rs       # NEW: Live query monitoring
│   │       └── base_table_provider.rs    # REFACTOR: Common logic
├── kalamdb-commons/
│   ├── src/
│   │   └── models/
│   │       ├── manifest.rs               # UPDATE: Schema sync
│   │       └── schemas/table.rs          # UPDATE: Options field
```

**Structure Decision**: We are extending the existing `kalamdb-core` and `kalamdb-commons` crates. No new crates are needed. The `ManifestService` will centralize manifest logic which is currently scattered.

## Phase 2: Implementation Steps

### Step 1: Manifest & Storage Foundation
- [x] **Refactor `Manifest` struct**: Ensure it matches `data-model.md` in `kalamdb-commons`.
    - *Update*: Replaced `min_key`/`max_key` with `column_stats` (HashMap) for flexible indexing.
- [x] **Modify `ManifestService`**:
    - Edit `backend/crates/kalamdb-core/src/manifest/service.rs`.
    - Implement Hot/Cold split:
        - `update_manifest`: Update in-memory/RocksDB only (Hot Store).
        - `flush_manifest`: New method to write `manifest.json` to disk (Cold Store).
    - Ensure `ensure_manifest_initialized` loads from Cold Store if Hot Store is empty.
- [ ] **Update `UserTableProvider`**:
    - Integrate `ManifestService`.
    - Ensure `insert` updates the manifest (Hot Store).
    - Ensure `scan` reads from manifest segments + RocksDB memtable.

### Step 2: DataFusion DML Refactor
- [ ] **Refactor `UpdateHandler`**:
    - Edit `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/update.rs`.
    - Use `ctx.sql("SELECT ...")` to find rows matching the predicate.
    - Collect results and call `table.update_batch(keys, values)`.
- [ ] **Refactor `DeleteHandler`**:
    - Edit `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/delete.rs`.
    - Ensure it follows the same pattern: `Scan` -> `Filter` -> `Delete`.

### Step 3: DDL Support (ALTER TABLE)
- [ ] **Enhance `AlterTableHandler`**:
    - Edit `backend/crates/kalamdb-core/src/sql/executor/handlers/table/alter.rs`.
    - Add `RENAME COLUMN` support (Metadata only).
    - Verify `ADD/DROP/MODIFY` logic covers all edge cases.
- [ ] **Update `SchemaRegistry`**:
    - Ensure `alter_table` properly invalidates the cached `TableProvider`.

### Step 4: System Tables
- [ ] **Enhance `SystemTableProvider`**:
    - Edit `backend/crates/kalamdb-system/src/providers/tables/tables_provider.rs`.
    - Add `options` column to the schema and data mapping.
- [ ] **Verify `LiveQueryRegistry`**:
    - Check `backend/crates/kalamdb-core/src/live/manager/core.rs` and `backend/crates/kalamdb-system/src/providers/live_queries/`.
    - Ensure `system.live_queries` exposes all necessary fields.

### Step 5: Integration & Testing
- [ ] **Add Integration Tests**:
    - `tests/test_alter_table.rs`: Test ADD/DROP/RENAME column.
    - `tests/test_manifest_persistence.rs`: Verify `manifest.json` creation only on flush.
    - `tests/test_dml_complex.rs`: Test UPDATE/DELETE with complex predicates.
- [ ] **Run Smoke Tests**: Ensure no regressions.

## Phase 3: Validation
- [ ] Manual verification using `quickstart.md`.
- [ ] Performance benchmark for Manifest overhead.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| None | N/A | N/A |

