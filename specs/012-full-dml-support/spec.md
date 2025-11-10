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

1. **Given** a record exists only in fast storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system updates the record in fast storage and increments the version timestamp
2. **Given** a record has been persisted to long-term storage, **When** user executes `UPDATE table SET column = value WHERE id = X`, **Then** the system creates a new version in fast storage with updated timestamp, leaving the old version in long-term storage unchanged
3. **Given** a record has been updated 3 times (original + 2 updates all persisted), **When** user queries the record, **Then** the system returns only the version with MAX(_updated) timestamp by joining all storage layers
4. **Given** a persisted record, **When** user executes `DELETE FROM table WHERE id = X`, **Then** the system creates a deletion marker in fast storage with deletion timestamp, and subsequent queries exclude this record
5. **Given** a deleted record (with deletion marker), **When** user queries the table, **Then** the record is not returned even though historical versions exist in long-term storage

---

### User Story 2 - Metadata Index for Query Optimization (Priority: P2)

System administrators need efficient query execution on tables with many persisted storage files. A metadata index tracks which files contain relevant data, enabling the query planner to skip unnecessary file scans.

**Why this priority**: Performance optimization for large tables - without metadata indexes, queries must scan all persisted files even when data is concentrated in specific files. This becomes critical as tables grow beyond 100+ storage files. Secondary to core UPDATE/DELETE functionality.

**Independent Test**: Can be fully tested by creating a table, flushing data to multiple storage files, then querying with selective WHERE clauses. Success is verified by observing that the query planner only reads relevant files based on metadata index (column value ranges, row counts, file paths).

**Acceptance Scenarios**:

1. **Given** a table with data flushed to 10 storage files, **When** query includes `WHERE timestamp BETWEEN 'start' AND 'end'`, **Then** the system uses metadata index to identify only files containing records in that range and skips others
2. **Given** a metadata index tracking min/max values per column per file, **When** query includes `WHERE id = 12345`, **Then** the system scans only files where 12345 falls within the indexed ID range
3. **Given** a query targeting only fast storage data, **When** metadata indicates no relevant data in long-term storage, **Then** the system skips all persisted file reads
4. **Given** a table with 100+ storage files, **When** metadata index becomes corrupted, **Then** the system detects inconsistency and rebuilds the index from actual file metadata

---

### User Story 3 - Execute DML as Different User (AS USER) (Priority: P2)

A service account or admin needs to insert, update, or delete records on behalf of a specific user without switching authentication context. This enables system operations like message routing, AI-generated content, and cross-user notifications.

**Why this priority**: Critical for multi-tenant systems where services need to act on behalf of users (e.g., chat systems, notification services, AI assistants). Without this, services would need separate authentication per user, creating security and performance overhead.

**Independent Test**: Can be fully tested by authenticating as a service/admin user, executing `INSERT INTO table AS USER 'user123' VALUES (...)`, then verifying the record is owned by user123 (not the service account). Works independently for both User and Stream tables.

**Acceptance Scenarios**:

1. **Given** authenticated as service role, **When** executing `INSERT INTO namespace.table AS USER 'user123' VALUES (...)`, **Then** the record is inserted with user123 as the owner, visible only to user123 (respecting RLS)
2. **Given** authenticated as admin role, **When** executing `UPDATE namespace.stream_table AS USER 'user456' SET column = value WHERE id = X`, **Then** the record is updated as if user456 performed the action
3. **Given** authenticated as regular user role, **When** attempting `DELETE FROM table AS USER 'other_user' WHERE condition`, **Then** the system rejects the operation with "Permission denied: AS USER requires service/admin role"
4. **Given** authenticated as service role, **When** executing DML without AS USER clause on a user-scoped table, **Then** the operation fails with "User table requires explicit AS USER clause for service/admin roles"

---

### User Story 3 - Execute DML as Different User (AS USER) (Priority: P2)

