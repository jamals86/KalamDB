# Feature Specification: System Improvements and Performance Optimization

**Feature Branch**: `004-system-improvements-and`  
**Created**: October 21, 2025  
**Status**: Draft  
**Input**: User description: "System Improvements and Query Optimization - Parametrized queries, automatic flushing, caching, and architectural refactoring"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Parametrized Query Execution with Caching (Priority: P1)

Database users need to execute queries efficiently with dynamic parameters while maintaining security and performance. The system should compile queries once and reuse the execution plan for subsequent calls with different parameter values.

**Why this priority**: Query compilation is expensive. Eliminating repeated compilation for the same query structure will significantly improve response times and reduce CPU usage. This is a fundamental performance optimization that benefits all database operations.

**Independent Test**: Can be fully tested by submitting a parametrized query via the `/api/sql` endpoint, verifying it executes correctly with parameters, then submitting the same query with different parameters and confirming the cached execution plan is used (observable through faster execution time and query plan inspection).

**Acceptance Scenarios**:

1. **Given** a user has a SQL query with dynamic values, **When** they submit `{ "sql": "SELECT * FROM messages WHERE user_id = $1 AND created_at > $2", "params": ["user123", "2025-01-01"] }` to `/api/sql`, **Then** the query executes successfully and returns filtered results
2. **Given** a parametrized query has been executed once, **When** the same query structure is submitted again with different parameter values, **Then** the cached execution plan is used without recompilation
3. **Given** a query with invalid parameter count, **When** submitted to the API, **Then** the system returns a clear error message indicating parameter mismatch
4. **Given** query results are being returned, **When** the query execution configuration enables timing, **Then** the response includes the query execution duration

---

### User Story 2 - Automatic Table Flushing with Scheduled Jobs (Priority: P1)

Database administrators need user table data automatically persisted to storage at configured intervals without manual intervention. The system should group data by user, apply configured sharding strategies, and write to organized storage paths.

**Why this priority**: Data durability is critical. Without automatic flushing, data remains only in memory/buffer and is vulnerable to loss. This is essential for production readiness and data reliability.

**Independent Test**: Can be fully tested by creating a table with flush configuration, inserting data from multiple users, waiting for the scheduled flush interval, then verifying Parquet files are created in the correct storage locations organized by user and shard.

**Acceptance Scenarios**:

1. **Given** a table is created with flush interval configuration, **When** the scheduled flush time arrives and data exists in the buffer, **Then** a flush job initiates automatically
2. **Given** multiple users have data in a table buffer, **When** automatic flush executes, **Then** data is grouped by user_id and written to separate storage locations
3. **Given** flush storage locations are configured with path templates, **When** data is flushed, **Then** files are written following the template pattern (e.g., `{storageLocation}/{namespace}/users/{userId}/{tableName}/`)
4. **Given** a sharding strategy is configured, **When** data is flushed, **Then** data is distributed across shards according to the configured function
5. **Given** flush configuration specifies separate paths for user tables vs shared tables, **When** flush executes for each table type, **Then** data is written to the appropriate directory structure

---

### User Story 3 - Manual Table Flushing via SQL Command (Priority: P2)

Database administrators need to manually trigger immediate table flushing for maintenance, backup, or server shutdown scenarios. The command should provide control over which tables to flush and confirmation of the operation.

**Why this priority**: Manual control is necessary for planned maintenance and backup operations. While automatic flushing handles routine operations, administrators need the ability to force immediate persistence.

**Independent Test**: Can be fully tested by executing a `FLUSH TABLE` SQL command via the API, then verifying the specified table's buffered data is immediately written to storage and the buffer is cleared.

**Acceptance Scenarios**:

1. **Given** a user table has buffered data, **When** administrator executes `FLUSH TABLE namespace.table_name`, **Then** all buffered data is immediately written to storage
2. **Given** multiple tables exist, **When** administrator executes `FLUSH ALL TABLES`, **Then** all tables with buffered data are flushed sequentially
3. **Given** a flush operation completes, **When** the SQL command returns, **Then** the response includes the number of records flushed and the target storage location
4. **Given** the server is shutting down, **When** the shutdown sequence initiates, **Then** automatic flush of all tables executes before the process terminates

---

### User Story 4 - Session-Level Table Registration Caching (Priority: P2)

Database users who repeatedly query their own tables should experience faster query execution through intelligent table registration caching. The system should maintain frequently-accessed table registrations in memory and automatically evict unused registrations.

**Why this priority**: Current architecture registers/unregisters tables per query, creating overhead. Session-level caching eliminates this repeated work for sequential queries against the same tables, significantly improving user experience for interactive workloads.

**Independent Test**: Can be fully tested by executing multiple queries against a user table in the same session, measuring execution time, and verifying subsequent queries execute faster due to cached table registration (observable through query timing and session cache inspection).

**Acceptance Scenarios**:

