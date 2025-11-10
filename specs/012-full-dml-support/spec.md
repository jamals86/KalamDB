# Feature Specification: Full DML Support

**Feature Branch**: `012-full-dml-support`  
**Created**: 2025-11-10  
**Status**: Draft  
**Input**: User description: "Support Update/Delete of flushed tables with manifest files, AS USER support for DML statements, centralized config usage via AppContext, and generic JobExecutor parameters"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Update and Delete Persisted Table Records (Priority: P1)

A database administrator needs to update or delete specific records from a user table, including records that have been persisted to long-term storage. The system uses an append-only architecture where updates create new versions and queries automatically retrieve the latest version using timestamp-based resolution.

**Why this priority**: Core DML functionality - without UPDATE/DELETE on persisted tables, users cannot modify historical data, making the database read-only for persisted records. This is essential for data correction, GDPR compliance (right to erasure), and normal database operations.

**Independent Test**: Can be fully tested by creating a user table, inserting records, persisting to long-term storage, then executing UPDATE/DELETE statements. Success is verified by querying the table and confirming only the latest version of each record is returned based on timestamp ordering.

**Acceptance Scenarios**:

1. **Given** a record exists only in fast storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system updates the record in fast storage and sets `_updated` to current timestamp with nanosecond precision
2. **Given** a record has been persisted to long-term storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system creates a new version in fast storage with updated `_updated` timestamp, leaving the old version in long-term storage unchanged
3. **Given** a record has been updated 3 times (original + 2 updates all persisted), **When** user queries the record, **Then** the system returns only the version with MAX(_updated) timestamp by joining all storage layers
4. **Given** a persisted record, **When** user executes `DELETE FROM table WHERE id = X`, **Then** the system sets `_deleted = true` and updates `_updated` timestamp in fast storage, and subsequent queries exclude this record
5. **Given** a deleted record (`_deleted = true`), **When** user queries the table, **Then** the record is filtered out (WHERE _deleted = false) even though historical versions exist in long-term storage

---

### User Story 2 - Metadata Index for Query Optimization (Priority: P2)

System administrators need efficient query execution on tables with many persisted storage files. A manifest file per table tracks batch file metadata including min/max values for all columns (especially `_updated`), enabling the query planner to skip unnecessary file scans based on timestamp ranges and column predicates.

**Why this priority**: Performance optimization for large tables - without manifest files, queries must scan all persisted batch files even when data is concentrated in specific time windows or value ranges. This becomes critical as tables grow beyond 100+ batch files. Secondary to core UPDATE/DELETE functionality.

**Independent Test**: Can be fully tested by creating a table, flushing data to multiple batch files, then querying with selective WHERE clauses. Success is verified by observing that the query planner only reads relevant batch files based on manifest metadata (min/max _updated, min/max column values).

**Acceptance Scenarios**:

1. **Given** a table with data flushed to 5 batch files (batch-0001.parquet to batch-0005.parquet), **When** query includes `WHERE _updated >= '2025-11-10T00:00:00Z'`, **Then** the system uses manifest to identify only batch files with max_updated overlapping that range and skips older batches
2. **Given** a manifest tracking min/max values per column per batch file, **When** query includes `WHERE id = 12345`, **Then** the system scans only batch files where 12345 falls within the indexed ID range
3. **Given** a query targeting only fast storage data, **When** manifest indicates no relevant data in long-term storage based on _updated ranges, **Then** the system skips all batch file reads
4. **Given** a flush operation completing, **When** new batch file is written, **Then** the system reads current manifest, increments max_batch counter, writes batch-{max_batch+1}.parquet, and updates manifest with new batch metadata
5. **Given** a manifest file with batch entries, **When** query execution reads manifest, **Then** the manifest provides file path, min/max values for all columns including _updated (nanosecond precision), row count, byte size, and schema version

---

### User Story 3 - Bloom Filter Optimization for Batch Files (Priority: P2)