A service account or admin needs to insert, update, or delete records on behalf of a specific user without switching authentication context. This enables system operations like message routing, AI-generated content, and cross-user notifications.

**Why this priority**: Critical for multi-tenant systems where services need to act on behalf of users (e.g., chat systems, notification services, AI assistants). Without this, services would need separate authentication per user, creating security and performance overhead.

**Independent Test**: Can be fully tested by authenticating as a service/admin user, executing `INSERT INTO table AS USER 'user123' VALUES (...)`, then verifying the record is owned by user123 (not the service account). Works independently for both User and Stream tables.

**Acceptance Scenarios**:

1. **Given** authenticated as service role, **When** executing `INSERT INTO namespace.table AS USER 'user123' VALUES (...)`, **Then** the record is inserted with user123 as the owner, visible only to user123 (respecting RLS)
2. **Given** authenticated as admin role, **When** executing `UPDATE namespace.stream_table AS USER 'user456' SET column = value WHERE id = X`, **Then** the record is updated as if user456 performed the action
3. **Given** authenticated as regular user role, **When** attempting `DELETE FROM table AS USER 'other_user' WHERE condition`, **Then** the system rejects the operation with "Permission denied: AS USER requires service/admin role"
4. **Given** authenticated as service role, **When** executing DML without AS USER clause on a user-scoped table, **Then** the operation fails with "User table requires explicit AS USER clause for service/admin roles"

---

### User Story 4 - Centralized Configuration Access (Priority: P3)

Developers need a single consistent way to access all application configuration across the codebase, eliminating duplicate config models and file I/O scattered throughout modules.

**Why this priority**: Reduces code duplication, prevents configuration drift between components, and improves performance by eliminating redundant file reads. Essential for maintainability as configuration grows.

**Independent Test**: Can be fully tested by searching the codebase for direct config file reads (e.g., `fs::read_to_string("config.toml")`), migrating them to `AppContext.config()`, removing duplicate DTOs, and verifying all components still function correctly.

**Acceptance Scenarios**:

1. **Given** a module previously reading config from file directly, **When** refactored to use `AppContext.config().section_name`, **Then** the module accesses the same configuration values without file I/O
2. **Given** multiple modules using different config DTOs for the same settings, **When** consolidated to use AppContext config structs, **Then** all modules reference a single source of truth
3. **Given** server startup, **When** AppContext is initialized, **Then** configuration is loaded once and available to all components via shared reference
4. **Given** a configuration change requiring restart, **When** server restarts, **Then** all components automatically use updated config without code changes

---

### User Story 4 - Centralized Configuration Access (Priority: P3)

Developers need a single consistent way to access all application configuration across the codebase, eliminating duplicate config models and file I/O scattered throughout modules.

**Why this priority**: Reduces code duplication, prevents configuration drift between components, and improves performance by eliminating redundant file reads. Essential for maintainability as configuration grows. Lower priority as it's internal refactoring.

**Independent Test**: Can be fully tested by searching the codebase for direct config file reads (e.g., `fs::read_to_string("config.toml")`), migrating them to `AppContext.config()`, removing duplicate DTOs, and verifying all components still function correctly.

**Acceptance Scenarios**:

1. **Given** a module previously reading config from file directly, **When** refactored to use `AppContext.config().section_name`, **Then** the module accesses the same configuration values without file I/O
2. **Given** multiple modules using different config DTOs for the same settings, **When** consolidated to use AppContext config structs, **Then** all modules reference a single source of truth
3. **Given** server startup, **When** AppContext is initialized, **Then** configuration is loaded once and available to all components via shared reference
4. **Given** a configuration change requiring restart, **When** server restarts, **Then** all components automatically use updated config without code changes

---

