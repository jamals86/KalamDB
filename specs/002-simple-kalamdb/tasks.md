# Tasks: Simple KalamDB - User-Based Database with Live Queries

**Feature Branch**: `002-simple-kalamdb`  
**Input**: Design documents from `/specs/002-simple-kalamdb/`  
**Prerequisites**: spec.md (required)

**Tests**: Tests are NOT included in this task list as they were not explicitly requested in the specification. Focus is on implementation tasks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (Setup, Foundation, US0, US1, US2, etc.)
- Include exact file paths in descriptions

## Path Conventions
- Backend code: `backend/crates/`
- Core library: `backend/crates/kalamdb-core/src/`
- API library: `backend/crates/kalamdb-api/src/`
- Server binary: `backend/crates/kalamdb-server/src/`
- Configuration files: `backend/conf/` (config.toml, config.example.toml, namespaces.json, storage_locations.json)
- Data storage: `backend/conf/{namespace}/schemas/{table_name}/` for schema versioning

---

## Phase 1: Setup & Code Removal (Clean Slate)

**Purpose**: Remove existing message-centric implementation and prepare for table-centric architecture

- [X] T001 [P] [Setup] Remove existing Message model from `backend/crates/kalamdb-core/src/models/message.rs`
- [X] T002 [P] [Setup] Remove MessageStore trait and implementations from `backend/crates/kalamdb-core/src/storage/message_store.rs`
- [X] T003 [P] [Setup] Remove message-specific query logic from `backend/crates/kalamdb-core/src/storage/query.rs`
- [X] T004 [P] [Setup] Remove existing SQL parser logic from `backend/crates/kalamdb-core/src/sql/parser.rs` (will be replaced with DataFusion)
- [X] T005 [P] [Setup] Remove existing SQL executor from `backend/crates/kalamdb-core/src/sql/executor.rs` (will be replaced with DataFusion)
- [X] T006 [P] [Setup] Remove message-specific handlers from `backend/crates/kalamdb-api/src/handlers/messages.rs`
- [X] T007 [Setup] Update `backend/Cargo.toml` dependencies: Add DataFusion, Arrow, Parquet, Actix-Web for WebSocket support
- [X] T008 [P] [Setup] Create new directory structure: `backend/crates/kalamdb-core/src/catalog/` for namespace/table catalog
- [X] T009 [P] [Setup] Create new directory structure: `backend/crates/kalamdb-core/src/schema/` for Arrow schema management
- [X] T010 [P] [Setup] Create new directory structure: `backend/crates/kalamdb-core/src/tables/` for table types (user, shared, stream)
- [X] T011 [P] [Setup] Create new directory structure: `backend/crates/kalamdb-core/src/flush/` for flush policy management
- [X] T013 [P] [Setup] Create new directory structure: `backend/crates/kalamdb-core/src/live_query/` for live subscription management

### Additional Cleanup Tasks (Completed)

- [X] T007a [Setup] Update module files to remove references to deleted modules (`backend/crates/kalamdb-core/src/models/mod.rs`, `backend/crates/kalamdb-core/src/storage/mod.rs`, `backend/crates/kalamdb-core/src/sql/mod.rs`)
- [X] T007b [Setup] Clean up `backend/crates/kalamdb-core/src/storage/rocksdb_store.rs` to remove MessageStore trait implementation while preserving generic RocksDB operations
- [X] T007c [Setup] Update `backend/crates/kalamdb-core/src/lib.rs` to export new catalog, schema, tables, flush, and live_query modules
- [X] T007d [Setup] Create placeholder `mod.rs` files for all new directories with rustdoc documentation
- [X] T007e [Setup] Temporarily disable old query handlers in `backend/crates/kalamdb-api/src/handlers/mod.rs` (will be reimplemented with DataFusion)
- [X] T007f [Setup] Update `backend/crates/kalamdb-api/src/routes.rs` to prepare for new `/api/sql` endpoint structure
- [X] T007g [Setup] Update `backend/crates/kalamdb-server/src/main.rs` to minimal working state (components to be added in Phase 2)
- [X] T007h [Setup] Verify project compiles successfully with `cargo check` after all Phase 1 changes

**Phase 1 Status**: ✅ **COMPLETE** - All 12 core tasks + 8 cleanup tasks completed. Project compiles successfully. Clean slate ready for Phase 2 foundational work.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Core Data Structures

- [X] T014 [P] [Foundation] Create Namespace entity in `backend/crates/kalamdb-core/src/catalog/namespace.rs` (name, created_at, options, table_count)
- [X] T014a [P] [Foundation] Create NamespaceId type-safe wrapper in `backend/crates/kalamdb-core/src/catalog/namespace_id.rs` (newtype pattern around String, use everywhere instead of raw String for namespace identifiers)
- [X] T015 [P] [Foundation] Create TableMetadata entity in `backend/crates/kalamdb-core/src/catalog/table_metadata.rs` (table_name, table_type, namespace, created_at, storage_location, flush_policy)
- [X] T015a [P] [Foundation] Create TableName type-safe wrapper in `backend/crates/kalamdb-core/src/catalog/table_name.rs` (newtype pattern around String, use everywhere instead of raw String for table identifiers)
- [X] T015b [P] [Foundation] Create TableType enum in `backend/crates/kalamdb-core/src/catalog/table_type.rs` (values: User, Shared, System, Stream - use enum everywhere instead of String)
- [X] T015c [P] [Foundation] Create UserId type-safe wrapper in `backend/crates/kalamdb-core/src/catalog/user_id.rs` (newtype pattern around String, use everywhere instead of raw String for user identifiers)
- [X] T016 [P] [Foundation] Create FlushPolicy entity in `backend/crates/kalamdb-core/src/flush/policy.rs` (policy_type: RowLimit/TimeInterval, row_limit, time_interval)
- [X] T017 [P] [Foundation] Create StorageLocation entity in `backend/crates/kalamdb-core/src/catalog/storage_location.rs` (location_name, location_type, path, credentials_ref, usage_count)

### Configuration Persistence Foundation

- [X] T018 [P] [Foundation] Create configuration file manager in `backend/crates/kalamdb-core/src/config/file_manager.rs` (atomic file updates, JSON read/write)
- [X] T019 [P] [Foundation] Implement namespaces.json handler in `backend/crates/kalamdb-core/src/config/namespaces_config.rs` (load/save all namespaces)
- [X] T020 [P] [Foundation] Implement storage_locations.json handler in `backend/crates/kalamdb-core/src/config/storage_locations_config.rs` (load/save all storage locations)
- [X] T021 [Foundation] Create server startup configuration loader in `backend/crates/kalamdb-core/src/config/startup_loader.rs` (load all configs from JSON into in-memory catalog, NOT RocksDB - RocksDB is only for table data buffering)

### Schema Management Foundation

