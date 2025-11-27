# Research: Production Readiness & Full DML/DDL

**Feature**: `013-production-readiness`
**Date**: 2025-11-23

## 1. ALTER TABLE Implementation in DataFusion

**Decision**: Enhance existing `AlterTableHandler` in `kalamdb-core`.
**Rationale**:
- `backend/crates/kalamdb-core/src/sql/executor/handlers/table/alter.rs` already implements `ADD`, `DROP`, `MODIFY`.
- We will add `RENAME COLUMN` support.
- `RENAME` will be metadata-only: update `TableDefinition` in `SchemaRegistry`.
- Old Parquet files will retain old column names. DataFusion's `TableProvider` projection pushdown needs to handle the mapping, or we accept that old data for that column is lost/null unless we rewrite files.
- **Refinement**: For MVP, `RENAME` will be metadata-only. If the column name in Parquet doesn't match the schema, it will be read as NULL. This is acceptable for "Production Readiness" MVP, with a warning that it's a metadata operation.

## 2. Manifest Persistence & Lifecycle

**Decision**: Modify existing `ManifestService` for Hot/Cold split.
**Rationale**:
- `backend/crates/kalamdb-core/src/manifest/service.rs` currently writes to disk on every update (`write_manifest_atomic`).
- We will modify it to:
    - **Hot Store**: Keep `Manifest` in memory (or RocksDB if we want crash recovery before flush).
    - **Cold Store**: Write `manifest.json` only on `flush_manifest()`.
- `ensure_manifest_initialized` should load from `manifest.json` if exists, else create new.

## 3. System Tables Enhancements

**Decision**: Update existing `kalamdb-system` providers.
**Rationale**:
- `system.tables`: Modify `backend/crates/kalamdb-system/src/providers/tables/` to include `options` column.
- `system.live_queries`: Already exists in `backend/crates/kalamdb-system/src/providers/live_queries/`. No major changes needed unless schema is missing fields.

## 4. Update/Delete via DataFusion

**Decision**: Refactor existing handlers.
**Rationale**:
- `UpdateHandler` and `DeleteHandler` in `backend/crates/kalamdb-core/src/sql/executor/handlers/dml/` need to be standardized to use DataFusion's `Scan` -> `Filter` -> `Write` pattern.

## 5. Provider Refactoring

**Decision**: `BaseTableProvider` Trait + Composition.
**Rationale**:
- `User`, `Shared`, `Stream` providers share 90% of logic.
- Create `TableProviderCore` struct holding common state.
- Implement `BaseTableProvider` trait.


## 2. Manifest Persistence & Lifecycle

**Decision**: Hybrid Hot/Cold Manifest.
**Rationale**:
- **Hot Store**: A dedicated RocksDB column family `manifests` stores the `Manifest` struct (serialized). This ensures fast updates on inserts (append-only metadata) and crash consistency.
- **Cold Store**: `manifest.json` in the table directory.
- **Sync**:
    - On `INSERT`: Update Hot Store manifest (in-memory cache + RocksDB write).
    - On `FLUSH`: Read Hot Store manifest, update with new batch info, write `manifest.json` to disk, update Hot Store.
    - On `STARTUP`: Read `manifest.json` (if exists) and reconcile with Hot Store (if any). Or simply load `manifest.json` into Hot Store if Hot Store is empty/stale. *Refinement*: The Hot Store is the source of truth for *active* segments. `manifest.json` is the checkpoint.

**Alternatives Considered**:
- JSON only: Too slow for high-frequency inserts if we updated it every time.
- RocksDB only: Harder to recover/inspect manually.

## 3. System Tables Enhancements

**Decision**: Virtual Tables via `SystemTableProvider`.
**Rationale**:
- `system.tables`: The provider already scans `SchemaRegistry`. We just need to map the `options` field from `TableDefinition` to a JSON string column.
- `system.live_queries`: We need a `LiveQueryRegistry` singleton in `AppContext`.
    - `QueryContext` registers itself on creation and deregisters on drop.
    - The provider scans this registry.

## 4. Update/Delete via DataFusion

**Decision**: `Scan` -> `Filter` -> `Write`.
**Rationale**:
- `DELETE`: `DELETE FROM t WHERE predicate` -> `SELECT pk FROM t WHERE predicate` (via DataFusion) -> `delete_batch(pks)`.
- `UPDATE`: `UPDATE t SET c=v WHERE predicate` -> `SELECT pk, ... FROM t WHERE predicate` -> `update_batch(pks, new_values)`.
- This ensures we support all DataFusion predicates (complex filters, joins, etc.) without re-implementing the wheel.

## 5. Provider Refactoring

**Decision**: `BaseTableProvider` Trait + Composition.
**Rationale**:
- `User`, `Shared`, `Stream` providers share 90% of logic (scan, insert, update).
- Create a `TableProviderCore` struct holding common state (store, schema, etc.).
- Implement `BaseTableProvider` trait with default methods for `scan`, `insert`, `update`.
- Specific providers just implement the `get_storage_key` and `apply_rls` logic.