### User Story 5 - Type-Safe Job Executor Parameters (Priority: P3)

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
- What happens if `_updated` timestamps are identical for two versions of the same record?
- How does the system handle querying across many versions (e.g., 100+ updates to the same record)?
- What happens when a deletion marker is created while old versions are being queried?
- How does AS USER validation behave when the target user_id doesn't exist in the system?
- What happens when a service account uses AS USER with a soft-deleted user account?
- How does the system handle AS USER syntax in prepared statements or batch operations?
- What happens when central configuration initialization fails during server startup due to missing settings?
- How does the system handle configuration updates - is hot-reload supported or restart required?
- What happens when a job receives parameters that fail validation against the expected structure?
- How does the system handle backward compatibility when changing job parameter schemas?

## Requirements *(mandatory)*

### Functional Requirements

#### Append-Only Update/Delete (FR-001 to FR-012)

- **FR-001**: System MUST include `_updated` timestamp column in all user and shared tables for version tracking
- **FR-002**: System MUST check fast storage first when processing UPDATE operations to determine if record exists there
- **FR-003**: System MUST update records in-place when they exist only in fast storage, incrementing the `_updated` timestamp
- **FR-004**: System MUST create new record versions in fast storage when updating records that have been persisted to long-term storage
- **FR-005**: System MUST preserve old versions in long-term storage unchanged when creating new versions in fast storage
- **FR-006**: System MUST persist new versions to long-term storage during the next flush operation as separate files
- **FR-007**: System MUST join data from all storage layers during queries and return only the version with MAX(_updated) for each record ID
- **FR-008**: System MUST support DELETE operations by creating deletion markers in fast storage with `_deleted_at` timestamp
- **FR-009**: System MUST exclude records with deletion markers from query results regardless of historical versions in storage
- **FR-010**: System MUST handle multiple updates to the same record (e.g., 3 updates) by maintaining correct version ordering via timestamps
- **FR-011**: System MUST ensure `_updated` timestamps are monotonically increasing to guarantee correct version resolution
- **FR-012**: System MUST validate all user and shared tables have the required `_updated` column during table creation

#### Metadata Index for Query Optimization (FR-013 to FR-020)

- **FR-013**: System MUST create a metadata index for each user table tracking persistent storage file locations and statistics
- **FR-014**: System MUST record min/max values for each column in each storage file in the metadata index
- **FR-015**: System MUST update the metadata index whenever records are flushed from fast storage to long-term storage
- **FR-016**: System MUST use the metadata index during query planning to determine which storage files to scan
- **FR-017**: System MUST skip storage files when metadata proves they cannot contain matching records for the query
- **FR-018**: System MUST validate metadata index integrity on table access and rebuild if inconsistencies detected
- **FR-019**: System MUST track row counts and file sizes in the metadata index for storage management
- **FR-020**: System MUST handle queries when metadata index is unavailable by falling back to full file scans

#### AS USER Syntax (FR-021 to FR-028)

- **FR-021**: System MUST support `AS USER 'user_id'` clause in INSERT statements for User and Stream tables
- **FR-022**: System MUST support `AS USER 'user_id'` clause in UPDATE statements for User and Stream tables
- **FR-023**: System MUST support `AS USER 'user_id'` clause in DELETE statements for User and Stream tables
- **FR-024**: System MUST restrict AS USER clause to service and admin roles only
- **FR-025**: System MUST validate that the target user_id exists before executing AS USER operations
- **FR-026**: System MUST apply Row-Level Security (RLS) policies as if the specified user executed the operation
- **FR-027**: System MUST audit AS USER operations with both the authenticated user (actor) and target user (subject)
- **FR-028**: System MUST reject AS USER operations on Shared tables with clear error message

#### Centralized Configuration (FR-029 to FR-034)

- **FR-029**: System MUST provide all configuration through AppContext.config() instead of direct file reads
- **FR-030**: System MUST eliminate duplicate config DTOs and use single source of truth from AppContext
- **FR-031**: System MUST load configuration once during AppContext initialization and share via Arc reference
- **FR-032**: System MUST provide type-safe access to configuration sections (database, server, jobs, auth, etc.)
- **FR-033**: System MUST validate configuration completeness at startup and fail fast with clear errors
- **FR-034**: System MUST document all configuration migration points in code comments for future reference