1. **Given** a user queries their table for the first time in a session, **When** the query executes, **Then** the table is registered and the registration is cached in the session context
2. **Given** a table registration exists in the session cache, **When** a subsequent query references the same table, **Then** the cached registration is used without re-registration
3. **Given** a user session has multiple cached table registrations, **When** tables remain unused beyond a configured timeout, **Then** those registrations are automatically evicted from the cache
4. **Given** a table's schema is modified, **When** a query attempts to use a cached registration, **Then** the system detects the schema change and re-registers the table with the updated schema

---

### User Story 5 - Namespace Validation for Table Creation (Priority: P2)

Database users should be prevented from creating tables in non-existent namespaces. The system must validate namespace existence before allowing any table creation operation.

**Why this priority**: Data integrity and organizational structure depend on proper namespace management. Allowing table creation in non-existent namespaces leads to orphaned data and confusing error states.

**Independent Test**: Can be fully tested by attempting to create a table with a non-existent namespace, verifying the operation fails with a clear error message, then creating the namespace and confirming the table creation succeeds.

**Acceptance Scenarios**:

1. **Given** a user attempts to create a table, **When** they specify a namespace that doesn't exist, **Then** the system returns an error: "Namespace 'X' does not exist. Create it first with CREATE NAMESPACE."
2. **Given** a namespace exists, **When** a user creates a table within that namespace, **Then** the table is successfully created
3. **Given** validation applies to all table types, **When** creating user, shared, or stream tables, **Then** namespace existence is validated for each type

---

### User Story 6 - Code Quality and Maintenance Improvements (Priority: P3)

Development teams need a clean, maintainable codebase with reduced duplication, consistent patterns, and comprehensive documentation. The system should follow established architectural principles and use shared abstractions where appropriate.

**Why this priority**: Code quality improvements don't directly impact end users but significantly affect development velocity, bug rates, and long-term maintainability. These are important for sustainable development but can be addressed after core functionality is stable.

**Independent Test**: Can be verified through code review, measuring metrics like code duplication percentage, test coverage, documentation completeness, and adherence to architectural patterns defined in project guidelines.

**Acceptance Scenarios**:

1. **Given** multiple system table providers exist, **When** reviewing the codebase, **Then** they share a common base implementation eliminating duplication
2. **Given** table name constants are needed across crates, **When** examining the code, **Then** all table names are defined once in a shared location (e.g., kalamdb-commons)
3. **Given** type-safe wrappers exist (NamespaceId, TableName), **When** reviewing function signatures, **Then** they consistently use these types instead of raw strings
4. **Given** critical functions like scan() exist, **When** reviewing code, **Then** they have comprehensive documentation explaining their purpose, parameters, and usage patterns
5. **Given** repeated string formatting patterns exist (e.g., column family naming), **When** examining the code, **Then** they use centralized helper functions instead of inline formatting
6. **Given** the project uses external dependencies, **When** performing maintenance, **Then** all crate dependencies are updated to their latest compatible versions
7. **Given** the README documentation exists, **When** reviewing it, **Then** it accurately reflects current architecture with minimal Parquet-specific mentions and includes WebSocket information
8. **Given** DDL-related code exists across crates, **When** reviewing architecture, **Then** DDL definitions are consolidated in kalamdb-sql where they logically belong
9. **Given** storage operations use RocksDB, **When** reviewing direct usage, **Then** kalamdb-sql accesses storage through kalamdb-store abstraction layer instead of direct RocksDB calls
10. **Given** system tables need a catalog, **When** querying system tables, **Then** they use "system" as the default catalog consistently
11. **Given** test suites exist, **When** running tests, **Then** they support configuration to run against either local server or temporary test server
12. **Given** a kalamdb-commons crate is needed, **When** reviewing crate structure, **Then** shared models (UserId, NamespaceId, TableName), system helpers, error types, and configuration models are consolidated in kalamdb-commons
13. **Given** testing and development dependencies exist, **When** building release binaries, **Then** test-only libraries are excluded from the final binary to minimize size
14. **Given** live query subscriptions need management, **When** reviewing architecture, **Then** a separate kalamdb-live crate handles subscription logic and communication with kalamdb-store and kalamdb-sql
15. **Given** live query filtering uses expressions, **When** implementing filter checks, **Then** DataFusion expression objects are used and cached for performance
16. **Given** SQL functions are needed, **When** implementing custom functions, **Then** DataFusion's built-in function infrastructure is leveraged where possible

---

### User Story 7 - Storage Backend Abstraction and Architecture Cleanup (Priority: P3)

Development teams need the ability to support alternative storage backends beyond RocksDB while maintaining consistent APIs. The system should abstract storage operations to allow pluggable backends like Sled, Redis, or others in the future.

**Why this priority**: Storage backend flexibility is important for future scalability and deployment options, but doesn't block current functionality. This architectural improvement enables future features without requiring large rewrites.

**Independent Test**: Can be verified by implementing a storage trait/interface, migrating RocksDB to use this interface, and demonstrating that storage operations work identically through the abstraction layer.

