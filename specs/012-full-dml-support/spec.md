# Feature Specification: Full DML Support

**Feature Branch**: `012-full-dml-support`  
**Created**: 2025-11-10  
**Status**: Draft  
**Input**: User description: "Support Update/Delete of flushed tables with manifest files, AS USER support for DML statements, centralized config usage via AppContext, and generic JobExecutor parameters"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Update and Delete Persisted Table Records (Priority: P1)

A database administrator needs to update or delete specific records from a user table that has been persisted to long-term storage. The system should efficiently locate the relevant storage files using metadata indexes without scanning unnecessary files.

**Why this priority**: Core DML functionality - without UPDATE/DELETE on persisted tables, users cannot modify historical data, making the database read-only for persisted records. This is essential for data correction, GDPR compliance (right to erasure), and normal database operations.

**Independent Test**: Can be fully tested by creating a user table, inserting records, persisting to long-term storage, then executing UPDATE/DELETE statements. Success is verified by querying the table and confirming the modifications persisted correctly across both storage tiers.

**Acceptance Scenarios**:

1. **Given** a user table with records in long-term storage, **When** user executes `UPDATE table SET column = value WHERE condition`, **Then** the system reads the metadata index, identifies relevant storage files, updates matching records, and persists changes correctly
2. **Given** a user table with records split between fast storage (recent) and long-term storage (persisted), **When** user executes `DELETE FROM table WHERE condition`, **Then** the system deletes records from both storage tiers and updates the metadata index accordingly
3. **Given** a user table with only fast-storage records (no persisted data), **When** user executes UPDATE/DELETE, **Then** the system skips long-term storage access entirely based on metadata
4. **Given** a persisted table with indexed columns, **When** user executes `UPDATE table SET indexed_col = new_value WHERE id = X`, **Then** the system updates both the data and the index metadata

---

### User Story 2 - Execute DML as Different User (AS USER) (Priority: P2)

A service account or admin needs to insert, update, or delete records on behalf of a specific user without switching authentication context. This enables system operations like message routing, AI-generated content, and cross-user notifications.

**Why this priority**: Critical for multi-tenant systems where services need to act on behalf of users (e.g., chat systems, notification services, AI assistants). Without this, services would need separate authentication per user, creating security and performance overhead.

**Independent Test**: Can be fully tested by authenticating as a service/admin user, executing `INSERT INTO table AS USER 'user123' VALUES (...)`, then verifying the record is owned by user123 (not the service account). Works independently for both User and Stream tables.

**Acceptance Scenarios**:

1. **Given** authenticated as service role, **When** executing `INSERT INTO namespace.table AS USER 'user123' VALUES (...)`, **Then** the record is inserted with user123 as the owner, visible only to user123 (respecting RLS)
2. **Given** authenticated as admin role, **When** executing `UPDATE namespace.stream_table AS USER 'user456' SET column = value WHERE id = X`, **Then** the record is updated as if user456 performed the action
3. **Given** authenticated as regular user role, **When** attempting `DELETE FROM table AS USER 'other_user' WHERE condition`, **Then** the system rejects the operation with "Permission denied: AS USER requires service/admin role"
4. **Given** authenticated as service role, **When** executing DML without AS USER clause on a user-scoped table, **Then** the operation fails with "User table requires explicit AS USER clause for service/admin roles"

---

### User Story 3 - Centralized Configuration Access (Priority: P2)

Developers need a single consistent way to access all application configuration across the codebase, eliminating duplicate config models and file I/O scattered throughout modules.

**Why this priority**: Reduces code duplication, prevents configuration drift between components, and improves performance by eliminating redundant file reads. Essential for maintainability as configuration grows.

**Independent Test**: Can be fully tested by searching the codebase for direct config file reads (e.g., `fs::read_to_string("config.toml")`), migrating them to `AppContext.config()`, removing duplicate DTOs, and verifying all components still function correctly.

**Acceptance Scenarios**:

1. **Given** a module previously reading config from file directly, **When** refactored to use `AppContext.config().section_name`, **Then** the module accesses the same configuration values without file I/O
2. **Given** multiple modules using different config DTOs for the same settings, **When** consolidated to use AppContext config structs, **Then** all modules reference a single source of truth
3. **Given** server startup, **When** AppContext is initialized, **Then** configuration is loaded once and available to all components via shared reference
4. **Given** a configuration change requiring restart, **When** server restarts, **Then** all components automatically use updated config without code changes

---

### User Story 4 - Type-Safe Job Executor Parameters (Priority: P3)

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

- What happens when UPDATE/DELETE targets a storage file that is currently being compacted or reorganized?
- How does the system handle concurrent updates to the same persisted record from different sessions?
- What happens if a metadata index becomes corrupted or out of sync with actual storage files?
- How does AS USER validation behave when the target user_id doesn't exist in the system?
- What happens when a service account uses AS USER with a soft-deleted user account?
- How does the system handle AS USER syntax in prepared statements or batch operations?
- What happens when central configuration initialization fails during server startup due to missing settings?
- How does the system handle configuration updates - is hot-reload supported or restart required?
- What happens when a job receives parameters that fail validation against the expected structure?
- How does the system handle backward compatibility when changing job parameter schemas?

