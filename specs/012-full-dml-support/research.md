# Phase 0 Research: Full DML Support

## Append-Only UPDATE/DELETE Strategy
- Decision: Implement UPDATE/DELETE by appending new record versions in fast storage, keeping long-term Parquet files immutable and resolving latest state via MAX(`_updated`).
- Rationale: Aligns with existing append-only architecture, avoids costly in-place Parquet rewrites, and satisfies FR-001–FR-015 for multi-version consistency.
- Alternatives considered: In-place Parquet rewrite (rejected due to I/O cost and concurrency risk); tombstone-only deletes without `_updated` refresh (rejected because version ordering would break).

## Manifest Lifecycle & Caching
- Decision: Introduce ManifestService managing manifest.json generation, validation, caching via ManifestCacheStore (RocksDB CF + DashMap hot cache), and rebuild flows.
- Rationale: Centralizes manifest ownership, enables sub-millisecond cache hits, complies with FR-016–FR-046 and FR-047–FR-053, and leverages existing SchemaRegistry patterns.
- Alternatives considered: Direct Parquet directory scans per query (rejected for latency); distributed cache (e.g., Redis) (rejected as unnecessary complexity for single-node deployments).

## Bloom Filter Integration
- Decision: Generate Parquet Bloom filters for `_id`, `_updated`, and schema-declared indexed columns during flush using Apache Parquet bloom filter writer APIs with configurable false-positive rates.
- Rationale: Meets FR-054–FR-061 to reduce I/O for point queries while reusing Parquet-native tooling; keeps probabilistic filtering co-located with batch metadata.
- Alternatives considered: External inverted indexes (rejected due to maintenance overhead); custom hash tables (rejected because Parquet already provides compact bloom filters).

## AS USER Impersonation Controls
- Decision: Extend SQL grammar to accept `AS USER 'user_id'` for INSERT/UPDATE/DELETE on User/Stream tables, enforcing service/admin role checks, user existence validation (active only), and dual audit logging.
- Rationale: Satisfies FR-062–FR-069 and clarifications; enables service impersonation without session switching while preserving RLS semantics and auditability.
- Alternatives considered: Separate impersonation API (rejected to keep SQL-first interface); unrestricted AS USER (rejected for security reasons).

## SystemColumnsService Centralization
- Decision: Create SystemColumnsService responsible for injecting `_id`, `_updated`, `_deleted` at table creation, generating Snowflake IDs, handling update/delete mutations, and applying automatic `_deleted = false` filters.
- Rationale: Fulfills FR-070–FR-081, SC-014–SC-016, and eliminates scattered logic; ensures consistent system column handling across DDL, DML, and query layers.
- Alternatives considered: Multiple helper modules per subsystem (rejected for duplication); leaving legacy scattered logic (rejected due to maintenance risk).

## Snowflake ID Generation
- Decision: Derive 64-bit IDs using timestamp + hashed `node_id` + per-node sequence, pulling configured `node_id` via `AppContext.config().server.node_id` and providing monotonic increments with 1 ns adjustments.
- Rationale: Meets SC-015 and clarifications, leverages existing NodeId infrastructure, and keeps ID generation deterministic per node.
- Alternatives considered: UUIDv4 (rejected—non-orderable, higher storage); centralized ID allocator (rejected—single point of failure).

## Typed Job Executor Parameters
- Decision: Refactor UnifiedJobManager executors to use typed serde-powered parameter structs with compile-time types and JSON persistence for backward compatibility.
- Rationale: Addresses FR-088–FR-095, reduces boilerplate parsing, improves validation, and preserves existing storage format.
- Alternatives considered: Runtime JSON schema validation (rejected—higher runtime overhead); binary serialization (rejected—less debuggable, harder evolution).

## Configuration Access Unification
- Decision: Route all configuration reads through `AppContext.config()` canonical structs, removing direct file access and duplicative DTOs.
- Rationale: Delivers FR-082–FR-087, enforces single source of truth, and ties into AppContext-first pattern established in previous phases.
- Alternatives considered: Module-specific config loaders (rejected—drift risk); environment-variable fallbacks without central config (rejected—fragmented state).

## Governance Clarification
- Decision: Escalate absence of populated SpecKit constitution to maintainers while continuing under KalamDB AGENTS.md guidelines for immediate planning.
- Rationale: Required to address Gate 1 blocker annotated in plan.md; AGENTS.md already prescribes architecture standards ensuring progress.
- Alternatives considered: Halting planning until constitution provided (rejected—would stall spec execution without delivering work-in-progress artifacts demanded by process).