**Acceptance Scenarios**:

1. **Given** storage operations are needed, **When** reviewing the architecture, **Then** a storage backend trait/interface defines all required operations
2. **Given** RocksDB is the current backend, **When** examining implementations, **Then** RocksDB operations implement the storage trait without exposing RocksDB-specific details
3. **Given** system tables have a naming convention, **When** renaming occurs, **Then** "system.storage_locations" is renamed to "system.storages" consistently across all code and documentation
4. **Given** column families are used for organization, **When** considering alternative backends, **Then** the abstraction layer provides equivalent partitioning mechanisms for non-RocksDB backends

---

### User Story 8 - Documentation Organization and Deployment Infrastructure (Priority: P3)

Users and operators need well-organized documentation with clear categories and containerized deployment options. Documentation should be easy to navigate with logical grouping, and deployment should be straightforward using Docker.

**Why this priority**: Good documentation and deployment infrastructure lower barriers to entry and improve operational efficiency. While not blocking development, these improvements significantly enhance user experience and production readiness.

**Independent Test**: Can be verified by reviewing the organized /docs folder structure with clear categories, building a Docker image successfully, and running the system via docker-compose with all services functional.

**Acceptance Scenarios**:

1. **Given** documentation files exist in /docs, **When** organizing them, **Then** they are categorized into logical subfolders: build/, quickstart/, architecture/
2. **Given** outdated or redundant documentation exists, **When** cleaning up /docs, **Then** unnecessary files are removed while preserving essential information
3. **Given** users need to run KalamDB in containers, **When** Docker files are created, **Then** a complete Dockerfile exists in /docker folder that builds a working image
4. **Given** a Dockerfile exists, **When** building the image, **Then** the resulting container includes the server binary and required dependencies
5. **Given** deployment scenarios exist, **When** providing orchestration, **Then** a docker-compose.yml in /docker folder enables single-command system startup
6. **Given** docker-compose configuration exists, **When** running the system, **Then** all services (database server, storage volumes, networking) are properly configured

---

### Edge Cases

- What happens when a parametrized query is submitted with a parameter count that doesn't match the placeholder count?
- How does the system handle flush operations when storage location is unavailable or disk space is exhausted?
- What occurs if a manual flush is triggered while an automatic flush is already in progress for the same table?
- How does table registration caching behave when a user's session spans a schema migration?
- What happens when attempting to create a table in a namespace that was deleted after validation but before table creation completes?
- How does the system handle queries against cached table registrations when the underlying table has been dropped?
- What occurs when sharding configuration changes while flush jobs are in progress?
- How does the system handle flush job scheduling when the server was offline during scheduled flush time?
- What happens when switching storage backends with existing data in RocksDB - is migration required?
- How does the storage abstraction handle backend-specific features (like RocksDB column families) when using backends that don't support them?
- What occurs when dependency updates introduce breaking API changes in external crates?
- How does the system handle references to "storage_locations" during the migration period to "storages"?
- What happens when kalamdb-commons types are updated - how do dependent crates handle version mismatches?
- How does the system handle circular dependencies if kalamdb-commons depends on other crates?
- What occurs when kalamdb-live loses connection to kalamdb-store or kalamdb-sql during active subscriptions?
- How are cached DataFusion expressions invalidated when query semantics change?
- What happens when a SQL function requires functionality not available in DataFusion's built-in UDFs?
- What occurs when documentation is moved during reorganization and external links break?
- How does the Docker container handle configuration file updates without rebuilding the image?
- What happens when persistent volumes in docker-compose contain data from incompatible schema versions?
- How does the Docker image behave when required environment variables are not provided?

## Requirements *(mandatory)*

### Functional Requirements

#### Parametrized Query Support

- **FR-001**: System MUST accept SQL queries with positional parameter placeholders ($1, $2, etc.) via the `/api/sql` endpoint
- **FR-002**: API request body MUST support a format containing both `sql` (query string) and `params` (array of parameter values)
- **FR-003**: System MUST validate that the number of provided parameters matches the number of placeholders in the query
- **FR-004**: System MUST compile parametrized queries into reusable execution plans on first execution
- **FR-005**: System MUST cache compiled query execution plans indexed by the normalized query structure
- **FR-006**: System MUST substitute parameter values into cached execution plans without recompilation
- **FR-007**: System MUST support parameter types: string, integer, float, boolean, timestamp
- **FR-008**: System MUST return clear error messages when parameter types don't match expected column types
- **FR-009**: API response MUST optionally include query execution time when configured in `config.toml`

#### Automatic Flushing System