Query performance on large batch files needs row-level filtering before reading full column data. Bloom filters embedded in Parquet files enable efficient point lookups by quickly eliminating batch files that definitely don't contain a specific ID or timestamp value.

**Why this priority**: Critical performance optimization for point queries (e.g., `WHERE id = X`) - without Bloom filters, the system must read and decompress full columns from every batch file. Bloom filters provide probabilistic set membership testing with minimal space overhead (<1% of file size). Secondary to basic manifest-based file skipping.

**Independent Test**: Can be fully tested by creating a table with 100K records flushed across 10 batch files, then executing `WHERE id = specific_value` queries. Success is verified by observing that only 1-2 batch files are scanned (those where Bloom filter returns "maybe") instead of all 10 files.

**Acceptance Scenarios**:

1. **Given** a flush operation writing a new batch file, **When** Parquet file is created, **Then** the system automatically generates Bloom filters for `id` column and `_updated` column by default
2. **Given** a table with indexed columns defined in schema, **When** batch file is written, **Then** the system creates Bloom filters for all indexed columns in addition to default `id` and `_updated` filters
3. **Given** a batch file with Bloom filters, **When** query includes `WHERE id = 12345`, **Then** the system tests Bloom filter before reading column data, skipping files where filter returns "definitely not present"
4. **Given** a query with `WHERE indexed_column = value`, **When** indexed_column has a Bloom filter, **Then** the system uses the filter to eliminate batch files, reducing I/O by 90%+ for point lookups
5. **Given** Bloom filter false positive rate configuration, **When** batch file is written, **Then** the system tunes filter size to achieve target false positive rate (default 1%) balancing space vs accuracy

---

### User Story 4 - Execute DML as Different User (AS USER) (Priority: P2)

A service account or admin needs to insert, update, or delete records on behalf of a specific user without switching authentication context. This enables system operations like message routing, AI-generated content, and cross-user notifications.

**Why this priority**: Critical for multi-tenant systems where services need to act on behalf of users (e.g., chat systems, notification services, AI assistants). Without this, services would need separate authentication per user, creating security and performance overhead.

**Independent Test**: Can be fully tested by authenticating as a service/admin user, executing `INSERT INTO table AS USER 'user123' VALUES (...)`, then verifying the record is owned by user123 (not the service account). Works independently for both User and Stream tables.

**Acceptance Scenarios**:

1. **Given** authenticated as service role, **When** executing `INSERT INTO namespace.user_table AS USER 'user123' VALUES (...)`, **Then** the record is inserted with user123 as the owner, visible only to user123 (respecting RLS)
2. **Given** authenticated as admin role, **When** executing `UPDATE namespace.stream_table AS USER 'user456' SET column = value WHERE id = X`, **Then** the record is updated as if user456 performed the action
3. **Given** authenticated as regular user role, **When** attempting `DELETE FROM user_table AS USER 'other_user' WHERE condition`, **Then** the system rejects the operation with "Permission denied: AS USER requires service/admin role"
5. **Given** authenticated as service role, **When** attempting `INSERT INTO shared_table AS USER 'user123' VALUES (...)`, **Then** the system rejects the operation with "AS USER clause not supported for Shared tables"

---

### User Story 6 - Centralized Configuration Access (Priority: P3)
5. **Given** authenticated as service role, **When** attempting `INSERT INTO shared_table AS USER 'user123' VALUES (...)`, **Then** the system rejects the operation with "AS USER clause not supported for Shared tables"

---

### User Story 5 - Centralized Configuration Access (Priority: P3)

Developers need a single consistent way to access all application configuration across the codebase, eliminating duplicate config models and file I/O scattered throughout modules.

**Why this priority**: Reduces code duplication, prevents configuration drift between components, and improves performance by eliminating redundant file reads. Essential for maintainability as configuration grows.

**Independent Test**: Can be fully tested by searching the codebase for direct config file reads (e.g., `fs::read_to_string("config.toml")`), migrating them to `AppContext.config()`, removing duplicate DTOs, and verifying all components still function correctly.

**Acceptance Scenarios**:

1. **Given** a module previously reading config from file directly, **When** refactored to use `AppContext.config().section_name`, **Then** the module accesses the same configuration values without file I/O
2. **Given** multiple modules using different config DTOs for the same settings, **When** consolidated to use AppContext config structs, **Then** all modules reference a single source of truth
3. **Given** server startup, **When** AppContext is initialized, **Then** configuration is loaded once and available to all components via shared reference
4. **Given** a configuration change requiring restart, **When** server restarts, **Then** all components automatically use updated config without code changes

---

### User Story 7 - Type-Safe Job Executor Parameters (Priority: P3)

Developers implementing job executors need type-safe parameter handling instead of manual JSON parsing, reducing boilerplate and preventing runtime errors from malformed parameters.

**Why this priority**: Improves developer experience and code quality. While not user-facing, this prevents production bugs from parameter parsing failures and makes executor code more maintainable. Lower priority because existing JSON approach works, just less elegantly.

**Independent Test**: Can be fully tested by refactoring one executor (e.g., FlushExecutor) to use generic typed parameters, verifying parameter validation at compile time, and confirming all existing job execution tests still pass.

**Acceptance Scenarios**:

1. **Given** a FlushExecutor implementation, **When** refactored to use `impl JobExecutor<FlushParams>` with generic type parameter, **Then** parameters are automatically deserialized to FlushParams struct with compile-time type safety
2. **Given** a job created with invalid parameters, **When** executor attempts to deserialize parameters, **Then** the error is caught during parameter parsing with clear type mismatch message
3. **Given** multiple executor types (Flush, Cleanup, Retention), **When** each defines its own parameter struct, **Then** each executor has type-specific parameter validation without shared JSON parsing code
4. **Given** a job executor execution, **When** parameters include nested structures or arrays, **Then** the generic deserializer handles complex types correctly without manual parsing logic

---

### Edge Cases

- What happens when UPDATE creates a new version while the original is being persisted to long-term storage?
- How does the system handle concurrent updates to the same record from different sessions?
- What happens if `_updated` timestamps are identical for two versions of the same record (nanosecond precision collision)?
- How does the system handle querying across many versions (e.g., 100+ updates to the same record)?
- What happens when a deletion (`_deleted = true`) is created while old versions are being queried?
- What happens if manifest.json becomes corrupted or out of sync with actual batch files?
- How does the system handle manifest rebuild when batch files exist but manifest is missing?
- What happens when flush operation fails after writing batch file but before updating manifest?
- How does the system ensure atomic manifest updates (read max_batch → write new file → update manifest)?
- What happens when Bloom filter indicates "maybe present" but record is not in the batch file (false positive)?
- How does AS USER validation behave when the target user_id doesn't exist in the system?
- What happens when a service account uses AS USER with a soft-deleted user account?
- How does the system handle AS USER syntax in prepared statements or batch operations?
- What happens when central configuration initialization fails during server startup due to missing settings?
- How does the system handle configuration updates - is hot-reload supported or restart required?
- What happens when a job receives parameters that fail validation against the expected structure?
- How does the system handle backward compatibility when changing job parameter schemas?
- What happens when AS USER is attempted on a Shared table (should be rejected)?

## Requirements *(mandatory)*

### Functional Requirements

#### Append-Only Update/Delete (FR-001 to FR-015)