#### Generic Job Executor (FR-035 to FR-039)

- **FR-035**: Job execution framework MUST support type-specific parameter definitions for each job type
- **FR-036**: System MUST automatically validate and convert job parameters to job-specific structures at execution time
- **FR-037**: System MUST provide early validation of job parameter correctness before job execution
- **FR-038**: System MUST maintain compatibility with existing job storage format
- **FR-039**: System MUST handle parameter validation errors with clear messages identifying expected parameter structure

#### Test Coverage Requirements (FR-040 to FR-045)

- **FR-040**: System MUST include tests for updating records after they have been flushed to long-term storage
- **FR-041**: System MUST include tests for records updated 3 times (original + 2 updates, all flushed)
- **FR-042**: System MUST include tests verifying deleted records are not returned in query results
- **FR-043**: System MUST include tests verifying MAX(_updated) correctly resolves latest version across storage tiers
- **FR-044**: System MUST include tests for concurrent updates to the same record
- **FR-045**: System MUST include tests for querying records with deletion markers at different storage layers

### Key Entities

- **RecordVersion**: A versioned instance of a record containing all column values plus `_updated` timestamp for version ordering and `_deleted_at` timestamp for soft deletes
- **VersionResolution**: Query-time logic that joins all storage layers and selects the version with MAX(_updated) for each record ID, excluding records with deletion markers
- **StorageLayer**: Either fast storage (in-memory/recent writes) or long-term storage (persisted columnar files), both participating in version resolution
- **MetadataIndex**: Per-table index tracking storage file locations, min/max column values, row counts, and file sizes for query optimization
- **FileStatistics**: Column-level statistics (min/max values, null counts) for a single storage file enabling predicate pushdown
- **ImpersonationContext**: Execution context holding both authenticated user (actor) and target user (subject) for audit trail and authorization enforcement
- **TypedJobParameters**: Structured parameter container enabling validation and type checking for job-specific configurations

## Assumptions *(optional)*

- **Append-Only Architecture**: System uses append-only writes where updates create new versions rather than modifying existing records in-place
- **Version Tracking**: All user and shared tables already include `_updated` column for version timestamp tracking
- **Storage Tiers**: System uses two-tier storage (fast storage for recent writes + long-term storage for persisted data)
- **Flush Process**: Records are periodically moved from fast storage to long-term storage, with new versions flushed as separate files
- **User Isolation**: User tables are stored separately per user, enabling per-user version tracking
- **Role-Based Authorization**: System has existing role hierarchy (user < service < admin < system) for permission checks
- **Configuration Format**: Single configuration file loaded at startup containing all system settings
- **Job Framework**: Existing job execution system supports parameter passing and error handling
- **Audit Requirements**: All impersonated operations require full audit trail for compliance

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: UPDATE operations on persisted records complete without reading long-term storage files (append-only write to fast storage)
- **SC-002**: DELETE operations execute in under 50ms by creating deletion markers in fast storage only
- **SC-003**: Queries returning latest versions correctly resolve MAX(_updated) across all storage layers within 2x baseline query time
- **SC-004**: System handles records with 10+ versions without degradation in query performance
- **SC-005**: Metadata index reduces unnecessary file scans by 80%+ for queries with selective WHERE clauses
- **SC-006**: Queries on tables with 100+ storage files complete within 3x time of scanning only relevant files
- **SC-007**: Impersonated operations are audited with both actor and subject user IDs in 100% of executions
- **SC-008**: Impersonation permission checks reject unauthorized users in under 10ms
- **SC-009**: Centralized configuration access eliminates all direct configuration file reads outside initialization code
- **SC-010**: Job parameter validation failures provide actionable error messages identifying expected structure within 100ms
- **SC-011**: Type-safe job parameter implementation reduces parameter handling code by 50%+ lines per job type
- **SC-012**: Test suite achieves 100% coverage for post-flush updates, multi-version records, and deletion marker handling