- **FR-010**: System MUST support configuration of automatic flush intervals per table at creation time
- **FR-011**: System MUST initialize a scheduler service that monitors all tables with flush configurations
- **FR-012**: Scheduler MUST trigger flush jobs at configured intervals for each table
- **FR-013**: Flush jobs MUST group buffered data by user_id before writing to storage
- **FR-014**: System MUST support configurable storage location path templates with variables: {storageLocation}, {namespace}, {userId}, {tableName}, {shard}
- **FR-015**: System MUST provide default storage location in `config.toml` (defaulting to `./data/storage`)
- **FR-016**: System MUST support separate path templates for user tables vs shared tables
- **FR-017**: User table default path template MUST be: `{storageLocation}/{namespace}/users/{userId}/{tableName}/`
- **FR-018**: Shared table default path template MUST be: `{storageLocation}/{namespace}/{tableName}/`
- **FR-019**: System MUST support configurable sharding strategies for distributing data across storage locations
- **FR-020**: System MUST provide a default alphabetic sharding strategy (a-z) when no custom strategy is specified
- **FR-021**: Flush jobs MUST write data in Parquet format to the determined storage locations
- **FR-022**: System MUST track flush job status using an actor model for observability
- **FR-023**: Each Parquet file MUST include metadata indicating the schema version used

#### Manual Flushing Commands

- **FR-024**: System MUST support SQL command: `FLUSH TABLE <namespace>.<table_name>`
- **FR-025**: System MUST support SQL command: `FLUSH ALL TABLES` to flush all tables with buffered data
- **FR-026**: Manual flush commands MUST be synchronous, returning only after flush operation completes
- **FR-027**: Flush command response MUST include number of records flushed and target storage location
- **FR-028**: System MUST automatically flush all tables during server shutdown sequence before process termination
- **FR-029**: System MUST prevent concurrent flush operations on the same table (queue subsequent requests)

#### Session-Level Table Caching

- **FR-030**: System MUST maintain a per-user session context for database operations
- **FR-031**: Session context MUST cache table registrations for tables accessed during the session
- **FR-032**: System MUST reuse cached table registrations for subsequent queries within the same session
- **FR-033**: System MUST implement a configurable timeout for cached table registrations (LRU or time-based eviction)
- **FR-034**: System MUST automatically evict unused table registrations based on eviction policy
- **FR-035**: System MUST detect schema changes and invalidate cached registrations when schema modifications occur
- **FR-036**: System MUST validate cached table registrations still reference existing tables before query execution

#### Namespace Validation

- **FR-037**: System MUST validate namespace existence before creating any user, shared, or stream table
- **FR-038**: System MUST return error "Namespace '<namespace>' does not exist" when table creation references non-existent namespace
- **FR-039**: Error message MUST include guidance: "Create it first with CREATE NAMESPACE."
- **FR-040**: Validation MUST be transactional to prevent race conditions between validation and table creation

#### Code Quality and Architectural Improvements

- **FR-041**: System MUST provide a common base implementation for system table providers to eliminate code duplication
- **FR-042**: System MUST define all system table names in a centralized location (single source of truth)
- **FR-043**: System MUST consistently use type-safe wrappers (NamespaceId, TableName, UserId) instead of raw strings throughout the codebase
- **FR-044**: All scan() functions MUST include documentation explaining their purpose, key parameter usage, and architectural role
- **FR-045**: Column family naming logic MUST be centralized in helper functions instead of inline string formatting
- **FR-046**: Validation logic for insert operations MUST be shared between user and shared table providers
- **FR-047**: System MUST store metadata columns ("_deleted", "_updated") efficiently without repeated string serialization
- **FR-048**: System table constant strings (like "SHOW BACKUP FOR DATABASE") MUST be defined once as enums or constants
- **FR-062**: All Rust crate dependencies MUST be updated to their latest compatible versions
- **FR-063**: README documentation MUST be rewritten to accurately reflect current architecture
- **FR-064**: README MUST minimize Parquet-specific details (mention once maximum)
- **FR-065**: README MUST document that WebSocket connections are direct to the server (no intermediary service)
- **FR-066**: DDL statement definitions and models MUST be located in kalamdb-sql crate where they logically belong
- **FR-067**: kalamdb-sql MUST access storage through kalamdb-store abstraction layer instead of direct RocksDB calls
- **FR-068**: System tables MUST use "system" as the default catalog name consistently
- **FR-069**: Test framework MUST support configuration to run tests against local server or temporary test server
- **FR-070**: System table "storage_locations" MUST be renamed to "storages" across all code, configuration, and documentation
- **FR-077**: A kalamdb-commons crate MUST be created to consolidate shared models, helpers, and types
- **FR-078**: kalamdb-commons MUST include type-safe models: UserId, NamespaceId, TableName, TableType
- **FR-079**: kalamdb-commons MUST include system table name constants (centralized enum or constants)
- **FR-080**: kalamdb-commons MUST include shared error types used across kalamdb-core, kalamdb-sql, and kalamdb-store
- **FR-081**: kalamdb-commons MUST include configuration models from kalamdb-server that other crates depend on
- **FR-082**: kalamdb-commons MUST include system helper functions used across multiple crates
- **FR-083**: Release build configuration MUST exclude testing and dev-only dependencies from final binary
- **FR-084**: Binary size MUST be audited to identify and remove unused dependencies
- **FR-085**: A kalamdb-live crate MUST be created to manage live query subscriptions separately from core logic
- **FR-086**: kalamdb-live MUST handle WebSocket subscription lifecycle and client notification
- **FR-087**: kalamdb-live MUST communicate with kalamdb-store for data access and kalamdb-sql for query execution
- **FR-088**: Live query expression evaluation MUST use DataFusion Expression objects
- **FR-089**: DataFusion Expression objects for live query filters MUST be compiled once and cached
- **FR-090**: SQL custom functions MUST leverage DataFusion's UDF (User Defined Function) infrastructure where applicable
- **FR-091**: SQL function implementations MUST reuse DataFusion built-in functions when functionality overlaps