- **FR-001**: System MUST include `_updated` timestamp column (with nanosecond precision) in all user and shared tables for version tracking
- **FR-002**: System MUST include `_deleted` boolean column in all user and shared tables for soft deletion tracking
- **FR-003**: System MUST set `_deleted = false` for all new records by default
- **FR-004**: System MUST check fast storage first when processing UPDATE operations to determine if record exists there
- **FR-005**: System MUST update records in-place when they exist only in fast storage, setting `_updated` to current timestamp with nanosecond precision
- **FR-006**: System MUST create new record versions in fast storage when updating records that have been persisted to long-term storage
- **FR-007**: System MUST preserve old versions in long-term storage unchanged when creating new versions in fast storage
- **FR-008**: System MUST persist new versions to long-term storage during the next flush operation as separate batch files
- **FR-009**: System MUST join data from all storage layers during queries and return only the version with MAX(_updated) for each record ID
- **FR-010**: System MUST support DELETE operations by setting `_deleted = true` and updating `_updated` timestamp in fast storage
- **FR-011**: System MUST filter out records where `_deleted = true` from query results (implicit WHERE _deleted = false)
- **FR-012**: System MUST handle multiple updates to the same record (e.g., 3 updates) by maintaining correct version ordering via `_updated` timestamps
- **FR-013**: System MUST ensure `_updated` timestamps are monotonically increasing to guarantee correct version resolution
- **FR-014**: System MUST validate all user and shared tables have the required `_updated` and `_deleted` columns during table creation
- **FR-015**: System MUST update `_updated` timestamp when DELETE operation sets `_deleted = true` to track deletion time

#### Manifest File for Query Optimization (FR-016 to FR-030)

- **FR-016**: System MUST create a manifest.json file per user table (per user directory) tracking batch file metadata
- **FR-017**: System MUST create a manifest.json file per shared table tracking batch file metadata
- **FR-018**: System MUST store manifest files in same directory as batch files (user_id/ for user tables, shared/ for shared tables)
- **FR-019**: System MUST include version number, generation timestamp, and max_batch counter in manifest metadata
- **FR-020**: System MUST track batch file entries with: file path (batch-{number}.parquet), min/max values for all columns, row count, byte size, schema version, status
- **FR-021**: System MUST record min_updated and max_updated (with nanosecond precision) for each batch file to enable timestamp-based filtering
- **FR-022**: System MUST read current manifest before flush operation to determine next batch number
- **FR-023**: System MUST write new batch file using naming pattern: batch-{max_batch+1}.parquet where max_batch comes from manifest
- **FR-024**: System MUST update manifest.json atomically after flush completes with new batch entry and incremented max_batch counter
- **FR-025**: System MUST use manifest during query planning to determine which batch files overlap query predicates
- **FR-026**: System MUST skip batch files when manifest proves they cannot contain matching records (e.g., _updated range outside query window)
- **FR-027**: System MUST validate manifest integrity on table access and rebuild if inconsistencies detected
- **FR-028**: System MUST handle queries when manifest is unavailable by falling back to scanning all batch files
- **FR-029**: System MUST support manifest schema evolution (version field) for future enhancements
- **FR-030**: System MUST mark batch files with status field in manifest (active, compacting, archived) for lifecycle management

#### Bloom Filter Optimization (FR-031 to FR-038)

- **FR-031**: System MUST generate Bloom filters for `id` and `_updated` columns by default when writing batch files
- **FR-032**: System MUST generate Bloom filters for all indexed columns defined in table schema
- **FR-033**: System MUST embed Bloom filters in Parquet file metadata for efficient access without reading column data
- **FR-034**: System MUST test Bloom filters during query execution before reading column data from batch files
- **FR-035**: System MUST skip batch files where Bloom filter returns "definitely not present" for equality predicates
- **FR-036**: System MUST configure Bloom filter false positive rate (default 1%) balancing space overhead vs accuracy
- **FR-037**: System MUST handle Bloom filter false positives gracefully by reading actual column data for verification
- **FR-038**: System MUST support disabling Bloom filters per column via table configuration for space-constrained scenarios

#### AS USER Syntax (FR-039 to FR-046)

- **FR-039**: System MUST support `AS USER 'user_id'` clause in INSERT statements for User and Stream tables
- **FR-040**: System MUST support `AS USER 'user_id'` clause in UPDATE statements for User and Stream tables
- **FR-041**: System MUST support `AS USER 'user_id'` clause in DELETE statements for User and Stream tables
- **FR-042**: System MUST restrict AS USER clause to service and admin roles only
- **FR-043**: System MUST validate that the target user_id exists before executing AS USER operations
- **FR-044**: System MUST apply Row-Level Security (RLS) policies as if the specified user executed the operation
- **FR-045**: System MUST audit AS USER operations with both the authenticated user (actor) and target user (subject)
- **FR-046**: System MUST reject AS USER operations on Shared tables with clear error message