## Requirements *(mandatory)*

### Functional Requirements

#### Manifest-Based Update/Delete (FR-001 to FR-010)

- **FR-001**: System MUST create a metadata index for each user table tracking persistent storage file locations, row ranges, and column index information
- **FR-002**: System MUST update the metadata index whenever records are moved from fast storage to persistent storage
- **FR-003**: System MUST use the metadata index to determine which persistent files contain records matching UPDATE/DELETE conditions
- **FR-004**: System MUST skip persistent file reads when metadata indicates all matching records are in fast storage only
- **FR-005**: System MUST maintain column index metadata (column names, value ranges, file offsets)
- **FR-006**: System MUST support UPDATE operations on persisted records through record versioning
- **FR-007**: System MUST support DELETE operations on persisted records by marking records as deleted with deletion timestamp
- **FR-008**: System MUST track record modification timestamps per user for incremental change detection
- **FR-009**: System MUST handle concurrent modifications to persisted data with appropriate conflict resolution
- **FR-010**: System MUST validate metadata index integrity on table access and repair if corrupted

#### AS USER Syntax (FR-011 to FR-018)

- **FR-011**: System MUST support `AS USER 'user_id'` clause in INSERT statements for User and Stream tables
- **FR-012**: System MUST support `AS USER 'user_id'` clause in UPDATE statements for User and Stream tables
- **FR-013**: System MUST support `AS USER 'user_id'` clause in DELETE statements for User and Stream tables
- **FR-014**: System MUST restrict AS USER clause to service and admin roles only
- **FR-015**: System MUST validate that the target user_id exists before executing AS USER operations
- **FR-016**: System MUST apply Row-Level Security (RLS) policies as if the specified user executed the operation
- **FR-017**: System MUST audit AS USER operations with both the authenticated user (actor) and target user (subject)
- **FR-018**: System MUST reject AS USER operations on Shared tables with clear error message

#### Centralized Configuration (FR-019 to FR-024)

- **FR-019**: System MUST provide all configuration through AppContext.config() instead of direct file reads
- **FR-020**: System MUST eliminate duplicate config DTOs and use single source of truth from AppContext
- **FR-021**: System MUST load configuration once during AppContext initialization and share via Arc reference
- **FR-022**: System MUST provide type-safe access to configuration sections (database, server, jobs, auth, etc.)
- **FR-023**: System MUST validate configuration completeness at startup and fail fast with clear errors
- **FR-024**: System MUST document all configuration migration points in code comments for future reference

#### Generic Job Executor (FR-025 to FR-029)

- **FR-025**: Job execution framework MUST support type-specific parameter definitions for each job type
- **FR-026**: System MUST automatically validate and convert job parameters to job-specific structures at execution time
- **FR-027**: System MUST provide early validation of job parameter correctness before job execution
- **FR-028**: System MUST maintain compatibility with existing job storage format
- **FR-029**: System MUST handle parameter validation errors with clear messages identifying expected parameter structure

### Key Entities

- **TableMetadata**: Structure tracking persistent storage files for a table, including file locations, row ranges, column statistics, index positions, and synchronization timestamps
- **StorageFileMetadata**: Individual file entry containing file identifier, row count, size, column value ranges, and creation timestamp
- **ChangeJournal**: Per-user tracking of record modification timestamps for incremental change detection and optimization
- **ImpersonationContext**: Execution context holding both authenticated user (actor) and target user (subject) for audit trail and authorization enforcement
- **TypedJobParameters**: Structured parameter container enabling validation and type checking for job-specific configurations

## Assumptions *(optional)*

- **Storage Architecture**: System uses two-tier storage (fast in-memory/on-disk cache + persistent columnar files)
- **User Isolation**: User tables are stored separately per user, enabling per-user metadata tracking
- **Role-Based Authorization**: System has existing role hierarchy (user < service < admin < system) for permission checks
- **Configuration Format**: Single configuration file loaded at startup containing all system settings
- **Job Framework**: Existing job execution system supports parameter passing and error handling
- **Audit Requirements**: All impersonated operations require full audit trail for compliance

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: UPDATE/DELETE operations on tables with 1M+ persisted records complete in under 5 seconds when metadata indicates target rows are in specific storage files
- **SC-002**: UPDATE/DELETE operations on tables with only fast-storage records execute without accessing persistent storage files
- **SC-003**: Impersonated operations are audited with both actor and subject user IDs in 100% of executions
- **SC-004**: Impersonation permission checks reject unauthorized users in under 10ms
- **SC-005**: Centralized configuration access eliminates all direct configuration file reads outside initialization code
- **SC-006**: Job parameter validation failures provide actionable error messages identifying expected structure within 100ms
- **SC-007**: Metadata-based query optimization reduces unnecessary storage file scans by 80%+ for targeted UPDATE/DELETE operations
- **SC-008**: Type-safe job parameter implementation reduces parameter handling code by 50%+ lines per job type