#### Documentation Organization and Deployment

- **FR-092**: Documentation folder (/docs) MUST be organized into clear categorical subfolders
- **FR-093**: /docs/build/ MUST contain build instructions, compilation guides, and dependency information
- **FR-094**: /docs/quickstart/ MUST contain getting started guides, basic examples, and initial setup instructions
- **FR-095**: /docs/architecture/ MUST contain system design documents, architectural decisions, and component diagrams
- **FR-096**: Outdated and redundant documentation files MUST be identified and removed from /docs
- **FR-097**: All Docker-related files MUST be located in /docker folder at repository root
- **FR-098**: A production-ready Dockerfile MUST exist in /docker folder that builds KalamDB server
- **FR-099**: Dockerfile MUST use multi-stage builds to minimize final image size
- **FR-100**: Dockerfile MUST include only runtime dependencies in the final image (no build tools)
- **FR-101**: A docker-compose.yml MUST exist in /docker folder for orchestrating KalamDB deployment
- **FR-102**: docker-compose.yml MUST configure KalamDB server with appropriate environment variables
- **FR-103**: docker-compose.yml MUST define persistent volume mounts for data storage
- **FR-104**: docker-compose.yml MUST configure networking to expose appropriate ports (API, WebSocket)
- **FR-105**: Docker image MUST be configurable via environment variables (config.toml overrides)

#### Storage Backend Abstraction

- **FR-071**: System MUST define a storage backend trait/interface that abstracts storage operations
- **FR-072**: Storage trait MUST include operations for: get, put, delete, scan, batch operations, column family management
- **FR-073**: RocksDB implementation MUST implement the storage trait without exposing RocksDB-specific types
- **FR-074**: Storage abstraction MUST support pluggable backends (Sled, Redis, or custom implementations)
- **FR-075**: Column family concept MUST be abstracted to work with storage backends that don't natively support it
- **FR-076**: All existing RocksDB column family usage MUST be migrated to use the abstracted storage trait

#### Configuration and System Management

- **FR-049**: RocksDB storage directory MUST be configurable via `config.toml`
- **FR-050**: RocksDB storage directory MUST default to a location relative to the server binary (not temporary directory)
- **FR-051**: System MUST log RocksDB database size at server startup and periodically during operation
- **FR-052**: Server startup logs MUST include Git branch name and commit revision in version information
- **FR-053**: Configuration MUST support enabling/disabling query execution time reporting in API responses
- **FR-054**: System MUST support configurable localhost authentication bypass (allowing queries without JWT)
- **FR-055**: When localhost bypass is enabled, configuration MUST specify default user_id for localhost connections (defaulting to "system")
- **FR-056**: Non-localhost connections MUST always require valid JWT with user_id claim

#### Data Organization and Query Optimization

- **FR-057**: All scan operations on user-partitioned data MUST filter by user_id at the storage level
- **FR-058**: Scan operations on stream tables MUST filter by user_id to prevent full table scans
- **FR-059**: System MUST prevent users from subscribing to shared tables via WebSocket (performance protection)
- **FR-060**: When querying user tables with `.user.` qualifier (e.g., `FROM namespace1.user.user_files`), system MUST require X-USER-ID header
- **FR-061**: System MUST substitute `.user.` qualifier with actual user_id from X-USER-ID header in query resolution

### Key Entities