#### Centralized Configuration (FR-047 to FR-052)

- **FR-047**: System MUST provide all configuration through AppContext.config() instead of direct file reads
- **FR-048**: System MUST eliminate duplicate config DTOs and use single source of truth from AppContext
- **FR-049**: System MUST load configuration once during AppContext initialization and share via Arc reference
- **FR-050**: System MUST provide type-safe access to configuration sections (database, server, jobs, auth, etc.)
- **FR-051**: System MUST validate configuration completeness at startup and fail fast with clear errors
- **FR-052**: System MUST document all configuration migration points in code comments for future reference

#### Generic Job Executor (FR-053 to FR-057)

- **FR-053**: Job execution framework MUST support type-specific parameter definitions for each job type
- **FR-054**: System MUST automatically validate and convert job parameters to job-specific structures at execution time
- **FR-055**: System MUST provide early validation of job parameter correctness before job execution
- **FR-056**: System MUST maintain compatibility with existing job storage format
- **FR-057**: System MUST handle parameter validation errors with clear messages identifying expected parameter structure

#### Test Coverage Requirements (FR-058 to FR-063)

- **FR-058**: System MUST include tests for updating records after they have been flushed to long-term storage
- **FR-059**: System MUST include tests for records updated 3 times (original + 2 updates, all flushed)
- **FR-060**: System MUST include tests verifying deleted records (_deleted = true) are not returned in query results
- **FR-061**: System MUST include tests verifying MAX(_updated) correctly resolves latest version across storage tiers
- **FR-062**: System MUST include tests for concurrent updates to the same record
- **FR-063**: System MUST include tests for querying records with _deleted flag at different storage layers

### Key Entities

- **RecordVersion**: A versioned instance of a record containing all column values plus `_updated` timestamp (nanosecond precision) for version ordering and `_deleted` boolean flag for soft deletion
- **VersionResolution**: Query-time logic that joins all storage layers and selects the version with MAX(_updated) for each record ID, filtering out records where `_deleted = true`
- **StorageLayer**: Either fast storage (RocksDB/in-memory) or long-term storage (batch Parquet files), both participating in version resolution
- **ManifestFile**: JSON file (manifest.json) per table tracking batch file metadata including max_batch counter, file paths, min/max column values, row counts, sizes, and schema versions
- **BatchFileEntry**: Manifest entry for a single batch file containing: file path (batch-{number}.parquet), min_updated/max_updated timestamps, min/max values for all columns, row_count, size_bytes, schema_version, status
- **BloomFilter**: Probabilistic data structure embedded in Parquet file metadata for `id`, `_updated`, and indexed columns enabling efficient point lookup elimination
- **ImpersonationContext**: Execution context holding both authenticated user (actor) and target user (subject) for audit trail and authorization enforcement in AS USER operations (User/Stream tables only)
- **ManifestService**: Centralized service component responsible for all manifest file operations (create, update, rebuild, validate) ensuring single source of truth for batch file metadata management
- **TypedJobParameters**: Structured parameter container enabling validation and type checking for job-specific configurations

## Clarifications

### Concurrent Update Conflict Resolution (added 2025-11-10)

**Question**: How are concurrent updates handled when a record is being flushed to long-term storage?

**Answer**: Concurrent updates during flush are safe because flush operations work on a snapshot of fast storage while the live fast storage continues accepting writes. Conflict resolution:

1. **Flush Process**: Creates snapshot of fast storage at T0, writes snapshot to batch file, continues normal operation
2. **Concurrent Updates**: Any updates after T0 write to live fast storage (not the snapshot), creating new versions with current `_updated` timestamp
3. **Version Ordering**: Query-time resolution uses MAX(_updated) across all layers - newer updates (after snapshot) automatically supersede flushed versions
4. **No Locking Required**: Fast storage remains fully available during flush - updates are never blocked or delayed