- [X] T022 [Foundation] Implement Arrow schema serialization/deserialization in `backend/crates/kalamdb-core/src/schema/arrow_schema.rs` (use DataFusion's built-in SchemaRef::to_json() and SchemaRef::from_json() methods, include table_options in schema files)
- [X] T023 [Foundation] Implement schema versioning logic in `backend/crates/kalamdb-core/src/schema/versioning.rs` (create schema_v{N}.json files with user options using DataFusion's schema serialization)
- [X] T024 [Foundation] Implement manifest.json management in `backend/crates/kalamdb-core/src/schema/manifest.rs` (read, write, update schema metadata)
- [X] T025 [Foundation] Implement schema directory structure creation in `backend/crates/kalamdb-core/src/schema/storage.rs` (/conf/{namespace}/schemas/{table_name}/)
- [X] T026 [Foundation] Add system column injection logic in `backend/crates/kalamdb-core/src/schema/system_columns.rs` (\_updated TIMESTAMP, \_deleted BOOLEAN for user/shared tables)

### RocksDB Column Family Architecture

- [X] T027 [Foundation] Implement column family manager in `backend/crates/kalamdb-core/src/storage/column_family_manager.rs` (create/delete column families with naming convention, use NamespaceId and TableName types)
- [X] T027a [Foundation] Add column family naming utilities in column_family_manager.rs (use TableType enum: user_table:{NamespaceId}:{TableName}, shared_table:{NamespaceId}:{TableName}, stream_table:{NamespaceId}:{TableName}, system_table:{TableName})
- [X] T027b [Foundation] Implement RocksDB configuration in `backend/crates/kalamdb-core/src/storage/rocksdb_config.rs` (per-column-family memtable, write buffer, WAL settings, compaction)
- [X] T027c [Foundation] Create RocksDB initialization in `backend/crates/kalamdb-core/src/storage/rocksdb_init.rs` (open database, create default column families, configure options)
- [X] T027d [Foundation] Create TableName initialization in `backend/crates/kalamdb-core/src/storage/table_name.rs` to be used as type-safe identifiers for tables instead of raw strings (MERGED with T015a - delete this duplicate task)

### RocksDB Catalog Store

- [X] T028 [Foundation] Implement catalog store using RocksDB in `backend/crates/kalamdb-core/src/catalog/catalog_store.rs` (store system table data in dedicated column families: system_table:users, system_table:live_queries, system_table:storage_locations, system_table:jobs, use UserId type)
- [X] T029 [Foundation] Add catalog key prefixes for different entity types in catalog_store.rs (use UserId for user keys, subscription_id:, location_name:, job_id: for system tables)
- [X] T030 [Foundation] Implement system table CRUD operations in catalog_store.rs (insert, update, delete, query system table data, use type-safe wrappers)
- [X] T031 [Foundation] Implement table metadata cache in `backend/crates/kalamdb-core/src/catalog/table_cache.rs` (in-memory cache for table metadata loaded from JSON, NOT from RocksDB, use NamespaceId and TableName as keys)

### DataFusion Integration Foundation

- [X] T032 [Foundation] Create DataFusion session factory in `backend/crates/kalamdb-core/src/sql/datafusion_session.rs` (create SessionContext with config, use NamespaceId and UserId in session state)
- [X] T033 [Foundation] Implement hybrid TableProvider in `backend/crates/kalamdb-core/src/tables/hybrid_table_provider.rs` (queries both RocksDB and Parquet, use TableName and TableType)
- [X] T034 [Foundation] Create RocksDB scan implementation for DataFusion in `backend/crates/kalamdb-core/src/tables/rocksdb_scan.rs` (convert RocksDB data to Arrow RecordBatch, support different key formats per TableType: {UserId}:{row_id} for User tables, {row_id} for Shared tables, {timestamp}:{row_id} for Stream tables)
- [X] T035 [Foundation] Create Parquet scan implementation for DataFusion in `backend/crates/kalamdb-core/src/tables/parquet_scan.rs` (scan Parquet files with bloom filter optimization)
- [X] T036 [Foundation] Implement \_deleted filter injection for queries in `backend/crates/kalamdb-core/src/sql/query_rewriter.rs` (add WHERE \_deleted = false by default)

### Storage Backend Foundation

- [X] T037 [P] [Foundation] Create filesystem storage backend in `backend/crates/kalamdb-core/src/storage/filesystem_backend.rs` (write/read Parquet files to local disk)
- [X] T038 [P] [Foundation] Create storage location template engine in `backend/crates/kalamdb-core/src/storage/path_template.rs` (substitute ${user_id} in paths using UserId type)
- [X] T039 [Foundation] Implement Parquet writer with \_updated bloom filter in `backend/crates/kalamdb-core/src/storage/parquet_writer.rs`

### System Tables Foundation

- [X] T040 [Foundation] Create system.users table schema in `backend/crates/kalamdb-core/src/tables/system/users.rs` (use UserId type, username, email, created_at)
- [X] T041 [Foundation] Create system.live_queries table schema in `backend/crates/kalamdb-core/src/tables/system/live_queries.rs` (live_id [PK, format: {user_id}-{unique_conn_id}-{table_name}-{query_id}], connection_id, table_name, query_id, use UserId and TableName types, query, options [JSON], created_at, updated_at, changes, node)
- [X] T042 [Foundation] Create system.storage_locations table schema in `backend/crates/kalamdb-core/src/tables/system/storage_locations.rs` (location_name, location_type, path, credentials_ref, usage_count)
- [X] T043 [Foundation] Create system.jobs table schema in `backend/crates/kalamdb-core/src/tables/system/jobs.rs` (job_id, job_type, use TableName for table_name, status, start_time, end_time, parameters, result, trace, memory_used_mb, cpu_used_percent, node_id, error_message)
- [X] T044 [Foundation] Implement system table registration in DataFusion in `backend/crates/kalamdb-core/src/tables/system/mod.rs` (register all system tables at startup)

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

**Phase 2 Status**: ✅ **COMPLETE** - All 37 foundational tasks completed. 122 tests passing. Ready for Phase 3 user story implementation.

---

## Phase 3: User Story 0 - REST API and WebSocket Interface (Priority: P1) 🎯 MVP Critical

**Goal**: Provide single REST API endpoint `/api/sql` for SQL execution and WebSocket endpoint `/ws` for live query subscriptions with initial data fetch and real-time updates

**Independent Test**: Send POST requests to `/api/sql` with various SQL commands and establish WebSocket connections with subscription queries

### Implementation for User Story 0

- [X] T045 [P] [US0] Create SqlRequest model in `backend/crates/kalamdb-api/src/models/sql_request.rs` (sql: String field)
- [X] T046 [P] [US0] Create SqlResponse models in `backend/crates/kalamdb-api/src/models/sql_response.rs` (status, results array, execution_time_ms, error details)
- [X] T047 [P] [US0] Create WebSocket subscription models in `backend/crates/kalamdb-api/src/models/ws_subscription.rs` (id, sql, options with last_rows)
- [X] T048 [P] [US0] Create WebSocket notification models in `backend/crates/kalamdb-api/src/models/ws_notification.rs` (type: initial_data/change, subscription_id, change_type, rows, old_values, new_values)
- [X] T049 [US0] Implement POST `/api/sql` handler in `backend/crates/kalamdb-api/src/handlers/sql_handler.rs` (parse SQL, execute with DataFusion, return results)
- [X] T050 [US0] Add multiple statement execution support in sql_handler.rs (split by semicolon, execute in sequence, aggregate results)
- [X] T051 [US0] Implement WebSocket endpoint `/ws` in `backend/crates/kalamdb-api/src/handlers/ws_handler.rs` (accept connection, handle subscription array)
- [X] T052 [US0] Add initial data fetch logic in ws_handler.rs (execute "last N rows" query, send initial_data message) - TODO comment added for full implementation in Phase 6
- [X] T053 [US0] Create WebSocket session actor in `backend/crates/kalamdb-api/src/actors/ws_session.rs` using Actix (manage connection lifecycle, handle multiple subscriptions)
- [X] T054 [US0] Implement subscription registration in ws_session.rs (parse subscription queries, validate SQL, register in live query manager) - Basic parsing implemented, TODO for live query manager integration
- [X] T055 [US0] Add change notification delivery in ws_session.rs (receive changes from live query manager, format as WebSocket messages, send to client) - Handler implemented, TODO for live query manager
- [X] T056 [US0] Implement error handling and HTTP status codes in sql_handler.rs (400 Bad Request for invalid SQL, 500 Internal Server Error)
- [X] T057 [US0] Add CORS configuration in `backend/crates/kalamdb-server/src/main.rs` (allow web browser clients)
- [X] T058 [US0] Update routes configuration in `backend/crates/kalamdb-api/src/routes.rs` (add /api/sql POST route and /ws WebSocket route)
- [X] T059 [US0] Remove old /api/v1/query endpoint and related code from routes.rs (already removed in Phase 1)

**Checkpoint**: ✅ **COMPLETE** - REST API and WebSocket interface functional - can execute SQL and establish live subscriptions. Note: Full live query manager integration deferred to Phase 6 (User Story 2a).

**Phase 3 Status**: ✅ **COMPLETE** - All 15 tasks completed. Project compiles successfully. REST API (`/api/sql`) and WebSocket (`/ws`) endpoints implemented with basic functionality. Full live query integration to be completed in Phase 6.

---

## Phase 4: User Story 1 - Namespace Management (Priority: P1)

**Goal**: Enable creation, listing, editing, and deletion of namespaces as the foundational organizational structure

**Independent Test**: Create namespace, list it, edit options, verify catalog, attempt to drop with and without tables

### Implementation for User Story 1

- [ ] T060 [P] [US1] Implement CREATE NAMESPACE parser in `backend/crates/kalamdb-core/src/sql/ddl/create_namespace.rs` (parse CREATE NAMESPACE name syntax)
- [ ] T061 [P] [US1] Implement SHOW NAMESPACES parser in `backend/crates/kalamdb-core/src/sql/ddl/show_namespaces.rs` 
- [ ] T062 [P] [US1] Implement ALTER NAMESPACE parser in `backend/crates/kalamdb-core/src/sql/ddl/alter_namespace.rs` (parse SET OPTIONS clause)
- [ ] T063 [P] [US1] Implement DROP NAMESPACE parser in `backend/crates/kalamdb-core/src/sql/ddl/drop_namespace.rs`
- [ ] T064 [US1] Create namespace service in `backend/crates/kalamdb-core/src/services/namespace_service.rs` (create, list, update, delete operations, use NamespaceId type)
- [ ] T065 [US1] Add namespace existence validation in namespace_service.rs (prevent duplicate names)
- [ ] T066 [US1] Add table count check before DROP NAMESPACE in namespace_service.rs (prevent deletion if tables exist, return error with table list)
- [ ] T067 [US1] Implement namespace creation in namespace_service.rs (create namespace entry, create schema directory structure /conf/{namespace}/)
- [ ] T068 [US1] Register DDL executors in `backend/crates/kalamdb-core/src/sql/executor.rs` (CREATE/ALTER/DROP/SHOW NAMESPACE)
- [ ] T069 [US1] Add namespace context to DataFusion session in datafusion_session.rs (track current NamespaceId for table operations)

**Checkpoint**: Namespace management fully functional - can create, list, edit, delete namespaces

---

## Phase 5: User Story 2 - System Tables and User Management (Priority: P1)

**Goal**: Manage users through system.users table for user tracking

**Independent Test**: Insert users into system.users, query user information

### Implementation for User Story 2

- [ ] T070 [P] [US2] Implement system.users table provider in `backend/crates/kalamdb-core/src/tables/system/users_provider.rs` (TableProvider backed by RocksDB column family system_table:users, use UserId type)
- [ ] T073 [US2] Implement CURRENT_USER() function in `backend/crates/kalamdb-core/src/sql/functions/current_user.rs` (resolve from session context, return UserId)
- [ ] T076 [US2] Add user authentication context to DataFusion session in datafusion_session.rs (track current UserId for CURRENT_USER())
- [ ] T077 [US2] Implement INSERT/UPDATE/DELETE operations on system.users via SQL in users_provider.rs

**Checkpoint**: User management functional - can add users, track current user context

---

## Phase 6: User Story 2a - Live Query Monitoring via System Table (Priority: P1)

**Goal**: Monitor active live query subscriptions through system.live_queries table

**Independent Test**: Create live subscriptions, query system.live_queries to see subscription details, verify disconnect cleanup

### Implementation for User Story 2a

- [ ] T080 [P] [US2a] Implement system.live_queries table provider in `backend/crates/kalamdb-core/src/tables/system/live_queries_provider.rs` (TableProvider backed by RocksDB column family system_table:live_queries)
- [ ] T081 [US2a] Create in-memory WebSocket connection registry in `backend/crates/kalamdb-core/src/live_query/connection_registry.rs` (struct UserConnectionSocket { connection_id: ConnectionId, actor: Addr<WebSocketSession>, live_queries: HashMap<LiveId, LiveQuery> }, struct UserConnections { sockets: HashMap<ConnectionId, UserConnectionSocket> }, HashMap<UserId, UserConnections>, store current node_id)
- [ ] T082 [US2a] Create live query manager in `backend/crates/kalamdb-core/src/live_query/manager.rs` (coordinates subscriptions, change detection, and actor notifications)
- [ ] T083 [US2a] Add subscription registration in live query manager (on WebSocket connect: generate ConnectionId { user_id, unique_conn_id }; on subscribe: parse SQL to extract table_name, generate live_id = LiveId { connection_id, table_name, query_id }, serialize options to JSON, register in system.live_queries with live_id/connection_id/table_name/query_id/user_id/query/options/created_at/updated_at/changes=0/node, add to UserConnectionSocket.live_queries HashMap)
- [ ] T084 [US2a] Implement multi-subscription support per WebSocket connection (each connection can have multiple live_ids with different query_ids and table_names, track in UserConnectionSocket.live_queries HashMap keyed by LiveId)
- [ ] T085 [US2a] Add subscription cleanup in live query manager (on disconnect: lookup UserConnectionSocket by connection_id, collect all live_ids from live_queries.keys(), delete from system.live_queries WHERE connection_id, remove socket from UserConnections.sockets HashMap)
- [ ] T086 [US2a] Implement changes counter tracking (increment system.live_queries.changes field on each notification delivery, update updated_at timestamp)
- [ ] T087 [US2a] Add node-aware notification delivery (extract user_id from RocksDB key, check registry.users.get(&user_id), loop over sockets and live_queries, filter by table_name from LiveId, send message to actor with query_id from LiveId)
- [ ] T089 [US2a] Implement KILL LIVE QUERY command parser in `backend/crates/kalamdb-core/src/sql/ddl/kill_live_query.rs` (parse KILL LIVE QUERY live_id string, convert to LiveId struct, extract UserId)
- [ ] T090 [US2a] Add kill live query execution in live query manager (parse live_id to extract UserId and ConnectionId, lookup UserConnectionSocket, remove LiveId from live_queries HashMap, send disconnect message to actor, delete from system.live_queries)

**Checkpoint**: Live query monitoring functional - can view and kill active subscriptions via SQL

---

## Phase 7: User Story 2b - Storage Location Management via System Table (Priority: P1)

**Goal**: Manage predefined storage locations through system.storage_locations table

**Independent Test**: Insert storage locations, create tables using LOCATION REFERENCE, verify usage tracking

### Implementation for User Story 2b

- [ ] T099 [P] [US2b] Implement system.storage_locations table provider in `backend/crates/kalamdb-core/src/tables/system/storage_locations_provider.rs` (TableProvider backed by RocksDB column family system_table:storage_locations, use UserId type)
- [ ] T100 [US2b] Create storage location service in `backend/crates/kalamdb-core/src/services/storage_location_service.rs` (add, update, delete, resolve location, use UserId type)
- [ ] T099 [US2b] Add location name uniqueness validation in storage_location_service.rs (per UserId)
- [ ] T100 [US2b] Implement usage count tracking in storage_location_service.rs (increment when table created, decrement when table dropped)
- [ ] T099 [US2b] Add deletion prevention for referenced locations in storage_location_service.rs (check usage_count > 0, return error with dependent TableName references)
- [ ] T100 [US2b] Implement INSERT/UPDATE/DELETE operations on system.storage_locations via SQL (use UserId type)
- [ ] T099 [US2b] Add location accessibility validation in storage_location_service.rs (test filesystem/S3 path before adding, use UserId in path template)

**Checkpoint**: Storage location management functional - can predefine locations, track usage, prevent deletion of referenced locations

---

## Phase 8: User Story 2c - Job Monitoring via System Table (Priority: P1)

**Goal**: Monitor active and historical jobs (flush, cleanup, scheduled) through system.jobs table

**Independent Test**: Trigger flush jobs, query system.jobs to see job details, verify metrics recording

### Implementation for User Story 2c

- [ ] T099 [P] [US2c] Implement system.jobs table provider in `backend/crates/kalamdb-core/src/tables/system/jobs_provider.rs` (TableProvider backed by RocksDB column family system_table:jobs, use TableName type)
- [ ] T100 [US2c] Create job execution framework in `backend/crates/kalamdb-core/src/jobs/executor.rs` (execute jobs, track metrics, update status, use TableName type)
- [ ] T101 [US2c] Add job registration in job executor (create job record with status='running' when job starts, use TableName)
- [ ] T102 [US2c] Add job completion recording in job executor (update status='completed'/'failed', record end_time, result, trace, metrics, use TableName)
- [ ] T103 [US2c] Implement resource usage tracking in job executor (measure memory_used_mb and cpu_used_percent during execution)
- [ ] T104 [US2c] Add job parameters serialization in job executor (store job inputs as array of strings)
- [ ] T105 [US2c] Add job result and trace recording in job executor (capture job outcome and execution context)
- [ ] T106 [US2c] Implement job retention policy in `backend/crates/kalamdb-core/src/jobs/retention.rs` (cleanup jobs older than configurable period)
- [ ] T108 [US2c] Add node_id tracking in job executor (identify which node executed the job)

**Checkpoint**: Job monitoring functional - can track jobs, view metrics, enforce retention policies

---

## Phase 9: User Story 3 - User Table Creation and Management (Priority: P1)

**Goal**: Create user-scoped tables with isolated storage per user ID, auto-increment fields, flush policies, system columns, and soft delete support

**Independent Test**: Create user table, insert data for different users, verify data isolation, test flush policies

### Implementation for User Story 3

- [ ] T109 [P] [US3] Implement CREATE USER TABLE parser in `backend/crates/kalamdb-core/src/sql/ddl/create_user_table.rs` (parse schema, LOCATION clause, LOCATION REFERENCE, FLUSH POLICY, deleted_retention, use NamespaceId and TableName types)
- [ ] T110 [US3] Create user table service in `backend/crates/kalamdb-core/src/services/user_table_service.rs` (create table metadata, register in catalog, create column family, use NamespaceId and TableName types)
- [ ] T111 [US3] Add auto-increment field injection in user_table_service.rs (add snowflake ID field if not specified)
- [ ] T112 [US3] Add system column injection in user_table_service.rs (\_updated TIMESTAMP, \_deleted BOOLEAN for user tables)
- [ ] T112a [US3] Prevent system column injection for stream tables (stream tables do NOT have \_updated or \_deleted columns - this validation added in stream table service, use TableType enum)
- [ ] T113 [US3] Implement storage location resolution in user_table_service.rs (resolve LOCATION REFERENCE or validate LOCATION path template, use UserId in path)
- [ ] T114 [US3] Create schema file for user table in user_table_service.rs (generate schema_v1.json using DataFusion's SchemaRef::to_json(), update manifest.json, create current.json symlink, use NamespaceId and TableName in path)
- [ ] T115 [US3] Create column family for user table in user_table_service.rs (use column_family_manager to create user_table:{NamespaceId}:{TableName} using TableType enum)
- [ ] T116 [US3] Implement user table INSERT handler in `backend/crates/kalamdb-core/src/tables/user_table_insert.rs` (write to RocksDB column family with key format {UserId}:{row_id}, set \_updated = NOW(), \_deleted = false)
- [ ] T117 [US3] Implement user table UPDATE handler in `backend/crates/kalamdb-core/src/tables/user_table_update.rs` (update in RocksDB column family, set \_updated = NOW(), use UserId type)
- [ ] T118 [US3] Implement user table DELETE handler (soft delete) in `backend/crates/kalamdb-core/src/tables/user_table_delete.rs` (set \_deleted = true, \_updated = NOW(), use UserId type)
- [ ] T119 [US3] Create user table provider for DataFusion in `backend/crates/kalamdb-core/src/tables/user_table_provider.rs` (register table, provide schema, handle queries, use NamespaceId and TableName types)
- [ ] T120 [US3] Implement user ID path substitution in user_table_provider.rs (replace ${user_id} with UserId in storage paths)
- [ ] T121 [US3] Add data isolation enforcement in user_table_provider.rs (queries only access current user's data by filtering on UserId key prefix)
- [ ] T122 [US3] Implement flush trigger logic in `backend/crates/kalamdb-core/src/flush/trigger.rs` (monitor row count and time intervals per column family, use TableName type)
- [ ] T123 [US3] Create flush job for user tables in `backend/crates/kalamdb-core/src/flush/user_table_flush.rs` (iterate column family, group rows by UserId prefix, write separate Parquet file per user at ${UserId}/batch-*.parquet, delete flushed rows from RocksDB)
- [ ] T124 [US3] Add deleted_retention configuration to table metadata in user_table_service.rs (use TableName type)
- [ ] T125 [US3] Register flush jobs in system.jobs table (status, metrics, result, trace, use TableName type)

**Checkpoint**: User table creation and basic operations functional - can create tables, insert/update/delete data with isolation

---

## Phase 10: User Story 3a - Table Deletion and Cleanup (Priority: P1)

**Goal**: Drop user and shared tables with cleanup of RocksDB buffers, Parquet files, and metadata

**Independent Test**: Create table with data, drop it, verify all data and metadata removed, test prevention when subscriptions exist

### Implementation for User Story 3a

- [ ] T126 [P] [US3a] Implement DROP TABLE parser in `backend/crates/kalamdb-core/src/sql/ddl/drop_table.rs` (parse DROP USER TABLE and DROP SHARED TABLE, use NamespaceId and TableName types)
- [ ] T127 [US3a] Create table deletion service in `backend/crates/kalamdb-core/src/services/table_deletion_service.rs` (orchestrate cleanup, use NamespaceId and TableName types)
- [ ] T128 [US3a] Add active subscription check in table_deletion_service.rs (query system.live_queries, prevent drop if active subscriptions exist, use TableName)
- [ ] T129 [US3a] Implement RocksDB buffer cleanup in table_deletion_service.rs (delete entire column family for table: user_table:{NamespaceId}:{TableName} or shared_table:{NamespaceId}:{TableName} using TableType enum)
- [ ] T130 [US3a] Implement Parquet file deletion in table_deletion_service.rs (delete all user-specific Parquet files from storage, use UserId in path)
- [ ] T131 [US3a] Add metadata removal in table_deletion_service.rs (remove from manifest.json, delete schema directory, use NamespaceId and TableName)
- [ ] T132 [US3a] Update storage location usage count in table_deletion_service.rs (decrement usage_count when table references location)
- [ ] T133 [US3a] Add error handling for partial failures in table_deletion_service.rs (rollback if file deletion fails)
- [ ] T134 [US3a] Register DROP TABLE operations as jobs in system.jobs (track cleanup progress, use TableName)

**Checkpoint**: Table deletion functional - can drop tables with complete cleanup, prevent deletion when in use

---

## Phase 11: User Story 3b - Table Schema Evolution (ALTER TABLE) (Priority: P2)

**Goal**: Modify table schemas after creation with ADD/DROP/MODIFY COLUMN, preserve existing data, maintain backwards compatibility

**Independent Test**: Create table with data, alter schema (add/drop columns), verify queries work with both old and new data

### Implementation for User Story 3b

- [ ] T135 [P] [US3b] Implement ALTER TABLE parser in `backend/crates/kalamdb-core/src/sql/ddl/alter_table.rs` (parse ADD COLUMN, DROP COLUMN, MODIFY COLUMN, use NamespaceId and TableName types)
- [ ] T136 [US3b] Create schema evolution service in `backend/crates/kalamdb-core/src/services/schema_evolution_service.rs` (orchestrate schema changes, use NamespaceId and TableName types)
- [ ] T137 [US3b] Add ALTER TABLE validation in schema_evolution_service.rs (check backwards compatibility, validate type changes)
- [ ] T138 [US3b] Add subscription column reference check in schema_evolution_service.rs (prevent dropping columns referenced in active subscriptions, use TableName)
- [ ] T139 [US3b] Prevent altering system columns in schema_evolution_service.rs (reject changes to \_updated, \_deleted)
- [ ] T140 [US3b] Prevent altering stream tables in schema_evolution_service.rs (stream table schemas are immutable, use TableType enum)
- [ ] T141 [US3b] Implement schema version increment in schema_evolution_service.rs (create schema_v{N+1}.json using DataFusion's SchemaRef::to_json())
- [ ] T142 [US3b] Update manifest.json in schema_evolution_service.rs (update current_version, updated_at timestamp)
- [ ] T143 [US3b] Update current.json symlink in schema_evolution_service.rs (point to new schema version)
- [ ] T144 [US3b] Invalidate schema cache in schema_evolution_service.rs (force DataFusion to reload schema, use NamespaceId and TableName as cache key)
- [ ] T145 [US3b] Implement schema projection for old Parquet files in `backend/crates/kalamdb-core/src/schema/projection.rs` (fill missing columns with NULL/DEFAULT)
- [ ] T146 [US3b] Add DESCRIBE TABLE enhancement to show schema history in `backend/crates/kalamdb-core/src/sql/ddl/describe_table.rs` (use NamespaceId and TableName)

**Checkpoint**: Schema evolution functional - can alter tables, queries work across schema versions

---

## Phase 12: User Story 4a - Stream Table Creation for Ephemeral Events (Priority: P1)

**Goal**: Create stream tables for transient events with TTL, ephemeral mode, max_buffer, memory-only storage

**Independent Test**: Create stream table, insert events, subscribe, verify real-time delivery without disk persistence

### Implementation for User Story 4a

- [ ] T147 [P] [US4a] Implement CREATE STREAM TABLE parser in `backend/crates/kalamdb-core/src/sql/ddl/create_stream_table.rs` (parse schema, retention, ephemeral, max_buffer, use NamespaceId and TableName types)
- [ ] T148 [US4a] Create stream table service in `backend/crates/kalamdb-core/src/services/stream_table_service.rs` (create table metadata, register in catalog, create column family, NO system columns, use NamespaceId and TableName types, set TableType::Stream)
- [ ] T149 [US4a] Create stream table provider in `backend/crates/kalamdb-core/src/tables/stream_table_provider.rs` (memory/RocksDB-only, no Parquet, use NamespaceId and TableName)
- [ ] T150 [US4a] Create column family for stream table in stream_table_service.rs (use column_family_manager to create stream_table:{NamespaceId}:{TableName} using TableType::Stream enum)
- [ ] T151 [US4a] Implement stream table INSERT handler in `backend/crates/kalamdb-core/src/tables/stream_table_insert.rs` (write to RocksDB column family with key format {timestamp_ms}:{row_id}, check ephemeral mode)
- [ ] T152 [US4a] Add ephemeral mode check in stream_table_insert.rs (discard events if no subscribers when ephemeral=true)
- [ ] T153 [US4a] Implement TTL-based eviction in `backend/crates/kalamdb-core/src/tables/stream_table_eviction.rs` (background job removes entries with timestamp older than retention period, use TableName)
- [ ] T154 [US4a] Implement max_buffer eviction in stream_table_eviction.rs (evict oldest entries by timestamp prefix when buffer full)
- [ ] T155 [US4a] Add real-time event delivery to subscribers in stream_table_provider.rs (< 5ms latency)
- [ ] T156 [US4a] Prevent Parquet flush for stream tables in flush trigger logic (check TableType enum, skip TableType::Stream)
- [ ] T157 [US4a] Add stream table metadata to DESCRIBE TABLE output (use NamespaceId and TableName)
- [ ] T158 [US4a] Implement DROP STREAM TABLE support in drop_table.rs (delete column family stream_table:{NamespaceId}:{TableName} using TableType::Stream enum)

**Checkpoint**: Stream tables functional - can create ephemeral tables, insert events, deliver real-time without persistence

---

## Phase 13: User Story 5 - Shared Table Creation and Management (Priority: P1)

**Goal**: Create shared tables accessible to all users in namespace with single storage location, flush policies, system columns

**Independent Test**: Create shared table, insert data from different users, verify all users see same data

### Implementation for User Story 5

- [ ] T159 [P] [US5] Implement CREATE SHARED TABLE parser in `backend/crates/kalamdb-core/src/sql/ddl/create_shared_table.rs` (parse schema, LOCATION, FLUSH POLICY, deleted_retention, use NamespaceId and TableName types)
- [ ] T160 [US5] Create shared table service in `backend/crates/kalamdb-core/src/services/shared_table_service.rs` (create table metadata, register in catalog, create column family, use NamespaceId and TableName types, set TableType::Shared)
- [ ] T161 [US5] Add system column injection in shared_table_service.rs (\_updated TIMESTAMP, \_deleted BOOLEAN for shared tables)
- [ ] T162 [US5] Create column family for shared table in shared_table_service.rs (use column_family_manager to create shared_table:{NamespaceId}:{TableName} using TableType::Shared enum)
- [ ] T163 [US5] Implement shared table INSERT/UPDATE/DELETE handlers in `backend/crates/kalamdb-core/src/tables/shared_table_ops.rs` (write to RocksDB column family with key format {row_id}, update system columns)
- [ ] T164 [US5] Create shared table provider for DataFusion in `backend/crates/kalamdb-core/src/tables/shared_table_provider.rs` (single storage location, no UserId templating, use NamespaceId and TableName)
- [ ] T166 [US5] Create flush job for shared tables in `backend/crates/kalamdb-core/src/flush/shared_table_flush.rs` (read all rows from column family, write to SINGLE Parquet file at shared/{TableName}/batch-*.parquet, delete flushed rows from RocksDB)
- [ ] T167 [US5] Add shared table support to DROP TABLE command (delete column family shared_table:{NamespaceId}:{TableName} using TableType::Shared enum)

**Checkpoint**: Shared tables functional - can create global tables, flush to storage

---

## Phase 14: User Story 6 - Live Query Subscriptions with Change Tracking (Priority: P2)

**Goal**: WebSocket subscriptions with filtered queries, initial data fetch, and real-time INSERT/UPDATE/DELETE notifications

**Independent Test**: Subscribe via WebSocket, perform CRUD operations, verify subscriber receives all matching change notifications

### Implementation for User Story 6

- [ ] T168 [P] [US6] Create change notification generator in `backend/crates/kalamdb-core/src/live_query/change_detector.rs` (detect INSERT/UPDATE/DELETE from RocksDB writes, use TableName type)
- [ ] T169 [US6] Implement filter matching for subscriptions in change_detector.rs (evaluate WHERE clause against changed rows)
- [ ] T170 [US6] Add subscription filter compilation in live query manager (parse WHERE clause, create matcher)
- [ ] T171 [US6] Implement INSERT notification in change_detector.rs (send new row values with change_type='INSERT')
- [ ] T172 [US6] Implement UPDATE notification in change_detector.rs (send old and new row values with change_type='UPDATE')
- [ ] T173 [US6] Implement DELETE notification in change_detector.rs (send deleted row with \_deleted=true, change_type='DELETE')
- [ ] T174 [US6] Add change notification on flush completion in flush jobs (notify subscribers after Parquet write, use TableName)
- [ ] T175 [US6] Implement "changes since timestamp" query using \_updated column in `backend/crates/kalamdb-core/src/live_query/initial_data.rs`
- [ ] T176 [US6] Add subscription isolation per UserId in live query manager (users only subscribe to their own data)
- [ ] T177 [US6] Optimize change detection using \_updated and \_deleted columns

**Checkpoint**: Live query subscriptions with full CDC functional - real-time change tracking for all operations

---

## Phase 15: User Story 7 - Namespace Backup and Restore (Priority: P3)

**Goal**: Backup entire namespaces including schemas, data, metadata, and restore them

**Independent Test**: Backup namespace with data, drop it, restore from backup, verify all data recovered

### Implementation for User Story 7

- [ ] T178 [P] [US7] Implement BACKUP DATABASE parser in `backend/crates/kalamdb-core/src/sql/ddl/backup_namespace.rs` (use NamespaceId type)
- [ ] T179 [P] [US7] Implement RESTORE DATABASE parser in `backend/crates/kalamdb-core/src/sql/ddl/restore_namespace.rs` (use NamespaceId type)
- [ ] T180 [P] [US7] Implement SHOW BACKUP parser in `backend/crates/kalamdb-core/src/sql/ddl/show_backup.rs` (use NamespaceId type)
- [ ] T181 [US7] Create backup service in `backend/crates/kalamdb-core/src/services/backup_service.rs` (orchestrate backup operations, use NamespaceId and TableName types)
- [ ] T182 [US7] Implement manifest.json backup in backup_service.rs (save schema versions and table metadata, include NamespaceId and TableName)
- [ ] T183 [US7] Implement Parquet file backup in backup_service.rs (copy all Parquet files to backup location, use UserId in paths)
- [ ] T184 [US7] Include soft-deleted rows in backup (\_deleted=true rows preserved for change history)
- [ ] T185 [US7] Exclude stream tables from backup in backup_service.rs (ephemeral data not persisted, check TableType::Stream enum)
- [ ] T186 [US7] Create restore service in `backend/crates/kalamdb-core/src/services/restore_service.rs` (restore schemas, tables, data, use NamespaceId and TableName types)
- [ ] T187 [US7] Add backup verification in restore_service.rs (validate backup integrity before restore)
- [ ] T188 [US7] Register backup/restore jobs in system.jobs table (use NamespaceId type)

**Checkpoint**: Backup and restore functional - can backup namespaces, restore with data integrity

---

## Phase 16: User Story 8 - Table and Namespace Catalog Browsing (Priority: P2)

**Goal**: Browse database structure with SQL-like catalog queries, list namespaces/tables, inspect schemas

**Independent Test**: Create namespaces and tables, use catalog queries to list and inspect them

### Implementation for User Story 8

- [ ] T189 [P] [US8] Implement SHOW TABLES parser in `backend/crates/kalamdb-core/src/sql/ddl/show_tables.rs` (use NamespaceId type)
- [ ] T190 [P] [US8] Implement DESCRIBE TABLE parser (already started in T146, enhance here, use NamespaceId and TableName types)
- [ ] T191 [US8] Create information_schema.tables virtual table in `backend/crates/kalamdb-core/src/tables/system/information_schema_tables.rs` (use NamespaceId and TableName types)
- [ ] T192 [US8] Add table type indicator in SHOW TABLES output (use TableType enum: User, Shared, Stream, System)
- [ ] T193 [US8] Add storage location to DESCRIBE TABLE output (use NamespaceId and TableName)
- [ ] T194 [US8] Add flush policy to DESCRIBE TABLE output
- [ ] T195 [US8] Add stream configuration (retention, ephemeral, max_buffer) to DESCRIBE TABLE for stream tables (filter by TableType::Stream)
- [ ] T196 [US8] Add system columns (\_updated, \_deleted) to DESCRIBE TABLE output (exclude for TableType::Stream)
- [ ] T197 [US8] Add schema version and history to DESCRIBE TABLE output (current version, schema file paths)
- [ ] T198 [US8] Implement table statistics query in `backend/crates/kalamdb-core/src/sql/ddl/show_table_stats.rs` (row counts, storage size, use NamespaceId and TableName types)

**Checkpoint**: Catalog browsing functional - can discover and inspect database structure via SQL

---

## Phase 17: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

### Configuration and Deployment

- [ ] T199 [P] [Polish] Update server configuration in `backend/crates/kalamdb-server/src/config.rs` (add DataFusion config, flush policy defaults, retention policies, RocksDB column family settings)
- [ ] T200 [P] [Polish] Create example configuration file `backend/conf/config.example.toml` with all new settings documented (note: server config files live in backend/conf/ alongside namespaces.json and storage_locations.json)
- [ ] T201 [P] [Polish] Add environment variable support for sensitive config (S3 credentials, etc.)

### Error Handling and Logging

- [ ] T202 [P] [Polish] Enhance error types in `backend/crates/kalamdb-core/src/error.rs` (add TableNotFound, PermissionDenied, SchemaEvolutionError, ColumnFamilyError, etc.)
- [ ] T203 [P] [Polish] Add structured logging for all operations (namespace CRUD, table CRUD, flush jobs, schema evolution, column family operations)
- [ ] T204 [P] [Polish] Add request/response logging for REST API and WebSocket connections

### Performance Optimization

- [ ] T205 [P] [Polish] Add RocksDB connection management in column_family_manager (connection pooling, reuse)
- [ ] T206 [P] [Polish] Implement schema cache in DataFusion session factory (avoid repeated manifest.json reads, use NamespaceId and TableName as cache key)
- [ ] T207 [P] [Polish] Add query result caching for system table queries (catalog, live_queries, jobs, use TableName type)
- [ ] T208 [P] [Polish] Optimize Parquet bloom filter generation for \_updated column
- [ ] T209 [P] [Polish] Add metrics collection (query latency, flush job duration, WebSocket message throughput, column family sizes, use TableName type)

### Security and Validation

- [ ] T210 [P] [Polish] Add SQL injection prevention (use parameterized queries in DataFusion)
- [ ] T211 [P] [Polish] Add WebSocket authentication and authorization (use UserId type)
- [ ] T212 [P] [Polish] Add rate limiting for REST API and WebSocket connections (per UserId)

### Documentation

- [ ] T214 [P] [Polish] Update README.md with new architecture overview and feature list (include RocksDB column family architecture)
- [ ] T215 [P] [Polish] Create API documentation for REST endpoint `/api/sql` with examples
- [ ] T216 [P] [Polish] Create WebSocket protocol documentation for `/ws` endpoint with subscription examples
- [ ] T217 [P] [Polish] Document SQL syntax for all DDL commands (CREATE/ALTER/DROP NAMESPACE, CREATE USER/SHARED/STREAM TABLE, etc.)
- [ ] T218 [P] [Polish] Add rustdoc comments to all public APIs (modules, structs, functions) ensuring 100% coverage for kalamdb-core public API, kalamdb-api handlers, and all service interfaces
- [ ] T219 [P] [Polish] Create Architecture Decision Records (ADRs) in `docs/backend/adrs/` for key design choices (table-per-user architecture, DataFusion integration, soft deletes, RocksDB column families, JSON config files, in-memory registry, Parquet bloom filters, JWT authentication) using markdown template with Context/Decision/Consequences sections

### Testing Support

- [ ] T220 [P] [Polish] Create integration test framework setup in `backend/tests/integration/common/mod.rs` (test harness, server lifecycle, cleanup utilities)
- [ ] T221 [P] [Polish] Add namespace/table test utilities in `backend/tests/integration/common/fixtures.rs` (create/cleanup helpers, sample data generators)
- [ ] T222 [P] [Polish] Add WebSocket test utilities in `backend/tests/integration/common/websocket.rs` (connection helpers, subscription matchers, change notification validators)
- [ ] T227 [P] [Polish] Create automated test script from quickstart.md in `backend/tests/quickstart.sh` (bash script that runs all steps from quickstart guide: server startup, namespace/table creation, REST API queries, WebSocket subscriptions, live query notifications)
- [ ] T228 [P] [Polish] Create benchmark suite in `backend/benches/` using criterion.rs (benchmark RocksDB writes, DataFusion queries, WebSocket message delivery, flush operations, measure <1ms write latency and <10ms notification latency)
- [ ] T229 [P] [Polish] Create end-to-end integration test in `backend/tests/integration/test_quickstart.rs` (implement all scenarios from quickstart.md as automated tests: setup, REST API, WebSocket, live queries, system tables, performance validation)

### Code Cleanup

- [ ] T223 [Polish] Remove all old message-centric code remnants
- [ ] T224 [Polish] Update Cargo.toml dependencies (remove unused, add missing)
- [ ] T225 [Polish] Run `cargo fmt` and `cargo clippy` across all crates
- [ ] T226 [Polish] Audit and update error messages for clarity

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - **BLOCKS all user stories**
- **User Story 0 (Phase 3)**: Depends on Foundational - REST API and WebSocket foundation
- **User Stories 1-8 (Phases 4-16)**: All depend on Foundational and US0 completion
  - US1 (Namespace): Independent, can start after US0
  - US2 (User Management): Independent, can start after US0
  - US2a (Live Queries Table): Depends on US2, US6 for live query manager
  - US2b (Storage Locations): Independent, can start after US0
  - US2c (Jobs Table): Independent, can start after US0
  - US3 (User Tables): Depends on US1 (namespaces), US2b (storage locations)
  - US3a (Table Deletion): Depends on US3, US2a (check active subscriptions)
  - US3b (ALTER TABLE): Depends on US3
  - US4a (Stream Tables): Depends on US1 (namespaces)
  - US5 (Shared Tables): Depends on US1 (namespaces)
  - US6 (Live Subscriptions): Depends on US3 (user tables), US0 (WebSocket)
  - US7 (Backup/Restore): Depends on US1, US3, US5 (all table types)
  - US8 (Catalog): Depends on US1, US3, US4a, US5 (all table types)
- **Polish (Phase 17)**: Depends on all desired user stories being complete

### Critical Path for MVP (P1 Features Only)

1. **Phase 1**: Setup & Code Removal
2. **Phase 2**: Foundational (blocking)
3. **Phase 3**: US0 - REST API & WebSocket
4. **Phase 4**: US1 - Namespace Management
5. **Phase 5**: US2 - User Management
6. **Phase 6**: US2a - Live Query Monitoring
7. **Phase 7**: US2b - Storage Locations
8. **Phase 8**: US2c - Job Monitoring
9. **Phase 9**: US3 - User Table Creation
10. **Phase 10**: US3a - Table Deletion
11. **Phase 12**: US4a - Stream Tables
12. **Phase 13**: US5 - Shared Tables
13. **Phase 17**: Polish (subset for MVP)

**Checkpoint**: MVP complete with all P1 features

### Parallel Opportunities

- **Within Setup (Phase 1)**: All tasks marked [P] can run in parallel (T001-T006, T008-T013)
- **Within Foundational (Phase 2)**: Many tasks marked [P] can run in parallel within subsections
- **After Foundational**: US1, US2, US2b, US2c can start in parallel
- **After US1**: US3, US4a, US5 can start in parallel (namespace-dependent)
- **Different developers**: Can work on independent user stories simultaneously

---

## Parallel Example: After Foundational Phase

```bash
# Developer A works on US1 (Namespace Management)
Tasks T059-T068 (namespace DDL and service)

# Developer B works on US2 (User Management)
Tasks T070, T073, T076-T077 (system.users and user context)

# Developer C works on US2b (Storage Locations)
Tasks T088-T095 (system.storage_locations)

# All three developers can work in parallel since they touch different files
```

---

## Implementation Strategy

### MVP First (P1 Features)

1. Complete **Phase 1**: Setup & Code Removal (clean slate)
2. Complete **Phase 2**: Foundational (CRITICAL - blocks everything)
3. Complete **Phase 3**: US0 - REST API & WebSocket (essential interface)
4. Complete **Phases 4-13**: All P1 user stories (core functionality)
5. **STOP and VALIDATE**: Test all P1 features independently
6. Complete **Phase 17**: Polish (MVP subset - error handling, basic docs, config)
7. Deploy/demo MVP

### Incremental Delivery (After MVP)

1. Add **Phase 11**: US3b - ALTER TABLE (P2 feature)
2. Add **Phase 14**: US6 - Live Query Subscriptions (P2 feature)
3. Add **Phase 16**: US8 - Catalog Browsing (P2 feature)
4. Add **Phase 15**: US7 - Backup/Restore (P3 feature)
5. Each addition is tested independently before moving to next

### Parallel Team Strategy

With 3-4 developers after Foundational phase:

1. **Developer A**: US0 (REST/WebSocket) → US1 (Namespaces) → US3 (User Tables) → US6 (Live Queries)
2. **Developer B**: US2 (Users/Permissions) → US2a (Live Query Monitoring) → US5 (Shared Tables)
3. **Developer C**: US2b (Storage Locations) → US2c (Jobs) → US4a (Stream Tables) → US7 (Backup)
4. **Developer D**: US3a (Table Deletion) → US3b (ALTER TABLE) → US8 (Catalog) → Polish

---

## Notes

### Key Implementation Guidelines

- **Code Removal First**: Phase 1 removes old message-centric code to avoid conflicts
- **DataFusion-First**: All SQL parsing and execution goes through DataFusion (except custom DDL)
- **Arrow-Native**: All schemas stored in Arrow JSON format for zero-copy integration
- **System Columns**: \_updated and \_deleted automatically added to user/shared tables (NOT stream tables)
- **Soft Deletes**: DELETE operations set \_deleted=true (physical deletion deferred to cleanup jobs)
- **User Isolation**: User tables enforce strict user_id-based data isolation
- **Live Query CDC**: Change tracking is built into WebSocket subscriptions (no separate CDC mechanism)
- **Stream Tables**: Memory-only, no Parquet, no system columns, ephemeral by design
- **Job Tracking**: All background operations (flush, cleanup, backup) recorded in system.jobs

### Development Best Practices

- [P] tasks = different files, can run in parallel
- [Story] label maps task to user story for traceability
- Commit after each task or logical group
- Stop at checkpoints to validate story independently
- Use feature branches for each user story
- Write comprehensive rustdoc comments (Constitution Principle VIII)
- Add inline comments for complex algorithms
- Create ADRs for architectural decisions

### Testing Strategy (When Implemented)

While tests are not included in this task list, consider this testing approach when ready:

- **Unit tests**: For services, parsers, and utilities
- **Integration tests**: For DDL operations, data flow, WebSocket subscriptions
- **End-to-end tests**: For complete user journeys across user stories
- **Performance tests**: For flush policies, query latency, WebSocket throughput

---

## Summary

**Total Tasks**: 237 tasks (229 original + 8 additional cleanup tasks from Phase 1)
**Completed Tasks**: 20 tasks (12 core Phase 1 tasks + 8 cleanup tasks)
**Remaining Tasks**: 217 tasks
**P1 Critical Tasks**: ~195 tasks (Phases 1-10, 12-13, and essential Polish including T227-T229)
**P2 Tasks**: ~20 tasks (Phases 11, 14, 16)
**P3 Tasks**: ~6 tasks (Phase 15)

**Progress**: 
- ✅ Phase 1 (Setup & Code Removal): 100% complete (20/20 tasks)
- ⏳ Phase 2 (Foundational): 0% complete (0/44 tasks) - **NEXT PHASE**
- ⏳ Remaining phases: Pending Phase 2 completion

**Parallel Opportunities**: 
- Setup phase: ✅ All 12 parallel tasks completed
- Foundational phase: ~23 parallel tasks (including RocksDB column family architecture)
- User stories: 3-4 stories can proceed in parallel after foundational

**MVP Scope** (P1 only):
- REST API and WebSocket interface
- Namespace management with persistent configuration (conf/namespaces.json)
- User management (basic tracking, no permissions enforcement)
- **System tables**:
  - system.users (basic user tracking with username, email)
  - **system.live_queries** (live_id [composite: connection_id-query_id], connection_id, query_id, user_id, query, created_at, updated_at, changes, node) - supports multiple subscriptions per connection
  - system.storage_locations (predefined storage locations)
  - system.jobs (job monitoring with metrics)
- **In-memory WebSocket connection registry** (HashMap<connection_id, ConnectedWebSocket> where ConnectedWebSocket has actor and live_ids vector, HashMap<live_id, connection_id> for reverse lookup, node-aware delivery for clusters)
- Storage location management with persistent configuration (conf/storage_locations.json)
- **RocksDB column family architecture** (user_table, shared_table, stream_table, system_table column families)
- User tables with flush policies and soft deletes
- Schema versioning with table options persistence
- Configuration persistence in JSON files for server restart capability
- Stream tables for ephemeral events
- Shared tables (accessible to all users - permissions via VIEWS in future)
- Table deletion
- Basic polish (config, errors, logging)

**Key Additions for RocksDB Column Family Architecture**:
- Column family manager for creating/deleting column families with naming conventions
- Column family naming: user_table:{namespace}:{table}, shared_table:{namespace}:{table}, stream_table:{namespace}:{table}, system_table:{table}
- Key format per table type: {user_id}:{row_id} for user tables, {row_id} for shared tables, {timestamp_ms}:{row_id} for stream tables, {live_id} (format: {connection_id}-{query_id}) for live_queries
- User table flush grouping by user_id prefix (separate Parquet per user)
- Shared table flush to single Parquet file
- Stream table timestamp-based eviction
- Per-column-family configuration (memtable, write buffer, WAL, compaction)

**Key Additions for Live Query Architecture**:
- **Connection-based registry**: Each WebSocket gets unique connection_id, stores ConnectedWebSocket { actor, live_ids } in HashMap
- **Composite live_id format**: connection_id + "-" + query_id (e.g., "conn_abc123-messages") enables client-friendly query identification
- **Reverse lookup**: HashMap<live_id, connection_id> for efficient change detection and notification delivery
- **Multi-subscription support**: Each connection can have multiple live_ids with different query_ids, tracked in ConnectedWebSocket.live_ids vector
- **Node-aware delivery**: node field in system.live_queries identifies which cluster node owns the WebSocket connection
- **Change tracking**: changes counter incremented on each notification, updated_at timestamp for last notification time
- **Query ID in notifications**: Clients receive query_id with each change so they know which subscription triggered the event

**Estimated MVP Completion**: Foundational work is substantial but each user story is independently testable, enabling incremental delivery and parallel development.