- **ParametrizedQuery**: Represents a SQL query with positional parameter placeholders and the array of parameter values to be substituted
- **QueryExecutionPlan**: A compiled and optimized execution plan for a specific query structure, cached for reuse with different parameter values
- **FlushJob**: Represents a scheduled or manual operation to persist buffered table data to Parquet files in storage locations
- **FlushConfiguration**: Defines automatic flush behavior for a table including interval, storage path template, and sharding strategy
- **StorageLocation**: A configured destination for persisted data with path template and template variables (namespace, userId, shard, tableName)
- **ShardingStrategy**: A function or algorithm that determines which shard a particular data subset should be written to
- **SessionCache**: A per-user session context maintaining cached table registrations and other session-specific state
- **TableRegistration**: A cached reference to a table's schema and metadata within a session, enabling fast query execution without re-registration
- **SystemTableProvider**: Base abstraction for system tables with common scanning, filtering, and projection logic
- **StorageBackend**: Trait/interface defining storage operations (get, put, delete, scan, batch) that can be implemented by different storage engines
- **StorageTrait**: Generic storage abstraction supporting pluggable backends (RocksDB, Sled, Redis, etc.) with consistent APIs
- **kalamdb-commons**: Shared crate containing type-safe models, constants, error types, and helper functions used across all other crates
- **LiveQuerySubscription**: Represents an active WebSocket subscription to a query with filter expressions and client notification state
- **CachedExpression**: A compiled and cached DataFusion Expression object used for efficient live query filtering without repeated parsing
- **DocumentationCategory**: Logical grouping of documentation files (build, quickstart, architecture) for organized navigation
- **DockerImage**: Containerized KalamDB server with runtime dependencies, built via multi-stage Dockerfile
- **DockerCompose**: Orchestration configuration defining services, volumes, networks, and environment for KalamDB deployment

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Parametrized queries with cached execution plans execute at least 40% faster than non-parametrized equivalent queries with identical logic
- **SC-002**: Session-level table registration caching reduces query execution time for repeated table access by at least 30% compared to per-query registration
- **SC-003**: Automatic flush jobs complete successfully for tables with up to 1 million buffered records within 5 minutes
- **SC-004**: Flush operations organize data correctly with 100% accuracy (no misplaced files, correct user/namespace/shard paths)
- **SC-005**: Manual flush commands complete synchronously and return status within 10 seconds for tables with under 100,000 records
- **SC-006**: Namespace validation prevents 100% of table creation attempts in non-existent namespaces
- **SC-007**: System handles at least 100 concurrent parametrized queries without execution plan cache contention or errors
- **SC-008**: Query plan cache reduces DataFusion compilation calls by at least 80% for workloads with repeated query patterns
- **SC-009**: Flush job scheduler maintains configured intervals with less than 5% drift over 24-hour periods
- **SC-010**: Server shutdown with automatic flush completes within 30 seconds for databases with under 10 active tables
- **SC-011**: Session cache eviction policy maintains memory usage under configured limits while maximizing hit rate
- **SC-012**: Code duplication in system table providers reduces by at least 70% after refactoring to shared base implementation
- **SC-013**: All public scan() functions have comprehensive documentation with usage examples
- **SC-014**: Type-safe wrappers (NamespaceId, TableName, UserId) replace at least 95% of raw string usage for identifiers
- **SC-015**: All Rust dependencies are updated to latest compatible versions without breaking changes
- **SC-016**: README accurately documents current architecture with WebSocket information and minimal Parquet references
- **SC-017**: All DDL definitions are consolidated in kalamdb-sql crate (100% migration)
- **SC-018**: kalamdb-sql eliminates all direct RocksDB calls and uses kalamdb-store abstraction (100% migration)
- **SC-019**: Storage backend abstraction trait enables at least one alternative backend implementation (proof of concept)
- **SC-020**: System table "storage_locations" is fully renamed to "storages" with no legacy references remaining
- **SC-021**: Test framework successfully runs all tests against both local and temporary server configurations
- **SC-022**: kalamdb-commons crate consolidates at least 95% of shared types and constants across other crates
- **SC-023**: Release binary size reduces by at least 10% after removing test-only dependencies and unused crates
- **SC-024**: kalamdb-live crate successfully manages all live query subscriptions with clear separation from core logic
- **SC-025**: Live query filter evaluation with cached DataFusion expressions executes at least 50% faster than string-based parsing
- **SC-026**: SQL custom functions leverage DataFusion UDFs for at least 80% of common function operations
- **SC-027**: Documentation in /docs is organized into 3 clear categories with no files in root folder
- **SC-028**: Docker image builds successfully and starts KalamDB server within 30 seconds
- **SC-029**: docker-compose brings up fully functional KalamDB system with single command
- **SC-030**: Docker image size is under 100MB (excluding data volumes)

### Documentation Success Criteria (Constitution Principle VIII)

- **SC-DOC-001**: All public APIs have comprehensive rustdoc comments with real-world examples
- **SC-DOC-002**: Module-level documentation explains purpose and architectural role
- **SC-DOC-003**: Complex algorithms and architectural patterns have inline comments explaining rationale
- **SC-DOC-004**: Architecture Decision Records (ADRs) document key design choices for query caching and flush architecture
- **SC-DOC-005**: Code review verification confirms documentation requirements are met

## Assumptions