Example timeline:
- T0: Flush begins, snapshots record {id:123, status:'pending', _updated:T0}
- T1: User updates to status='active', creates version with _updated:T1 in live fast storage
- T2: Flush completes, writes snapshot to batch-0005.parquet
- Query at T3: Returns status='active' (MAX(_updated) = T1 > T0)

### Nanosecond Timestamp Collision Handling (added 2025-11-10)

**Question**: What happens if two versions of the same record have identical `_updated` timestamps (nanosecond precision collision)?

**Answer**: Timestamp collisions are handled with a secondary tie-breaker:

1. **Primary Ordering**: MAX(_updated) timestamp (nanosecond precision)
2. **Collision Tie-Breaker**: If multiple versions have identical _updated timestamps, use storage layer priority:
   - Fast storage version > Long-term storage version (newer always wins)
   - Within same layer: undefined behavior (system assumes nanosecond precision prevents this)
3. **Prevention**: Fast storage UPDATE operations add 1 nanosecond to previous MAX(_updated) if current_time() returns same value
4. **Guarantee**: Each UPDATE increments _updated by at least 1ns, ensuring strict ordering even at microsecond update rates

Example:
- Version A: _updated = 1699564800.123456789 (in batch file)
- Version B: _updated = 1699564800.123456789 (in fast storage, collision detected)
- Version B adjusted to: 1699564800.123456790 (guaranteed unique)

### Manifest Corruption Recovery Strategy (added 2025-11-10)

**Question**: How does the system recover from manifest.json corruption or inconsistencies with batch files?

**Answer**: Manifest corruption recovery follows a scan-and-rebuild approach with centralized management:

1. **Centralized Manifest Service**: Single ManifestService component within flush operations handles ALL manifest operations (create, update, rebuild, validate) - no scattered manifest logic across codebase
2. **Detection**: On table access, validate manifest exists, has valid JSON, and max_batch matches highest batch-{N}.parquet file number
3. **Rebuild Trigger**: If validation fails, initiate background rebuild while serving queries in degraded mode
4. **Degraded Mode**: Queries fallback to scanning all batch files directly (no manifest optimization) while rebuild runs
5. **Rebuild Process** (executed by ManifestService):
   - Scan directory for all batch-*.parquet files, extract max N → new max_batch
   - For each batch file: read Parquet metadata (footer only, not full data) to extract min/max values, row counts, schema
   - Generate new manifest.json with reconstructed metadata
   - Atomic rename: manifest.json.tmp → manifest.json
6. **Flush Job Integration**: Each flush operation completion triggers ManifestService to:
   - Validate current manifest exists (rebuild if missing/corrupt)
   - Update manifest with new batch entry
   - Ensure manifest is consistent before flush job completes
7. **Availability**: Table remains readable during entire rebuild (degraded performance, not downtime)
8. **Duration**: Manifest rebuild completes in O(number_of_batch_files × parquet_footer_read_time), typically <1 second per 100 files

**Architecture Constraint**: ManifestService is the ONLY component allowed to read/write manifest files - enforces single source of truth for manifest management.

### AS USER Validation with Soft-Deleted Users (added 2025-11-10)

**Question**: Should AS USER operations work with soft-deleted user accounts?

**Answer**: AS USER validation treats soft-deleted users as non-existent:

1. **Validation Logic**: Check both user_id existence AND active status (deleted_at IS NULL)
2. **Soft-Deleted Users**: Rejected with same error as non-existent users: 'Invalid user_id for AS USER operation'
3. **Rationale**: Soft-deleted users should be treated as if they don't exist - prevents services from creating data in deleted user contexts
4. **Security**: Generic error message prevents information leakage about which user_ids exist in the system
5. **Exception**: System-level jobs (user cleanup, data migration) bypass AS USER and operate directly on user stores

Example rejections:
- `INSERT INTO app.messages AS USER 'deleted_user123'` → Error: 'Invalid user_id for AS USER operation'
- `INSERT INTO app.messages AS USER 'never_existed'` → Error: 'Invalid user_id for AS USER operation' (same message)

