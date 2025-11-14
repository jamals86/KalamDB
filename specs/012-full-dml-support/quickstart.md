# Quickstart: Full DML Support Feature

1. **Checkout branch**
   ```bash
   git checkout 012-full-dml-support
   ```
2. **Build workspace** to ensure baseline passes prior to feature work.
   ```bash
   cargo check --workspace
   ```
3. **Review spec & plan** located in `specs/012-full-dml-support/` (`spec.md`, `plan.md`, `research.md`).
4. **Set up configuration** using `backend/config.toml`; ensure `[server] node_id` and `[manifest_cache]` blocks are populated per spec examples.
5. **Run smoke tests** to validate current behavior before modifying DML paths.
   ```bash
   cargo test -p kalamdb-core smoke_test
   ```
6. **Implement increments** following this order:
   1. Introduce `SystemColumnsService` and integrate with DDL/DML/query pipelines.
   2. Extend SQL parser/AST for `AS USER` and impersonation context plumbing.
   3. Add append-only UPDATE/DELETE execution paths with nanosecond `_updated` handling.
   4. Implement ManifestService + ManifestCacheStore, update flush/query code, and expose `SHOW MANIFEST CACHE`/API.
   5. Add Parquet Bloom filter generation and planner integration.
   6. Refactor UnifiedJobManager executors to typed parameter structs.
7. **Validate**
   - Unit/integration tests covering multi-version queries, deletes, impersonation, manifest caching, bloom filters.
   - Performance harnesses for query latency, manifest pruning, and update throughput (FR-102–FR-107).
8. **Document configuration & operational changes** in project README or admin guides as needed.