1. **DataFusion Integration**: The project uses Apache DataFusion for query compilation and execution, and its query plan caching capabilities are available
2. **Storage Format**: Parquet is the established format for persisted data; flush operations continue using this format
3. **RocksDB Usage**: The system currently uses RocksDB for buffering data before flush; this remains the buffer storage mechanism
4. **Authentication**: JWT-based authentication is already implemented; localhost bypass is an additional configuration option
5. **WebSocket Infrastructure**: WebSocket support for subscriptions exists; preventing shared table subscriptions is a policy enforcement addition
6. **Actor Model**: The project already uses actor patterns for some subsystems (like live queries); flush jobs follow the same pattern
7. **Configuration Format**: TOML format is the established configuration mechanism; new settings follow existing conventions
8. **Multi-tenancy Model**: User isolation and user_id-based data organization are core architectural principles already in place
9. **Namespace Concept**: Namespaces are an existing organizational structure; the change is enforcing their existence before table creation
10. **Default Sharding**: Alphabetic (a-z) sharding provides 26 shards initially; custom sharding functions can be added later
11. **Schema Versioning**: The ability to store metadata in Parquet files exists; schema version is one additional metadata field
12. **Session Context**: The concept of user sessions exists; table registration caching extends existing session infrastructure
13. **Backward Compatibility**: Changes are additive; existing non-parametrized queries continue working unchanged
14. **Performance Baseline**: Current system performance is measured and known, enabling validation of improvement targets
15. **Docker Availability**: Docker and docker-compose are available in the development and deployment environments
16. **Documentation Format**: Markdown is the standard format for documentation; reorganization maintains this format
17. **Existing Documentation**: Some documentation exists in /docs; reorganization improves structure rather than creating from scratch

## Out of Scope

The following items are explicitly NOT included in this feature specification:

1. **Distributed Query Execution**: This feature focuses on single-node optimization; distributed query planning across Raft cluster nodes is separate
2. **Advanced Sharding Strategies**: Only alphabetic sharding is included; sophisticated sharding functions (consistent hashing, range-based, custom user functions) are future enhancements
3. **Query Result Caching**: This feature caches execution plans only; caching actual query results is a separate performance optimization
4. **Automatic Schema Migration**: Session cache invalidation detects schema changes but does not automatically migrate data; migration remains a separate concern
5. **Compaction Jobs**: Merging multiple Parquet files in storage is mentioned in notes but is a separate background maintenance feature
6. **User File Storage**: The `user_files` table concept mentioned in notes is a distinct feature, not part of this specification
7. **Workflow Triggers (KFlows)**: Event-driven workflows listening to streams are a separate feature area
8. **Raft Replication Logic**: While flush jobs must work in a Raft environment, the replication protocol itself is not part of this feature
9. **CLI Tool Development**: The command-line interface mentioned in notes is a separate tool, not part of the database server improvements
10. **Client SDK Development**: Rust/WebAssembly SDK and TypeScript SDK are separate client-side projects
11. **Index Support**: Column-level indexes (BLOOM, SORTED) mentioned in notes are a separate query optimization feature
12. **Auto-increment Columns**: Automatic ID generation is a separate DDL enhancement
13. **Example Projects**: TypeScript TODO app example is separate documentation/sample code
14. **Binary Distribution**: Auto-deploy to GitHub releases is CI/CD pipeline work, not feature development
15. **Kubernetes/Helm Charts**: Container orchestration beyond docker-compose is separate infrastructure work

## Dependencies

- **Existing Configuration System**: Requires `config.toml` parsing and validation infrastructure
- **DataFusion Query Engine**: Depends on DataFusion APIs for query compilation, execution plans, and parameter binding
- **RocksDB Storage Layer**: Requires RocksDB column families for buffering user and shared table data
- **Parquet Writing Infrastructure**: Depends on existing Parquet serialization and file writing capabilities
- **Actor Framework**: Flush jobs depend on actor model infrastructure for job tracking and observability
- **Session Management**: Table registration caching depends on user session context tracking
- **Namespace Management**: Namespace validation requires functional namespace creation and metadata storage
- **Authentication System**: JWT validation and user_id extraction must be functional for secure parametrized queries
- **WebSocket Subscription System**: Preventing shared table subscriptions requires access to subscription logic

## Risks and Mitigations

### Risk: Query Plan Cache Memory Growth
**Impact**: Unbounded query plan cache could exhaust server memory with diverse query patterns  
**Mitigation**: Implement LRU eviction policy with configurable cache size limits; monitor cache hit rates and memory usage

### Risk: Flush Job Backlog During High Write Volume
**Impact**: Flush jobs may fall behind during sustained high insert rates, causing buffer growth  
**Mitigation**: Implement flush job queuing with priority handling; add monitoring alerts for flush lag; support multiple concurrent flush workers

### Risk: Race Conditions in Table Registration Cache
**Impact**: Concurrent queries may cause duplicate table registrations or cache inconsistency  
**Mitigation**: Use proper locking or lock-free data structures for session cache access; validate cache consistency in tests

### Risk: Storage Path Injection Vulnerabilities
**Impact**: Malicious template variable values could cause writes outside intended directories  
**Mitigation**: Validate and sanitize all path template variables; use safe path joining functions; restrict allowed characters