### Service-Level Subscription Design (added 2025-11-10)

**Question**: How should service-level subscriptions handle backpressure and event volume at scale?

**Status**: DEFERRED - Requires deeper design exploration

**Context**: Initial design proposed `SUBSCRIBE TO ALL` syntax for service-level monitoring of all user changes. However, this approach has significant backpressure challenges when subscribers can't keep up with event rates (10K events/sec generated, 1K/sec consumed).

**Alternative Approach Under Consideration**: 
Instead of real-time subscription connections, use materialized stream tables:
- `CREATE STREAM TABLE admin.all_messages AS SELECT * FROM user_tables.messages` 
- Stream table acts as durable event log with built-in backpressure via storage
- Services read from stream table at their own pace (pull model vs push)
- Enables batch processing, replay, and better resource isolation

**Next Steps**: 
1. Design stream table federation/aggregation semantics
2. Define materialization triggers and update propagation
3. Specify query syntax for cross-user table aggregation
4. Determine resource limits and quotas

**Impact**: User Story 5 (Service-Level Subscriptions) removed from this spec pending design completion. Will be addressed in separate feature specification.

---

## Assumptions *(optional)*

- **Append-Only Architecture**: System uses append-only writes where updates create new versions rather than modifying existing records in-place
- **System Columns**: All user and shared tables already include `_updated` (timestamp with nanosecond precision) and `_deleted` (boolean) columns
- **Storage Tiers**: System uses two-tier storage (fast storage for recent writes + long-term storage for persisted batch files)
- **Flush Process**: Records are periodically moved from fast storage to long-term storage, with new versions flushed to batch-{number}.parquet files
- **Batch Numbering**: Batch files use sequential numbering controlled by manifest max_batch counter (not timestamps)
- **User Isolation**: User tables are stored in per-user directories (/data/{namespace}/{table}/user_{id}/) enabling per-user manifest files
- **Shared Table Layout**: Shared tables use single shared/ directory (/data/{namespace}/{table}/shared/) with single manifest file
- **Role-Based Authorization**: System has existing role hierarchy (user < service < admin < system) for permission checks
- **Configuration Format**: Single configuration file loaded at startup containing all system settings
- **Job Framework**: Existing job execution system supports parameter passing and error handling
- **Audit Requirements**: All impersonated operations require full audit trail for compliance
- **Parquet Support**: Parquet library supports Bloom filter metadata embedding and column statistics extraction

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: UPDATE operations on persisted records complete without reading long-term storage files (append-only write to fast storage with nanosecond-precision `_updated`)
- **SC-002**: DELETE operations execute in under 50ms by setting `_deleted = true` and updating `_updated` in fast storage only
- **SC-003**: Queries returning latest versions correctly resolve MAX(_updated) across all storage layers within 2x baseline query time
- **SC-004**: System handles records with 10+ versions without degradation in query performance
- **SC-005**: Manifest-based query optimization reduces unnecessary batch file scans by 80%+ for queries with timestamp range predicates
- **SC-006**: Bloom filter optimization reduces I/O by 90%+ for point queries (WHERE id = X) on tables with 100+ batch files
- **SC-007**: Flush operations correctly increment max_batch counter and generate sequential batch file names (batch-0001.parquet, batch-0002.parquet, etc.)
- **SC-008**: Manifest files are atomically updated after flush completion with zero data loss on crash during manifest write
- **SC-009**: Impersonated operations are audited with both actor and subject user IDs in 100% of executions
- **SC-010**: Impersonation permission checks reject unauthorized users in under 10ms
- **SC-011**: Centralized configuration access eliminates all direct configuration file reads outside initialization code
- **SC-012**: Job parameter validation failures provide actionable error messages identifying expected structure within 100ms
- **SC-013**: Type-safe job parameter implementation reduces parameter handling code by 50%+ lines per job type
- **SC-014**: Test suite achieves 100% coverage for post-flush updates, multi-version records, and _deleted flag handling