### Risk: Flush Failure Data Loss
**Impact**: If flush operation fails after clearing buffer, data could be permanently lost  
**Mitigation**: Implement write-ahead logging for flush operations; only clear buffer after successful Parquet file write; support flush retry logic

### Risk: Schema Version Mismatch on Read
**Impact**: Reading Parquet files with incompatible schema versions could cause query failures  
**Mitigation**: Store schema version in file metadata; validate version compatibility on file open; support schema evolution rules

### Risk: Performance Regression from Validation Overhead
**Impact**: Adding namespace validation to table creation could slow down bulk table creation operations  
**Mitigation**: Cache namespace existence checks; use efficient lookup data structures; batch validation when possible

### Risk: Cache Invalidation Complexity
**Impact**: Detecting schema changes for cache invalidation may miss edge cases, causing stale cache usage  
**Mitigation**: Use schema version tracking; implement conservative invalidation (invalidate on any DDL); add manual cache clearing commands for troubleshooting

### Risk: Dependency Update Breaking Changes
**Impact**: Updating all dependencies to latest versions may introduce breaking API changes or incompatibilities  
**Mitigation**: Update dependencies incrementally with comprehensive test runs; pin major versions; maintain compatibility layer for critical dependencies; use semantic versioning strictly

### Risk: Storage Abstraction Performance Overhead
**Impact**: Adding an abstraction layer over RocksDB could introduce performance degradation from indirect calls  
**Mitigation**: Use zero-cost abstractions (traits with monomorphization); benchmark before/after abstraction; use inline hints for hot paths; consider trait objects only where dynamic dispatch is necessary

### Risk: Incomplete Storage Backend Migration
**Impact**: Missing direct RocksDB calls in kalamdb-sql could cause runtime errors or data inconsistencies  
**Mitigation**: Comprehensive code search for RocksDB imports; static analysis to detect direct usage; integration tests covering all storage operations; gradual migration with feature flags

### Risk: README Staleness After Update
**Impact**: Updated README may become outdated again as system evolves  
**Mitigation**: Include README review in pull request checklist; automate validation of code examples in documentation; link README sections to specific code locations with automated checks

### Risk: Storage Backend Feature Parity
**Impact**: Alternative storage backends (Sled, Redis) may lack features available in RocksDB (column families, transactions)  
**Mitigation**: Define minimum feature set for storage trait; document backend-specific capabilities; graceful degradation for optional features; clear error messages for unsupported operations

### Risk: kalamdb-commons Circular Dependencies
**Impact**: Creating a shared commons crate could introduce circular dependencies between crates  
**Mitigation**: Design commons crate as dependency-free foundation; only include pure data types and helpers; no logic that depends on other kalamdb crates; strict layered architecture

### Risk: Expression Cache Stale Data
**Impact**: Cached DataFusion expressions for live queries may not reflect updated query semantics or schema changes  
**Mitigation**: Include schema version in cache keys; invalidate cache on any schema DDL; implement cache TTL; add manual cache clear for troubleshooting

### Risk: kalamdb-live Communication Failures
**Impact**: If kalamdb-live loses connection to kalamdb-store or kalamdb-sql, subscriptions fail without graceful handling  
**Mitigation**: Implement connection retry logic; queue operations during temporary disconnections; notify clients of subscription errors; include health checks

### Risk: Binary Size Regression
**Impact**: Future dependency additions could reintroduce bloat after optimization efforts  
**Mitigation**: Add binary size checks to CI/CD pipeline; document approved dependency list; require justification for new dependencies; use feature flags to make dependencies optional

### Risk: DataFusion UDF Limitations
**Impact**: Some custom SQL functions may require features not available in DataFusion's UDF infrastructure  
**Mitigation**: Document DataFusion limitations; provide extension points for custom implementations; use DataFusion UDFs as default with fallback to custom code; contribute missing features upstream

### Risk: Documentation Link Breakage
**Impact**: Reorganizing /docs folder structure may break external links and bookmarks to documentation  
**Mitigation**: Create redirects or index file mapping old paths to new paths; communicate changes in release notes; use relative links within documentation; validate internal links after reorganization

### Risk: Docker Image Size Bloat
**Impact**: Including unnecessary build dependencies or layers could result in large Docker images  
**Mitigation**: Use multi-stage builds with separate builder and runtime stages; use Alpine or distroless base images; only COPY required artifacts; use .dockerignore to exclude unnecessary files; regularly audit image layers

### Risk: Docker Configuration Drift
**Impact**: docker-compose.yml configuration may diverge from actual deployment requirements  
**Mitigation**: Test docker-compose deployment in CI/CD; document required environment variables; provide example .env file; validate configuration matches production needs; include healthchecks

### Risk: Volume Permission Issues
**Impact**: Docker containers may have permission conflicts with host-mounted volumes for data persistence  
**Mitigation**: Document volume ownership requirements; use appropriate USER directive in Dockerfile; provide initialization scripts for volume setup; include troubleshooting guide for permission errors
