# Feature Specification: Simple KalamDB - User-Based Database with Live Queries

**Feature Branch**: `002-simple-kalamdb`  
**Created**: 2025-10-14  
**Status**: Draft  
**Input**: User description: "Simple user-based database with namespaces, user/shared/system tables, live queries with change tracking, and flush policies"

## 🚀 Architectural Differentiator: Table-Per-User Multi-Tenancy

**The Power of Per-User Tables**: Unlike traditional databases that store all users' messages in a shared table, KalamDB uses a **table-per-user architecture** where each user's data lives in isolated storage partitions (`{userId}/batch-*.parquet`). This fundamental design decision unlocks unprecedented scalability:

**Key Advantages**:

1. **Massive Scalability for Real-time Subscriptions**
   - Traditional approach: Complex database triggers on a shared table with billions of rows
   - KalamDB approach: Simple file-level notifications per user partition
   - Result: **Millions of concurrent users** can maintain real-time subscriptions without performance degradation

2. **O(1) Subscription Complexity**
   - Each user's subscription monitors only their own partition
   - Performance independent of total user count
   - No expensive WHERE userId = ? filtering on massive shared tables

3. **Horizontal Scaling**
   - Adding new users creates new isolated partitions
   - No impact on existing user performance
   - Storage and query operations naturally parallelized

4. **Simplified Real-time Implementation**
   - Monitor file changes in user-specific directories
   - Eliminates complex trigger logic on shared database tables
   - Predictable, consistent notification latency regardless of scale

**Example Comparison**:
```
Traditional Shared Table (Redis/Postgres):
- 1M users × 1 subscription = 1M WHERE clauses on shared messages table
- Performance degrades as table grows to billions of rows
- Complex trigger management and connection pooling

KalamDB Table-Per-User:
- 1M users × 1 subscription = 1M independent file watchers
- Each user's partition isolated (typically KB to MB in size)
- Simple, parallel, O(1) complexity per user
```

This architecture is what makes KalamDB capable of supporting millions of concurrent real-time subscriptions while maintaining sub-millisecond write performance and efficient query execution.

## Data Flow Architecture

### Write Path (User Tables & Shared Tables)

KalamDB uses a **two-tier storage architecture** to balance write performance with persistent storage:

1. **Hot Storage (RocksDB Buffer)**:
   - All INSERT/UPDATE/DELETE operations first write to RocksDB
   - RocksDB acts as a fast, in-memory/disk hybrid buffer
   - Enables sub-millisecond write acknowledgments to clients
   - Maintains recent data for fast queries

2. **Cold Storage (Parquet Files)**:
   - Data periodically flushed from RocksDB to Parquet format
   - Parquet files written to configured storage backend (S3 or filesystem)
   - Optimized for analytical queries and long-term storage
   - Immutable once written (append-only model)

**Complete Write Flow**:

```
Client Request (INSERT/UPDATE/DELETE)
    ↓
REST API / WebSocket Handler
    ↓
Write to RocksDB (hot storage)
    ↓
[Acknowledge to client - fast!]
    ↓
Buffered in RocksDB until flush policy triggered:
  - Row limit reached (e.g., 10,000 rows)
  - OR Time interval elapsed (e.g., 5 minutes)
    ↓
Flush Job Starts:
  - Record job in system.jobs table (status='running')
  - Read buffered data from RocksDB
  - Serialize to Parquet format
  - Write to storage backend (S3 or filesystem)
    ↓
For User Tables: ${user_id}/batch-*.parquet
For Shared Tables: shared/table_name/batch-*.parquet
    ↓
Flush Job Completes:
  - Clear flushed data from RocksDB buffer
  - Update system.jobs (status='completed', result, trace, metrics)
  - Notify live query subscribers of changes
```

**Storage Location Examples**:
- User table: `s3://bucket/users/user123/messages/batch-20251015-001.parquet`
- Shared table: `s3://bucket/shared/conversations/batch-20251015-001.parquet`

### Read Path (User Tables & Shared Tables)

Queries execute across **both storage tiers** to provide complete data:

```
Client Query (SELECT)
    ↓
DataFusion Query Planner
    ↓
Query Execution Strategy:
  1. Determine query time range (if WHERE clause has timestamp filter)
  2. Check RocksDB buffer for matching data
     - Always includes most recent writes (not yet flushed)
  3. Check Parquet files using bloom filters
     - _updated timestamp used as bloom filter
     - Only scan Parquet files overlapping query range
  4. DataFusion merges results from both sources:
     - Union data from RocksDB and Parquet
     - Apply WHERE filters across merged dataset
     - Handle _deleted column (exclude rows where _deleted = true)
  5. Sort and deduplicate if needed
    ↓
Apply permission filters (row-level security)
    ↓
Return results to client
```

**Query Optimization with System Columns**:
- **_updated (TIMESTAMP)**: Automatically added to every user/shared table
  - Used as Parquet bloom filter for efficient time-range queries
  - Enables efficient "changes since timestamp" queries
  - Updated on both INSERT and UPDATE operations
  
- **_deleted (BOOLEAN)**: Automatically added to every user/shared table
  - Soft delete mechanism (rows marked deleted, not physically removed)
  - Queries filter out rows where _deleted = true by default
  - Enables change tracking for deletions (subscribers can detect deleted rows)
  - Retention policy determines when deleted rows are physically removed

**Example Query Flow**:
```sql
-- User query
SELECT * FROM messages WHERE created_at > '2025-10-14' AND _deleted = false;

-- Execution:
-- 1. RocksDB: Scan buffer, filter by created_at and _deleted
-- 2. Parquet: Use _updated bloom filter to skip old files
--    Only scan files where _updated overlaps '2025-10-14' or later
-- 3. DataFusion: Merge results, apply WHERE clause
-- 4. Return combined dataset
```

**Live Query Change Detection**:
```sql
-- Live subscription
SUBSCRIBE SELECT * FROM messages WHERE conversation_id = 'conv123';

-- Change tracking uses _updated and _deleted:
-- INSERT: New row with _updated = NOW(), _deleted = false
-- UPDATE: Modified row with _updated = NOW(), _deleted = false
-- DELETE: Row marked with _deleted = true, _updated = NOW()
-- 
-- Subscribers receive:
-- - INSERT events: rows where _deleted = false (new data)
-- - UPDATE events: rows where _deleted = false (modified data)  
-- - DELETE events: rows where _deleted = true (soft deleted)
```

### Stream Tables (Ephemeral Architecture)

Stream tables use a **different architecture** optimized for transient, real-time events:

```
Client INSERT (ephemeral event)
    ↓
Write to RocksDB or in-memory buffer ONLY
    ↓
[Acknowledge to client]
    ↓
Event buffered with TTL:
  - Retention policy: automatic eviction after TTL (e.g., 10s)
  - Max buffer: oldest events evicted when limit reached (e.g., 1000 events)
  - Ephemeral mode: discard immediately if no subscribers
    ↓
Live Query Subscribers:
  - Real-time delivery to active WebSocket connections
  - No disk I/O, memory-only operations
    ↓
Events NEVER flushed to Parquet or persistent storage
    ↓
After retention period: Events automatically deleted from buffer
```

**Key Differences from Persistent Tables**:
- ❌ No Parquet files created
- ❌ No flush jobs (never persisted)
- ❌ No S3/filesystem writes
- ✅ Memory/RocksDB only (hot storage)
- ✅ Automatic TTL-based cleanup
- ✅ Ultra-low latency delivery to subscribers

### Live Query Subscriptions & Change Tracking

Live queries monitor **both storage tiers** for changes:

```
Client WebSocket Subscription
    ↓
Register in system.live_queries table
    ↓
Monitor for changes:
  1. RocksDB writes (immediate notification)
  2. Flush operations (batch notification)
    ↓
Match against subscription filter (WHERE clause)
    ↓
Send change notification to client:
  - INSERT: new row values
  - UPDATE: old and new row values
  - DELETE: deleted row values
    ↓
Update bandwidth metrics in system.live_queries
```

**Change Detection**:
- RocksDB writes trigger immediate notifications (< 50ms)
- Flush operations trigger batch notifications for persisted data
- Subscribers receive unified stream regardless of storage tier

### System Tables Architecture

System tables (users, live_queries, storage_locations, jobs) use **RocksDB only**:

```
System Table Operations
    ↓
Read/Write directly to RocksDB
    ↓
No flush policies (always in hot storage)
    ↓
Optimized for low-latency admin queries
```

**Rationale**:
- System tables are typically small (metadata)
- Require fast access for permissions, catalog queries
- No need for long-term archival in Parquet format

### Schema Storage and Versioning

Each table schema is stored in a **versioned, Arrow-compatible format** to support schema evolution and ALTER TABLE operations:

**Directory Structure**:
```
/configs/
  {namespace}/
    manifest.json                    # Registry of all tables and current schema versions
    schemas/
      messages/                      # One folder per table
        schema_v1.json               # Arrow schema with metadata
        schema_v2.json               # Evolved schema (after ALTER TABLE)
        current.json → schema_v2.json  # Symlink/pointer to active version
      user_events/
        schema_v1.json
        current.json → schema_v1.json
      ai_signals/
        schema_v1.json
        current.json → schema_v1.json
```

**Manifest File Format** (`manifest.json`):
```json
{
  "messages": {
    "current_version": 2,
    "path": "schemas/messages/schema_v2.json",
    "table_type": "user",
    "created_at": "2025-01-15T10:30:00Z",
    "updated_at": "2025-03-20T14:45:00Z"
  },
  "user_events": {
    "current_version": 1,
    "path": "schemas/user_events/schema_v1.json",
    "table_type": "shared",
    "created_at": "2025-02-01T08:00:00Z",
    "updated_at": "2025-02-01T08:00:00Z"
  },
  "ai_signals": {
    "current_version": 1,
    "path": "schemas/ai_signals/schema_v1.json",
    "table_type": "stream",
    "created_at": "2025-03-10T12:15:00Z",
    "updated_at": "2025-03-10T12:15:00Z"
  }
}
```

**Arrow Schema File Format** (`schema_v1.json`):
```json
{
  "version": 1,
  "created_at": "2025-01-15T10:30:00Z",
  "arrow_schema": {
    "fields": [
      {
        "name": "id",
        "type": {"name": "int", "bitWidth": 64},
        "nullable": false,
        "metadata": {"auto_increment": "true"}
      },
      {
        "name": "conversation_id",
        "type": {"name": "utf8"},
        "nullable": false
      },
      {
        "name": "content",
        "type": {"name": "utf8"},
        "nullable": true
      },
      {
        "name": "_updated",
        "type": {"name": "timestamp", "unit": "MICROSECOND"},
        "nullable": false,
        "metadata": {"system_column": "true"}
      },
      {
        "name": "_deleted",
        "type": {"name": "bool"},
        "nullable": false,
        "metadata": {"system_column": "true"}
      }
    ]
  }
}
```

**Schema Evolution Workflow**:
```
1. User executes ALTER TABLE command
    ↓
2. System validates schema compatibility (DataFusion projection)
    ↓
3. Create new schema version file (schema_v{N+1}.json)
    ↓
4. Update manifest.json with new current_version and updated_at
    ↓
5. Update current.json symlink to new version
    ↓
6. DataFusion reloads schema for future queries
    ↓
7. Existing Parquet files remain with old schema
   (DataFusion handles projection automatically)
```

**Schema Loading**:
- **On table access**: System reads `manifest.json` → loads `current.json` → deserializes Arrow schema
- **DataFusion integration**: Arrow schema directly loaded into DataFusion TableProvider
- **Caching**: Manifest and current schemas cached in memory, invalidated on ALTER TABLE
- **Backwards compatibility**: Old Parquet files read with schema projection (missing columns filled with NULL)

**Benefits**:
- ✅ Arrow-native format for zero-copy DataFusion integration
- ✅ Versioning enables audit trail of schema changes
- ✅ Symlink provides fast lookup of current schema
- ✅ Manifest provides O(1) table existence checks
- ✅ Created/updated timestamps enable change tracking
- ✅ Future-proof for ALTER TABLE ADD/DROP/MODIFY COLUMN

### Flush Policy Enforcement

Flush policies determine **when data moves from RocksDB to Parquet**:

**Row-Based Flush Policy**:
```sql
CREATE USER TABLE messages (...)
FLUSH POLICY ROWS 10000;
```
- Background monitor tracks row count in RocksDB per table/user
- When threshold reached → trigger flush job
- All buffered rows serialized to single Parquet file

**Time-Based Flush Policy**:
```sql
CREATE USER TABLE logs (...)
FLUSH POLICY INTERVAL '5 minutes';
```
- Background scheduler triggers flush every N seconds/minutes
- Flushes all buffered data regardless of row count
- Ensures data persisted within predictable time window

**Combined Policy** (flush on whichever condition met first):
```sql
CREATE USER TABLE metrics (...)
FLUSH POLICY ROWS 5000 INTERVAL '1 minute';
```

### Resource Management

**RocksDB Buffer Management**:
- Configurable memory limit per table/namespace
- LRU eviction for memory pressure (rare, typically flush first)
- WAL (Write-Ahead Log) for crash recovery

**Flush Job Execution**:
- Concurrent flush jobs allowed (different tables/users)
- Resource tracking: CPU, memory, bytes written
- Recorded in system.jobs table for monitoring

**Storage Backend Interaction**:
- S3: Async uploads, retry logic, credential management
- Filesystem: Direct writes, configurable paths
- Error handling: Failed flushes recorded in system.jobs with partial metrics

## User Scenarios & Testing *(mandatory)*

### User Story 0 - REST API and WebSocket Interface (Priority: P1)

A developer wants to interact with KalamDB through a simple HTTP REST API for executing SQL commands, and WebSocket connections for live query subscriptions with initial data fetch and real-time updates.

**Why this priority**: REST API and WebSocket are the fundamental interfaces for all database interactions. Without them, no other features can be accessed.

**Independent Test**: Can be fully tested by sending SQL commands via POST request and establishing WebSocket connections with multiple query subscriptions.

**Acceptance Scenarios**:

1. **Given** I have database access, **When** I send a POST request to `/api/sql` with a single SQL statement, **Then** I receive query results or success confirmation in JSON format
2. **Given** I want to execute multiple commands, **When** I send a POST request with multiple SQL statements separated by semicolons, **Then** all commands execute in sequence and I receive results for each
3. **Given** I want live updates, **When** I establish WebSocket connection with an array of subscription queries, **Then** the connection is accepted and I receive initial data for each query
4. **Given** I have an active WebSocket subscription, **When** I specify "last N rows" in my query options, **Then** I receive the last N rows immediately upon connection
5. **Given** I have a WebSocket subscription, **When** data changes (INSERT/UPDATE/DELETE) match my filter, **Then** I receive real-time change notifications with change type
6. **Given** I have multiple queries in one WebSocket connection, **When** changes occur, **Then** I receive notifications for all matching subscriptions
7. **Given** I send malformed SQL via REST API, **Then** I receive clear error messages with HTTP 400 status
8. **Given** I have a WebSocket subscription, **When** rows are deleted (soft delete), **Then** I receive DELETE notifications with the deleted row data

---

### User Story 1 - Namespace Management (Priority: P1)

A database administrator wants to organize their database into logical namespaces. They need to create, list, edit, drop namespaces, and configure namespace-specific options.

**Why this priority**: Namespaces are the foundational organizational structure. Without them, no tables can exist.

**Independent Test**: Can be fully tested by creating a namespace, listing it, editing its options, and verifying it appears in the catalog. Delivers the core organizational capability.

**Acceptance Scenarios**:

1. **Given** I have database access, **When** I create a namespace named "production", **Then** the namespace is created and appears in the namespace list
2. **Given** a namespace "production" exists, **When** I list all namespaces, **Then** I see "production" with its metadata (creation date, table count, options)
3. **Given** a namespace "production" exists, **When** I edit the namespace to add options, **Then** the options are stored and applied to the namespace
4. **Given** a namespace "production" has no tables, **When** I drop the namespace, **Then** the namespace is deleted successfully
5. **Given** a namespace "production" has tables, **When** I attempt to drop it, **Then** the system prevents deletion and shows which tables exist

---

### User Story 2 - System Tables and User Management (Priority: P1)

A database administrator wants to manage users and their permissions through a system table. Users have row-level permissions defined using filters that can reference columns or use regex patterns.

**Why this priority**: User management and permissions are foundational security requirements needed before any data operations.

**Independent Test**: Can be fully tested by adding users to the system table, assigning permissions with filters, and verifying access control is enforced.

**Acceptance Scenarios**:

1. **Given** I am a database administrator, **When** I add a user to the system users table, **Then** the user is created with default permissions
2. **Given** a user exists, **When** I assign permission `namespace.shared_table1.field1 = CURRENT_USER()`, **Then** the user can only access rows where field1 matches their username
3. **Given** a user exists, **When** I assign permission with regex pattern like `namespace.messages.*`, **Then** the user can access all tables matching the pattern
4. **Given** a user has row-level permissions, **When** they query a table, **Then** the system automatically applies the permission filter to their queries
5. **Given** a user has multiple permissions, **When** they access different tables, **Then** each table applies its specific permission filter

---

### User Story 2a - Live Query Monitoring via System Table (Priority: P1)

A database administrator or user wants to monitor active live query subscriptions in the system. They need to view all active subscriptions including connection details, bandwidth usage, and query information, and also ability to kill any live query

**Why this priority**: Visibility into active subscriptions is critical for monitoring, debugging, and resource management with killing/freeing the queries

**Independent Test**: Can be fully tested by creating live subscriptions and querying the system.live_queries table to verify all subscription details are visible.

**Acceptance Scenarios**:

1. **Given** a user has an active live query subscription, **When** I query `SELECT * FROM system.live_queries`, **Then** I see the subscription with id, user_id, query text, creation time, and bandwidth usage
2. **Given** multiple users have active subscriptions, **When** I filter `SELECT * FROM system.live_queries WHERE user_id = 'user123'`, **Then** I see only subscriptions for that user
3. **Given** a subscription is actively streaming data, **When** I query the live_queries table, **Then** I see real-time bandwidth metrics for that subscription
4. **Given** a subscription disconnects, **When** I query the live_queries table, **Then** the subscription is automatically removed from the table
5. **Given** I am a regular user, **When** I query system.live_queries, **Then** I only see my own subscriptions (subject to permissions)

---

### User Story 2b - Storage Location Management via System Table (Priority: P1)

A database administrator wants to predefine and manage storage locations through a system table. When creating tables, users can reference these predefined locations instead of specifying full location strings.

**Why this priority**: Centralized storage location management simplifies table creation and ensures consistency across tables.

**Independent Test**: Can be fully tested by adding storage locations to the system table, then creating tables that reference these locations by name.

**Acceptance Scenarios**:

1. **Given** I am a database administrator, **When** I add a storage location to `system.storage_locations` with name "s3-prod" and path "s3://prod-bucket/data/", **Then** the location is registered and available
2. **Given** a storage location "s3-prod" exists, **When** I create a table with `LOCATION REFERENCE 's3-prod'`, **Then** the table uses the predefined location
3. **Given** a storage location is referenced by tables, **When** I query `SELECT * FROM system.storage_locations`, **Then** I see usage count and which tables reference it
4. **Given** I want to update a storage location, **When** I modify the system.storage_locations entry, **Then** existing tables continue using the old path but new tables use the updated path
5. **Given** a storage location is in use, **When** I attempt to delete it from system.storage_locations, **Then** the system prevents deletion and shows dependent tables

---

### User Story 2c - Job Monitoring via System Table (Priority: P1)

A database administrator wants to monitor active and historical jobs (flush operations, cleanup tasks, scheduled jobs, etc.) through a system table. They need to view when jobs started and completed, job results/traces, resource usage (memory/CPU), execution parameters, and which node executed the job.

**Why this priority**: Visibility into all system jobs is critical for performance monitoring, debugging, and capacity planning across all operational tasks.

**Independent Test**: Can be fully tested by triggering various jobs (flush operations, cleanup tasks, scheduled jobs), querying the system.jobs table, and verifying all job details are recorded.

**Acceptance Scenarios**:

1. **Given** a job starts (flush, cleanup, or scheduled task), **When** I query `SELECT * FROM system.jobs`, **Then** I see the job with status='running', start_time, job_type, parameters, and node_id
2. **Given** a job completes, **When** I query the jobs table, **Then** I see status='completed', end_time, result, trace, memory_used, and cpu_used
3. **Given** multiple jobs are running, **When** I filter `SELECT * FROM system.jobs WHERE status='running'`, **Then** I see all active operations
4. **Given** I want to see job history, **When** I query `SELECT * FROM system.jobs WHERE created_at > NOW() - INTERVAL '1h'`, **Then** I see completed jobs from the last hour
5. **Given** the system.jobs table has a retention policy, **When** old job records exceed the retention period, **Then** they are automatically removed
6. **Given** a job fails, **When** I query the table, **Then** I see status='failed' with error_message and partial metrics
7. **Given** I am a regular user, **When** I query system.jobs, **Then** I only see jobs for my own user tables (subject to permissions)
8. **Given** a job has parameters, **When** I query the table, **Then** I see the parameters array showing all inputs to the job
9. **Given** a job completes, **When** I query the table, **Then** I see the result field containing the job outcome and trace showing execution context

---

### User Story 3 - User Table Creation and Management (Priority: P1)

A developer wants to create user-scoped tables where each user has their own isolated table instance. They need to define the table schema with auto-increment fields, specify storage locations with user ID templates, and configure flush policies. The system automatically adds tracking columns for change detection and soft deletes.

**Why this priority**: User tables are the core data model for isolated, per-user data storage. Essential for multi-tenant scenarios.

**Independent Test**: Can be fully tested by creating a user table definition, inserting data for different users, and verifying data isolation per user ID.

**Acceptance Scenarios**:

1. **Given** I am in namespace "production", **When** I create a user table "messages" with schema (message_id BIGINT, content STRING), **Then** the table definition is created with automatic system columns (_updated TIMESTAMP, _deleted BOOLEAN)
2. **Given** I create a user table "messages", **When** I specify a storage location with user ID template like `s3://bucket/users/${user_id}/messages/`, **Then** each user's data is stored in their own path
3. **Given** I create a user table, **When** I don't specify an auto-increment field, **Then** the system automatically adds a snowflake ID field
4. **Given** a user table "messages" exists, **When** user "user123" inserts data, **Then** the data is stored with _updated = NOW() and _deleted = false at the user-specific location
5. **Given** a user table "messages" exists, **When** I query data for user "user123", **Then** I only see data belonging to that user where _deleted = false
6. **Given** I create a user table, **When** I specify a flush policy with row limit, **Then** data is flushed to disk when the limit is reached
7. **Given** I create a user table, **When** I specify a flush policy with time interval, **Then** data is flushed to disk at the specified intervals
8. **Given** I create a user table with deleted_retention option, **When** I set deleted_retention='7d', **Then** soft-deleted rows are kept for 7 days before physical removal
9. **Given** a user table has deleted_retention='off', **When** rows are soft-deleted, **Then** they are never physically removed (kept indefinitely)

---

### User Story 3a - Table Deletion and Cleanup (Priority: P1)

A developer wants to drop (delete) user tables and shared tables they no longer need. The system must clean up all associated data including RocksDB buffers, Parquet files, and metadata.

**Why this priority**: Table lifecycle management is essential for production use. Users need ability to remove obsolete tables and reclaim storage.

**Independent Test**: Can be fully tested by creating a table, inserting data, dropping the table, and verifying all data and metadata is removed.

**Acceptance Scenarios**:

1. **Given** a user table "messages" exists with no active subscriptions, **When** I execute DROP TABLE messages, **Then** the table is deleted and all associated data is removed
2. **Given** a user table has buffered data in RocksDB, **When** I drop the table, **Then** both RocksDB buffer and Parquet files are deleted
3. **Given** a user table has data across multiple user IDs, **When** I drop the table, **Then** all user-specific Parquet files are deleted from storage
4. **Given** a shared table exists, **When** I drop the table, **Then** the shared table data and metadata are removed
5. **Given** a table has active live query subscriptions, **When** I attempt to drop the table, **Then** the system prevents deletion and shows active subscription count
6. **Given** I drop a table, **When** I query the catalog, **Then** the table no longer appears in the table list
7. **Given** a table is dropped, **When** I try to query it, **Then** I receive "table not found" error
8. **Given** a table is dropped, **When** I create a new table with the same name, **Then** the new table is created successfully without conflicts

---

### User Story 3b - Table Schema Evolution (ALTER TABLE) (Priority: P2)

A developer wants to modify table schemas after they have been created and contain data. They need to add new columns, remove unused columns, or modify column types as application requirements evolve. The system must preserve existing data and maintain backwards compatibility with old Parquet files.

**Why this priority**: Schema evolution is essential for long-lived applications. Requirements change over time, and forcing developers to recreate tables with data migration is costly.

**Independent Test**: Can be fully tested by creating a table, inserting data, altering the schema (add/drop/modify columns), and verifying queries work correctly with both old and new data.

**Acceptance Scenarios**:

1. **Given** a user table "messages" exists with data, **When** I execute ALTER TABLE messages ADD COLUMN priority STRING, **Then** the new column is added and existing rows have NULL for the new column
2. **Given** I add a column with DEFAULT value, **When** querying old data, **Then** the default value is used for rows created before the schema change
3. **Given** a table has multiple schema versions, **When** I query the table, **Then** DataFusion merges data from all Parquet files using schema projection
4. **Given** a column is referenced in an active live query subscription, **When** I attempt to drop that column, **Then** the system prevents the change and shows which subscriptions reference it
5. **Given** I modify a column type, **When** the change is incompatible (e.g., STRING to INT), **Then** the system rejects the change with validation error
6. **Given** I execute ALTER TABLE, **When** the operation succeeds, **Then** a new schema version file is created and manifest.json is updated
7. **Given** a table has schema version 3, **When** I run DESCRIBE TABLE, **Then** I see current version, schema history, and paths to schema files
8. **Given** I attempt to alter a stream table, **When** I execute ALTER TABLE, **Then** the system rejects the operation (stream tables have immutable schemas)
9. **Given** I attempt to alter a system column (_updated, _deleted), **When** I execute ALTER TABLE, **Then** the system rejects the operation

---

### User Story 4a - Stream Table Creation for Ephemeral Events (Priority: P1)

A developer wants to create stream tables for ephemeral, transient events that don't need persistence. These tables handle real-time events like "AI is typing", "user went offline", or other state changes that should be delivered to subscribers but not stored permanently on disk.

**Why this priority**: Stream tables enable real-time event delivery without the overhead of disk persistence, crucial for responsive chat applications and live status updates.

**Independent Test**: Can be fully tested by creating a stream table, publishing events to it, subscribing to the stream, and verifying events are delivered but not persisted to disk/S3.

**Acceptance Scenarios**:

1. **Given** I am in namespace "production", **When** I create a stream table "ai_signals" with retention='10s' and ephemeral=true, **Then** the stream table is created and does not persist data to disk
2. **Given** a stream table "ai_signals" exists, **When** I insert an event with event_type='ai_typing', **Then** the event is buffered in memory/RocksDB hot storage only
3. **Given** a stream table has retention='10s', **When** an event is older than 10 seconds, **Then** the event is automatically removed from the buffer
4. **Given** a stream table has max_buffer=1000, **When** the buffer reaches 1000 events, **Then** the oldest events are evicted to make room for new ones
5. **Given** a stream table has ephemeral=true, **When** there are no active subscribers, **Then** new events are immediately discarded without buffering
6. **Given** a stream table has ephemeral=false, **When** there are no active subscribers, **Then** events are buffered up to max_buffer limit for future subscribers
7. **Given** a user subscribes to a stream table, **When** events are inserted, **Then** the subscriber receives events in real-time without disk I/O
8. **Given** a stream table "ai_signals" exists, **When** I query its schema, **Then** I see it marked as type=STREAM with its retention and buffer configuration

---

### User Story 5 - Shared Table Creation and Management (Priority: P1)

A developer wants to create shared tables that are accessible across all users within a namespace. These tables store global configuration, lookup data, or cross-user information with optional flush policies.

**Why this priority**: Shared tables complement user tables by providing global data storage. Both are needed for a complete data model.

**Independent Test**: Can be fully tested by creating a shared table, inserting data from different users, and verifying all users can access the same data.

**Acceptance Scenarios**:

1. **Given** I am in namespace "production", **When** I create a shared table "conversations" with schema (conversation_id STRING, created_at TIMESTAMP), **Then** the table is created as a shared resource
2. **Given** a shared table "conversations" exists, **When** any user inserts data, **Then** the data is visible to all users in the namespace (subject to permissions)
3. **Given** a shared table exists, **When** I specify a storage location, **Then** all data is stored at that single location (no user ID templating)
4. **Given** I create a shared table, **When** I query from different users, **Then** all users see the same complete dataset (filtered by their permissions)
5. **Given** I create a shared table, **When** I specify a flush policy, **Then** data is flushed according to the policy (row-based or time-based)

---

### User Story 6 - Live Query Subscriptions with Change Tracking (Priority: P2)

A user wants to subscribe to real-time changes on their user table or stream table with filtered queries using WebSocket connections. The live query subscription automatically tracks and delivers all data changes (INSERT, UPDATE, DELETE) matching the subscription filter, eliminating the need for a separate Change Data Capture (CDC) mechanism.

**Why this priority**: Live queries with built-in change tracking enable real-time applications. The WebSocket-based subscription mechanism inherently provides CDC functionality.

**Independent Test**: Can be fully tested by subscribing to a filtered query via WebSocket, performing insert/update/delete operations, and verifying the subscriber receives all change notifications with correct change types.

**Acceptance Scenarios**:

1. **Given** user "user123" has a table "messages", **When** they subscribe to `SELECT * FROM messages WHERE conversation_id = 'conv456'`, **Then** the subscription is created and active
2. **Given** a subscription exists for conversation_id = 'conv456', **When** a message with that conversation_id is inserted, **Then** the subscriber receives an INSERT notification with the new row
3. **Given** a subscription exists, **When** a matching row is updated, **Then** the subscriber receives an UPDATE notification with old and new row values
4. **Given** a subscription exists, **When** a matching row is deleted, **Then** the subscriber receives a DELETE notification with the deleted row data
5. **Given** a subscription exists, **When** data not matching the filter is changed, **Then** the subscriber does not receive any notification
6. **Given** multiple subscriptions exist for the same user, **When** data is changed, **Then** only matching subscriptions receive notifications
7. **Given** a subscription is active, **When** the subscriber disconnects, **Then** the subscription is cleaned up automatically

---

### User Story 7 - Namespace Backup and Restore (Priority: P3)

A database administrator wants to backup entire namespaces including all tables, schemas, and data. They need a simple command to create backups and restore them later.

**Why this priority**: Backup is critical for production but can be implemented after core functionality is stable.

**Independent Test**: Can be fully tested by backing up a namespace, dropping it, restoring from backup, and verifying all data is recovered.

**Acceptance Scenarios**:

1. **Given** a namespace "production" with multiple tables, **When** I execute a backup command, **Then** all namespace metadata and table data is backed up to a specified location
2. **Given** a backup exists, **When** I restore the namespace, **Then** all tables and data are recreated exactly as they were
3. **Given** I want incremental backups, **When** I execute a backup with incremental flag, **Then** only changes since last backup are saved
4. **Given** a backup file exists, **When** I list backup contents, **Then** I see all tables and metadata without restoring

---

### User Story 8 - Table and Namespace Catalog Browsing (Priority: P2)

A user wants to browse and inspect their database structure just like in traditional SQL servers. They need to list namespaces, view tables within namespaces, and inspect table schemas.

**Why this priority**: Discovery and introspection are essential for usability but follow after basic creation capabilities.

**Independent Test**: Can be fully tested by creating namespaces and tables, then using catalog queries to list and inspect them.

**Acceptance Scenarios**:

1. **Given** multiple namespaces exist, **When** I query the system catalog for namespaces, **Then** I see all namespaces with their metadata
2. **Given** I am in namespace "production", **When** I list tables, **Then** I see user tables, shared tables, and system tables with their types indicated
3. **Given** a table "messages" exists, **When** I describe the table, **Then** I see all columns with their types, which is the auto-increment field, storage location, and flush policy
4. **Given** a user table exists, **When** I query table statistics, **Then** I see row counts per user ID or aggregate statistics

---

### Edge Cases

- What happens when a user tries to create a table without having permissions in the system users table?
- How does the system handle permission filters with CURRENT_USER() when the user is not authenticated?
- What happens when a user tries to create a user table without specifying any columns?
- How does the system handle storage location templates with invalid user ID variables?
- What happens when a live query subscription filter is syntactically invalid?
- How does the system handle INSERT/UPDATE/DELETE notifications when a row transitions in and out of a subscription filter?
- What happens when flush policy row limit is reached while a transaction is in progress?
- How does the system handle time-based flush policy conflicts with row-based flush policy?
- What happens when backup is attempted on a namespace that is actively being written to?
- How does the system handle user IDs with special characters in storage location paths?
- What happens when S3 storage is unavailable during a flush operation?
- What happens when a user tries to drop a table that has active live query subscriptions?
- How does the system handle restoring a backup when the namespace already exists?
- What happens when permission regex patterns overlap or conflict?
- How does the system handle a user querying data they previously had access to but permissions were revoked?
- What happens when querying system.live_queries if there are thousands of active subscriptions?
- How does the system calculate bandwidth metrics for live subscriptions with intermittent data flow?
- What happens when a storage location referenced by name is deleted while tables are using it?
- How does the system handle creating a table with LOCATION REFERENCE to a non-existent location name?
- What happens when a storage location path is updated while data is being written to it?
- How does the system handle duplicate storage location names in system.storage_locations?
- What happens when a stream table's max_buffer is reached and new events arrive?
- How does the system handle stream table retention period updates while events are buffered?
- What happens when a subscriber connects to a stream table with buffered events (ephemeral=false)?
- How does the system handle INSERT operations on stream tables vs regular user tables?
- What happens when querying a stream table directly (SELECT without subscription)?
- How does the system handle stream table events when RocksDB memory is full?
- What happens when a job crashes mid-operation and system.jobs still shows status='running'?
- How does the system handle concurrent jobs of the same type on the same table?
- What happens when querying system.jobs if there are millions of historical records?
- How does the system measure CPU and memory usage during job execution accurately?
- What happens when job retention policy is updated while old jobs exist?
- How does the system handle job parameters that are too large to store as strings?
- What happens when a job result exceeds reasonable string length limits?
- How does the system handle job trace information when execution context is unavailable?
- What happens when a query spans both RocksDB buffer and Parquet files with overlapping data?
- How does DataFusion handle deduplication when the same row exists in both RocksDB and Parquet?
- What happens when _updated timestamp bloom filter has false positives?
- How does the system handle UPDATE operations on rows that are already soft-deleted (_deleted = true)?
- What happens when deleted_retention cleanup job runs while queries are accessing soft-deleted rows?
- How does the system handle queries that explicitly want to see soft-deleted rows (_deleted = true)?
- What happens when deleted_retention is changed from 'off' to a specific duration while old deleted rows exist?
- How does the system track which deleted rows are eligible for physical removal during retention cleanup?
- What happens when dropping a table with large amounts of buffered data in RocksDB?
- How does the system handle DROP TABLE requests while flush operations are in progress?
- What happens when attempting to drop a table that is referenced by active permissions?
- How does the system clean up user-specific Parquet files across thousands of user IDs during DROP TABLE?
- What happens when storage backend (S3/filesystem) is unavailable during DROP TABLE?
- How does the system handle DROP TABLE rollback if file deletion fails partway through?
- What happens when ALTER TABLE ADD COLUMN is executed while a flush operation is writing Parquet files?
- How does the system handle ALTER TABLE DROP COLUMN when the column is referenced in WHERE clauses of active subscriptions?
- What happens when manifest.json becomes corrupted or contains invalid JSON?
- How does the system recover if a schema version file (schema_v{N}.json) is missing or corrupted?
- What happens when current.json symlink points to a non-existent schema version?
- How does the system handle ALTER TABLE operations when the filesystem is out of space?
- What happens when two ALTER TABLE operations are executed concurrently on the same table?
- How does DataFusion handle schema projection when a column type changed incompatibly across versions?
- What happens when querying old Parquet files (v1) after multiple schema evolutions (now at v5)?
- How does the system handle DEFAULT values for new columns when reading old Parquet files?
- What happens when ALTER TABLE MODIFY COLUMN attempts to narrow a type (e.g., BIGINT to INT)?
- How does the system validate backwards compatibility before committing schema changes?
- What happens when schema cache is invalidated but some queries are in-flight with old schema?
- How does the system handle DROP COLUMN when the column was added and immediately dropped in rapid succession?
- What happens when attempting to add a column with a name that matches a system column (_updated, _deleted)?
- How does the system handle manifest.json concurrent updates from multiple schema alterations?
- What happens when a schema version number overflows or reaches very high values (v9999+)?
- How does DESCRIBE TABLE display schema history when there are hundreds of schema versions?
- What happens when backup/restore encounters tables with different schema versions across user IDs?

## Requirements *(mandatory)*

### Functional Requirements

#### REST API and WebSocket Interface
- **FR-001**: System MUST provide a single HTTP POST endpoint `/api/sql` for executing SQL statements
- **FR-002**: System MUST accept a single SQL statement or multiple statements separated by semicolons in the request body
- **FR-003**: System MUST execute multiple SQL statements in sequence when provided
- **FR-004**: System MUST return query results in JSON format for SELECT statements
- **FR-005**: System MUST return success/failure status for DDL and DML statements (CREATE, INSERT, UPDATE, DELETE, DROP)
- **FR-006**: System MUST provide WebSocket endpoint `/ws` for live query subscriptions
- **FR-007**: System MUST accept an array of subscription queries when establishing WebSocket connection
- **FR-008**: System MUST support subscription options including "last N rows" to fetch initial data
- **FR-009**: System MUST return initial data (last N rows) immediately upon WebSocket subscription establishment
- **FR-010**: System MUST stream real-time change notifications (INSERT, UPDATE, DELETE) for all active subscriptions
- **FR-011**: System MUST support multiple concurrent subscriptions within a single WebSocket connection
- **FR-012**: System MUST include change type (INSERT/UPDATE/DELETE) in all change notifications
- **FR-013**: System MUST support authentication and authorization for REST API and WebSocket connections
- **FR-014**: System MUST return appropriate HTTP status codes (200 OK, 400 Bad Request, 401 Unauthorized, 500 Internal Server Error)
- **FR-015**: System MUST return clear error messages for malformed SQL or invalid operations
- **FR-016**: System MUST support CORS headers for web browser clients
- **FR-017**: System MUST log all API requests and WebSocket connections for debugging and auditing
- **FR-018**: System MUST NOT implement any additional REST API endpoints beyond `/api/sql`

#### Namespace Management
- **FR-010**: System MUST allow creation of namespaces with unique names
- **FR-002**: System MUST support listing all namespaces with their metadata (creation date, table count, options)
- **FR-003**: System MUST allow editing namespace options (options structure to be defined in implementation)
- **FR-004**: System MUST allow dropping empty namespaces
- **FR-005**: System MUST prevent deletion of namespaces that contain tables and show which tables exist
- **FR-006**: System MUST enforce namespace name uniqueness across the entire database

#### System Tables and User Management
- **FR-016**: System MUST provide a system users table containing all users added to the system
- **FR-017**: System MUST store user permissions in the system users table
- **FR-018**: System MUST support row-level permissions using column value filters (e.g., `namespace.shared_table1.field1 = CURRENT_USER()`)
- **FR-019**: System MUST support regex-based permissions for table access patterns (e.g., `namespace.messages.*`)
- **FR-020**: System MUST automatically apply permission filters to all user queries
- **FR-021**: System MUST support CURRENT_USER() function in permission expressions
- **FR-022**: System MUST enforce permission checks before allowing any data access
- **FR-023**: System MUST handle multiple permissions per user with appropriate precedence rules

#### System Live Queries Table
- **FR-024**: System MUST provide a system.live_queries table showing all active live query subscriptions
- **FR-025**: System.live_queries table MUST include columns: id, user_id, query, created_at, bandwidth
- **FR-026**: System MUST automatically add entries to system.live_queries when subscriptions are created
- **FR-027**: System MUST automatically remove entries from system.live_queries when subscriptions disconnect
- **FR-028**: System MUST update bandwidth metrics in real-time for active subscriptions
- **FR-029**: System MUST apply user permissions to system.live_queries queries (users see only their own subscriptions by default)
- **FR-030**: System MUST track bandwidth usage per subscription (bytes sent, messages sent, or similar metric)

#### System Storage Locations Table
- **FR-031**: System MUST provide a system.storage_locations table for predefined storage locations
- **FR-032**: System.storage_locations table MUST include columns: location_name, location_type, path, credentials_ref, created_at, usage_count
- **FR-033**: System MUST allow administrators to add storage locations to system.storage_locations
- **FR-034**: System MUST support LOCATION REFERENCE '<name>' syntax for table creation using predefined locations
- **FR-035**: System MUST resolve location references from system.storage_locations at table creation time
- **FR-036**: System MUST track usage count showing how many tables reference each storage location
- **FR-037**: System MUST prevent deletion of storage locations that are referenced by existing tables
- **FR-038**: System MUST enforce unique location names in system.storage_locations
- **FR-039**: System MUST validate storage location accessibility when adding to system.storage_locations

#### System Jobs Table
- **FR-040**: System MUST provide a system.jobs table showing all active and historical jobs (flush, cleanup, scheduled tasks, etc.)
- **FR-041**: System.jobs table MUST include columns: job_id, job_type, table_name, namespace, user_id (for user tables), status, start_time, end_time, parameters (array of strings), result (string), trace (string), memory_used_mb, cpu_used_percent, node_id, error_message
- **FR-042**: System MUST automatically add entries to system.jobs when any job starts
- **FR-043**: System MUST update system.jobs entries with completion metrics when jobs finish
- **FR-044**: System MUST track job status as 'running', 'completed', or 'failed'
- **FR-045**: System MUST record resource usage metrics (memory and CPU) during job execution
- **FR-046**: System MUST record which node executed the job (node_id)
- **FR-047**: System MUST support configurable retention policy for job history (e.g., keep last 7 days)
- **FR-048**: System MUST automatically remove job records older than the retention period
- **FR-049**: System MUST apply user permissions to system.jobs queries (users see only their own table jobs by default)
- **FR-050**: System MUST record partial metrics for failed jobs with error details
- **FR-051**: System MUST store job parameters as an array of strings for input tracking
- **FR-052**: System MUST store job result as a string containing the job outcome/output
- **FR-053**: System MUST store job trace as a string containing execution context/location information

#### User Table Management
- **FR-045**: System MUST support creating user tables where each user ID gets their own table instance
- **FR-043**: System MUST support CREATE USER TABLE syntax with column definitions
- **FR-044**: System MUST always add auto-increment fields to user tables (snowflake IDs by default)
- **FR-045**: System MUST support LOCATION clause with user ID templating like `s3://bucket/path/${user_id}/table/`
- **FR-046**: System MUST support LOCATION REFERENCE '<name>' syntax to use predefined storage locations
- **FR-047**: System MUST substitute ${user_id} in storage paths with actual user IDs at runtime
- **FR-048**: System MUST isolate data between different user IDs in user tables
- **FR-049**: System MUST support DataFusion-compatible datatypes for user table columns
- **FR-050**: System MUST store user table data in Parquet format
- **FR-051**: System MUST support flush-to-disk policy with row limit configuration
- **FR-052**: System MUST support flush-to-disk policy with time-based interval configuration
- **FR-053**: System MUST flush data to disk when either row limit or time interval is reached
- **FR-054**: System MUST automatically add system columns to every user table: _updated (TIMESTAMP), _deleted (BOOLEAN)
- **FR-055**: System MUST set _updated column to current timestamp on every INSERT and UPDATE operation
- **FR-056**: System MUST initialize _deleted column to false on INSERT operations
- **FR-057**: System MUST build Bloom filters on _updated column in Parquet files for efficient time-range queries
- **FR-058**: System MUST support deleted_retention option for user tables (e.g., '7d', '30d', 'off')
- **FR-059**: System MUST implement soft delete by setting _deleted = true when DELETE operations execute
- **FR-060**: System MUST update _updated timestamp when soft delete occurs
- **FR-061**: When deleted_retention != 'off', system MUST schedule cleanup jobs to physically remove soft-deleted rows older than the retention period
- **FR-062**: When deleted_retention = 'off', system MUST keep soft-deleted rows indefinitely
- **FR-063**: System MUST support queries filtering by _deleted column to exclude/include soft-deleted rows
- **FR-064**: System MUST support DROP USER TABLE command to delete user tables
- **FR-065**: System MUST prevent DROP TABLE when active live query subscriptions exist
- **FR-066**: System MUST mark all rows as deleted (_deleted = true) when DROP TABLE executes
- **FR-067**: System MUST remove table metadata from manifest.json when DROP TABLE completes

#### Schema Storage and Versioning
- **FR-068**: System MUST store each table schema in Arrow-compatible JSON format
- **FR-069**: System MUST organize schemas in `/configs/{namespace}/schemas/{table_name}/` directory structure
- **FR-070**: System MUST create versioned schema files named `schema_v{N}.json` where N increments on each ALTER TABLE
- **FR-071**: System MUST maintain a manifest.json file at `/configs/{namespace}/manifest.json` listing all tables
- **FR-072**: Manifest.json MUST include for each table: current_version, path, table_type, created_at, updated_at
- **FR-073**: System MUST create a `current.json` symlink/pointer to the active schema version for each table
- **FR-074**: Schema files MUST include Arrow schema with field definitions (name, type, nullable, metadata)
- **FR-075**: Schema files MUST include version number and created_at timestamp
- **FR-076**: System MUST mark system columns (_updated, _deleted) with metadata flag `"system_column": "true"`
- **FR-077**: System MUST cache manifest.json and current schemas in memory for fast table access
- **FR-078**: System MUST invalidate schema cache when ALTER TABLE operations execute
- **FR-079**: System MUST load Arrow schemas directly into DataFusion TableProvider for zero-copy integration
- **FR-080**: System MUST support backwards compatibility when reading old Parquet files after schema evolution

#### Table Schema Alteration (ALTER TABLE)
- **FR-081**: System MUST support ALTER TABLE ADD COLUMN command for user and shared tables
- **FR-082**: System MUST support ALTER TABLE DROP COLUMN command for user and shared tables
- **FR-083**: System MUST support ALTER TABLE MODIFY COLUMN command to change column types (with validation)
- **FR-084**: System MUST increment schema version number when ALTER TABLE executes successfully
- **FR-085**: System MUST create new schema_v{N+1}.json file with updated Arrow schema
- **FR-086**: System MUST update manifest.json with new current_version and updated_at timestamp
- **FR-087**: System MUST update current.json symlink to point to new schema version
- **FR-088**: System MUST validate schema changes for DataFusion compatibility before applying
- **FR-089**: System MUST prevent dropping columns that are referenced in active live query subscriptions
- **FR-090**: System MUST prevent schema changes that break backwards compatibility with existing Parquet files
- **FR-091**: System MUST NOT allow altering system columns (_updated, _deleted)
- **FR-092**: System MUST support adding columns with DEFAULT values
- **FR-093**: System MUST handle NULL values for new columns when reading old Parquet data
- **FR-094**: System MUST NOT support ALTER TABLE on stream tables (schemas are immutable for ephemeral tables)
- **FR-095**: System MUST use schema projection to fill missing columns with NULL when reading old Parquet files

#### Stream Table Management
- **FR-096**: System MUST support creating stream tables for ephemeral, non-persistent events
- **FR-097**: System MUST support CREATE STREAM TABLE syntax with column definitions
- **FR-098**: System MUST support 'retention' option to specify event TTL (e.g., '10s', '1m', '5m')
- **FR-099**: System MUST support 'ephemeral' option (true/false) to control buffering when no subscribers exist
- **FR-100**: System MUST support 'max_buffer' option to limit the number of buffered events
- **FR-101**: System MUST NOT persist stream table events to disk or S3 storage
- **FR-102**: System MUST store stream table events in memory or RocksDB hot storage only
- **FR-103**: System MUST automatically evict events older than the retention period
- **FR-104**: System MUST evict oldest events when max_buffer limit is reached
- **FR-105**: When ephemeral=true, system MUST discard new events immediately if no active subscribers exist
- **FR-106**: When ephemeral=false, system MUST buffer events up to max_buffer even without subscribers
- **FR-107**: System MUST deliver stream table events to subscribers in real-time without disk I/O
- **FR-108**: System MUST support live query subscriptions on stream tables
- **FR-109**: System MUST show stream tables in catalog with type=STREAM and configuration details
- **FR-110**: System MUST support DataFusion-compatible datatypes for stream table columns
- **FR-111**: System MUST handle stream table inserts with minimal latency (< 5ms)
- **FR-112**: System MUST NOT add _updated or _deleted system columns to stream tables (ephemeral, no soft deletes)
- **FR-113**: System MUST support DROP STREAM TABLE command to delete stream tables

#### Shared Table Management  
- **FR-114**: System MUST support creating shared tables accessible to all users in a namespace
- **FR-115**: System MUST support CREATE SHARED TABLE syntax with column definitions
- **FR-116**: System MUST automatically add system columns to every shared table: _updated (TIMESTAMP), _deleted (BOOLEAN)
- **FR-117**: System MUST store shared table data in a single location (no user ID templating)
- **FR-118**: System MUST apply user permissions to shared table access
- **FR-119**: System MUST support DataFusion-compatible datatypes for shared table columns
- **FR-120**: System MUST store shared table data in Parquet format
- **FR-121**: System MUST support flush-to-disk policy with row limit configuration for shared tables
- **FR-122**: System MUST support flush-to-disk policy with time-based interval configuration for shared tables
- **FR-123**: System MUST support deleted_retention option for shared tables (same as user tables)
- **FR-124**: System MUST handle soft deletes in shared tables using _deleted column (same as user tables)
- **FR-125**: System MUST support DROP SHARED TABLE command to delete shared tables

#### Storage Management
- **FR-126**: System MUST support filesystem storage backend with configurable paths
- **FR-127**: System MUST support S3 storage backend with bucket and credential configuration
- **FR-128**: System MUST validate storage location accessibility before table creation
- **FR-129**: System MUST support storage location templates with ${user_id} variable substitution
- **FR-130**: System MUST track which tables use which storage locations
- **FR-131**: System MUST flush data to filesystem storage backend when flush policy is triggered
- **FR-132**: System MUST write Parquet files to configured filesystem paths
- **FR-133**: System MUST handle filesystem errors gracefully with retry logic and error reporting

#### Live Query Subscriptions with Change Tracking
- **FR-134**: System MUST allow users to subscribe to live queries on their user tables via WebSocket connections
- **FR-135**: System MUST support filtered subscriptions with WHERE clauses (e.g., `SELECT * FROM messages WHERE conversation_id = 'testid'`)
- **FR-136**: System MUST support subscription options to specify "last N rows" for initial data fetch
- **FR-137**: System MUST return initial data (last N rows) to subscribers immediately upon connection establishment
- **FR-138**: System MUST notify subscribers via WebSocket when matching data is inserted with INSERT change type
- **FR-139**: System MUST notify subscribers via WebSocket when matching data is updated with UPDATE change type and both old and new values
- **FR-140**: System MUST notify subscribers via WebSocket when matching data is soft-deleted (DELETE change type) with _deleted = true
- **FR-141**: System MUST only send notifications for data matching the subscription filter
- **FR-142**: System MUST isolate subscriptions per user ID (users can only subscribe to their own data)
- **FR-143**: System MUST automatically clean up subscriptions when WebSocket connections disconnect
- **FR-144**: System MUST support multiple concurrent subscriptions within a single WebSocket connection
- **FR-145**: System MUST handle each subscription independently within the same WebSocket connection
- **FR-146**: System MUST register all active subscriptions in system.live_queries table
- **FR-147**: Live query subscriptions MUST inherently provide change tracking without requiring separate CDC infrastructure
- **FR-148**: System MUST use _updated and _deleted columns for efficient change detection in live queries
- **FR-149**: System MUST support "changes since timestamp" queries using _updated column for initial subscription data

#### Namespace Backup and Restore
- **FR-150**: System MUST support backing up entire namespaces including all tables and data
- **FR-151**: System MUST support restoring namespaces from backup files
- **FR-152**: System MUST preserve all table schemas, data, and metadata during backup/restore
- **FR-153**: System MUST support listing backup contents without restoring
- **FR-154**: System MUST handle backup of namespaces with active write operations
- **FR-155**: System MUST support incremental backups (future enhancement, documented for planning)
- **FR-156**: System MUST NOT include stream table data in backups (stream tables are ephemeral)
- **FR-157**: System MUST include soft-deleted rows (_deleted = true) in backups to preserve change history
- **FR-158**: System MUST backup schema versions and manifest.json files to preserve schema history

#### Catalog and Introspection
- **FR-159**: System MUST provide SQL-like catalog queries to list namespaces
- **FR-160**: System MUST provide SQL-like catalog queries to list tables within a namespace
- **FR-161**: System MUST distinguish between user tables, shared tables, stream tables, and system tables in catalog listings
- **FR-162**: System MUST support DESCRIBE TABLE commands to show table schema
- **FR-163**: System MUST show auto-increment field configuration in table descriptions
- **FR-164**: System MUST show storage location configuration in table descriptions (not applicable for stream tables)
- **FR-165**: System MUST show flush policy configuration in table descriptions (not applicable for stream tables)
- **FR-166**: System MUST show stream configuration (retention, ephemeral, max_buffer) in stream table descriptions
- **FR-167**: System MUST provide table statistics (row counts, storage size)
- **FR-168**: System MUST support querying table metadata including creation date and last modified date
- **FR-169**: System MUST expose automatic system columns (_updated, _deleted) in table descriptions
- **FR-170**: System MUST show current schema version and schema history in table descriptions
- **FR-171**: DESCRIBE TABLE output MUST include path to current schema file and manifest.json reference

### Key Entities

- **Namespace**: A logical container for tables and configuration. Attributes: name (unique), creation timestamp, options (key-value pairs), table count
- **System Users Table**: A system table containing user information and permissions. Attributes: user_id, username, permissions (array of permission rules), created_at, last_login
- **System Live Queries Table**: A system table showing active live query subscriptions. Attributes: id (subscription_id), user_id, query (SQL text), created_at, bandwidth (bytes/messages sent)
- **System Storage Locations Table**: A system table containing predefined storage locations. Attributes: location_name (unique), location_type (filesystem/s3), path, credentials_ref, created_at, usage_count
- **System Jobs Table**: A system table showing active and historical jobs (flush, cleanup, scheduled tasks, etc.). Attributes: job_id, job_type (flush/cleanup/scheduled/etc.), table_name, namespace, user_id, status (running/completed/failed), start_time, end_time, parameters (array of strings), result (string), trace (string - execution context/location), memory_used_mb, cpu_used_percent, node_id, error_message, retention_days
- **Permission Rule**: Defines access control for a user. Attributes: rule_pattern (namespace.table.column or regex), filter_expression (e.g., `field1 = CURRENT_USER()`), rule_type (column_filter/regex)
- **User Table**: A table template where each user ID gets their own isolated table instance. Attributes: table_name, namespace, column definitions, auto-increment field, storage location template or reference, flush_policy, deleted_retention, system columns (_updated TIMESTAMP, _deleted BOOLEAN)
- **Stream Table**: An ephemeral table for transient events that are not persisted to disk. Attributes: table_name, namespace, column definitions, retention (TTL), ephemeral (boolean), max_buffer (integer), NO system columns
- **Shared Table**: A single table accessible by all users in a namespace (subject to permissions). Attributes: table_name, namespace, column definitions, storage location or reference, flush_policy, deleted_retention, system columns (_updated TIMESTAMP, _deleted BOOLEAN)
- **Flush Policy**: Configuration for when to write data to disk. Attributes: policy_type (row_limit/time_based), row_limit (optional), time_interval (optional)
- **User**: Represents an authenticated user in the system. Attributes: user_id, username
- **Live Query Subscription**: An active real-time query subscription. Attributes: subscription_id, user_id, table_name, query_filter, connection_handle, change_types (INSERT/UPDATE/DELETE), bandwidth_metrics
- **Change Notification**: A notification sent to subscribers. Attributes: change_type (INSERT/UPDATE/DELETE), affected_rows, old_values (for UPDATE/DELETE), new_values (for INSERT/UPDATE), timestamp
- **Storage Location**: Physical storage configuration (now managed via system table). Attributes: location_name, location_type (filesystem/s3), path_template, credentials_ref, usage_count, accessibility_status
- **Backup**: A snapshot of a namespace. Attributes: backup_id, namespace, creation_timestamp, file_location, size, table_count
- **Table Schema**: Arrow-compatible JSON representation of table structure. Attributes: version (integer), created_at (timestamp), arrow_schema (field definitions with types, nullability, metadata)
- **Schema Manifest**: Registry of all table schemas in a namespace. Attributes: namespace, table_entries (map of table_name → schema_metadata), last_updated
- **Schema Metadata**: Metadata for a single table's schema. Attributes: table_name, current_version (integer), schema_path (string), table_type (user/shared/stream), created_at (timestamp), updated_at (timestamp)
- **Schema Version File**: Individual versioned schema file. Location: `/configs/{namespace}/schemas/{table_name}/schema_v{N}.json`, Content: version, created_at, arrow_schema (Arrow JSON format)
- **Schema History**: Audit trail of schema changes. Derived from: sequence of schema_v{N}.json files, each representing one evolution of the table structure

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can create a namespace and user table within it in under 30 seconds
- **SC-002**: System supports at least 10,000 concurrent user IDs each with their own isolated table instances
- **SC-003**: Auto-increment ID generation completes in under 1 millisecond per ID
- **SC-004**: User table queries return results in under 100ms for tables with up to 1 million rows
- **SC-005**: Permission filters are applied to queries with less than 5ms overhead
- **SC-006**: Live query subscriptions deliver INSERT/UPDATE/DELETE notifications within 50ms of data change
- **SC-007**: System handles at least 1,000 concurrent live query subscriptions per user table
- **SC-008**: Row-based flush policy triggers within 100ms of reaching the configured row limit
- **SC-009**: Time-based flush policy triggers within 1 second of the configured interval
- **SC-010**: Namespace backup completes at a rate of at least 50 MB per second
- **SC-012**: Namespace restore completes with 100% data integrity verification
- **SC-013**: System correctly routes user data to storage paths with ${user_id} substitution 100% of the time
- **SC-014**: Catalog queries listing namespaces and tables return results in under 10ms
- **SC-015**: 95% of users successfully create their first user table without errors on first attempt
- **SC-016**: System handles storage backend failures gracefully with clear error messages and no data corruption
- **SC-017**: Live query subscription cleanup occurs within 5 seconds of client disconnect
- **SC-018**: System users table supports at least 100,000 users with permission lookups under 10ms
- **SC-019**: System.live_queries table queries return results in under 10ms even with 10,000 active subscriptions
- **SC-020**: Bandwidth metrics in system.live_queries update with less than 1 second latency
- **SC-021**: System.storage_locations table supports at least 1,000 predefined locations with lookup under 5ms
- **SC-022**: Storage location reference resolution during table creation adds less than 10ms overhead
- **SC-023**: Tables created with LOCATION REFERENCE successfully resolve to correct storage paths 100% of the time
- **SC-024**: Stream table event insertion completes in under 5ms
- **SC-025**: Stream table events are delivered to subscribers within 10ms of insertion
- **SC-026**: Stream table retention policy evicts expired events within 1 second of TTL expiration
- **SC-027**: Stream tables with ephemeral=true discard events immediately when no subscribers exist (< 1ms overhead)
- **SC-028**: Stream tables handle at least 10,000 events per second per table
- **SC-029**: System supports at least 1,000 concurrent stream tables per namespace
- **SC-030**: System.jobs table queries return results in under 10ms even with 10,000 historical job records
- **SC-031**: Job metrics (memory, CPU, result, trace) are recorded with less than 5% overhead
- **SC-032**: Job history retention cleanup runs automatically and completes within 1 minute
- **SC-033**: Failed jobs record partial metrics and error details 100% of the time
- **SC-034**: Job parameters are captured accurately for all job types 100% of the time
- **SC-035**: Queries spanning RocksDB and Parquet merge correctly with DataFusion 100% of the time
- **SC-036**: _updated bloom filter reduces Parquet file scans by at least 80% for time-range queries
- **SC-037**: Soft delete operations (_deleted = true) complete in under 5ms
- **SC-038**: Default queries (without explicit _deleted filter) exclude soft-deleted rows 100% of the time
- **SC-039**: Deleted retention cleanup jobs process at least 10,000 rows per second
- **SC-040**: Live query subscribers receive DELETE notifications (\_deleted = true) within 50ms of soft delete operation
- **SC-041**: REST API query endpoint responds within 100ms for simple SELECT queries on tables under 1M rows
- **SC-042**: WebSocket subscription establishment completes within 200ms
- **SC-043**: WebSocket initial data fetch (last N rows) completes within 500ms for N ≤ 1000
- **SC-044**: Multiple subscriptions in single WebSocket connection perform with less than 10% overhead vs separate connections
- **SC-045**: DROP TABLE operations complete within 5 seconds for tables with up to 1GB of data
- **SC-046**: Filesystem flush operations write at least 50 MB/sec to local disk
- **SC-047**: Hybrid queries (RocksDB + Parquet) perform within 2x of Parquet-only queries
- **SC-048**: REST API supports at least 1,000 concurrent SQL execution requests
- **SC-049**: ALTER TABLE ADD COLUMN completes within 1 second for tables with existing data
- **SC-050**: ALTER TABLE DROP COLUMN completes within 1 second (metadata operation only)
- **SC-051**: Schema version increment and manifest.json update complete atomically with no race conditions
- **SC-052**: Schema cache invalidation completes within 100ms across all active queries
- **SC-053**: DESCRIBE TABLE returns current schema version and history within 50ms
- **SC-054**: Queries on tables with 10+ schema versions perform within 10% of single-version tables
- **SC-055**: Schema projection for old Parquet files adds less than 5% query overhead
- **SC-056**: Manifest.json reads complete within 5ms when cached in memory
- **SC-057**: Schema files load and deserialize to Arrow format within 10ms
- **SC-058**: Backwards compatibility validation for schema changes completes within 50ms
- **SC-059**: ALTER TABLE validation (subscription checks, type compatibility) completes within 100ms
- **SC-060**: Schema storage directory structure supports at least 10,000 tables per namespace

### Documentation Success Criteria (Constitution Principle VIII)

- **SC-DOC-001**: All public APIs have comprehensive rustdoc comments with real-world examples
- **SC-DOC-002**: Module-level documentation explains purpose and architectural role
- **SC-DOC-003**: Complex algorithms and architectural patterns have inline comments explaining rationale
- **SC-DOC-004**: Architecture Decision Records (ADRs) document key design choices
- **SC-DOC-005**: Code review verification confirms documentation requirements are met

## Assumptions

- DataFusion is the query engine and supports the datatypes users will need
- Parquet is suitable for all DataFusion datatypes the system will support
- Snowflake ID generation provides sufficient uniqueness for distributed scenarios
- S3-compatible storage APIs are stable and well-documented
- User IDs are provided by the authentication system and are URL-safe strings
- Storage path templates with ${user_id} are substituted at runtime without performance penalties
- WebSocket or similar persistent connection mechanism
- Notification delivery guarantees are "at least once" for live query subscriptions
- Live query subscriptions inherently provide change tracking (INSERT/UPDATE/DELETE) without separate CDC mechanism
- Flush policies can be enforced at the table level without global synchronization
- Row-based and time-based flush policies can coexist, with flush happening when either condition is met
- Permission filters are simple enough to be translated into DataFusion query predicates
- CURRENT_USER() function can be resolved at query time from the session context
- Regex-based permissions are evaluated at query planning time, not at runtime
- System users table is protected and only accessible by administrators
- Backup/restore operations are performed during maintenance windows or low-traffic periods
- Namespace options structure will be defined during implementation phase
- Table schemas are defined at creation time and do not require dynamic schema evolution initially
- Stream tables are acceptable to lose on system restart (ephemeral by design)
- Stream table retention periods are short-lived (seconds to minutes, not hours/days)
- RocksDB or in-memory storage has sufficient capacity for max_buffer limits across all stream tables
- Stream table events do not require ACID guarantees or persistence
- Job metrics collection has minimal performance overhead (< 5%)
- System can accurately measure memory and CPU usage per job execution
- Job history retention policies are configurable per deployment
- Node_id for jobs can be determined from system context (single-node or cluster ID)
- Job parameters can be serialized as strings for storage
- Job results can be represented as strings (JSON, text, or structured format)
- Job trace information captures sufficient context for debugging and monitoring
- DataFusion can efficiently merge query results from RocksDB and Parquet sources
- Parquet bloom filters on _updated column provide significant query performance improvement
- Soft delete mechanism (_deleted column) is acceptable for most use cases (vs hard delete)
- Deleted row retention policies are sufficient for change tracking requirements
- Physical deletion of soft-deleted rows can be performed asynchronously without impacting live queries
- _updated and _deleted columns do not significantly increase storage overhead
- System can track deleted row timestamps efficiently for retention policy enforcement

## Dependencies

- DataFusion library for query execution and datatype support
- Actix for rest api/websocket and also each subscriber/live query is an actor
- Parquet library for data serialization/deserialization
- S3 SDK (e.g., aws-sdk-s3 for Rust) for S3 storage backend
- Snowflake ID generator library or implementation
- RocksDB store for namespace/table catalog, system tables, and buffered writes
- File system access for local storage backend
- WebSocket protocol for live query subscriptions (provides real-time change notifications for INSERT/UPDATE/DELETE)
- WebSocket must support multiple subscriptions per connection with independent filtering
- WebSocket must support initial data fetch ("last N rows") upon subscription establishment
- Template engine for ${user_id} path substitution
- Backup/restore library compatible with Parquet and metadata formats
- Permissions evaluation engine for filter and regex-based access control
- Background task scheduler for time-based flush policies and deleted row retention cleanup
- Bloom filter support in Parquet files for _updated column indexing
- JSON serialization/deserialization for Arrow schema storage and manifest files
- File system operations for schema directory management (/configs/{namespace}/schemas/)
- Symlink or pointer mechanism for current.json schema references
- Schema validation and backwards compatibility checking for ALTER TABLE operations
- Atomic file operations for manifest.json updates to prevent corruption

## Out of Scope
- Cross-namespace queries or data access
- Cross-user data access (strict user isolation)
- Advanced storage features like versioning, snapshots, or point-in-time recovery beyond basic backup
- Complex permission expressions beyond column filters and regex patterns (e.g., JOIN-based permissions)
- Role-based access control (RBAC) hierarchy - only direct user permissions
- Storage cost optimization or tiering automation
- Table partitioning strategies
- Incremental backup implementation (syntax reserved, implementation deferred)
- Live query subscriptions on shared tables (user tables only)
- Query optimization and performance tuning beyond basic DataFusion capabilities
- Transaction support and ACID guarantees beyond DataFusion's capabilities
- Data compression algorithm selection (use Parquet defaults)
- Audit logging of permission checks and data access
- Dynamic permission updates without system restart
- Permission inheritance or delegation mechanisms
- Hard delete operations (physical row removal on DELETE command)
- Automatic compaction of Parquet files with soft-deleted rows
- Additional REST API endpoints beyond `/api/sql` (keeping it simple with single endpoint)
- Separate WebSocket endpoints for different query types (single `/ws` endpoint handles all subscriptions)

## Future Considerations

### Table Export Functionality

**Note**: Table export is **NOT** part of the current feature scope. It is documented here for future implementation.

**Export Feature Overview**:

Users may need to export data from tables to files for backup, migration, or external processing using simple SQL-like syntax.

**Proposed Capabilities**:
- Export entire user tables: `EXPORT FROM messages TO 'backup.parquet'`
- Export shared tables: `EXPORT FROM conversations TO 'conversations.parquet'`
- Export query results: `EXPORT FROM (SELECT * FROM messages WHERE created_at > '2025-01-01') TO 'recent.parquet'`
- Support multiple formats: Parquet (default), CSV, JSON
- Respect user permissions during export operations
- Provide export confirmation with row count and file size

**Why defer this?**: Export functionality adds complexity around file handling, format conversion, and permission enforcement across potentially large datasets. The core database operations (CRUD, queries, live subscriptions) should be stable first. Users can achieve similar results by querying data through the API and saving results client-side.

**Implementation Considerations** (for future work):
- Streaming export for large datasets to avoid memory issues
- Parallel export for user tables across multiple user partitions
- Progress tracking for long-running exports
- Handling file path conflicts and storage quota limits
- Support for compressed export formats
- Incremental export capabilities

---

### Distributed Replication with Raft Consensus

**Note**: The following distributed replication architecture is **NOT** part of the current feature scope. It is documented here for future planning and to guide the architectural design to ensure compatibility when distribution is added later.

**Raft-based Distribution Model**:

KalamDB can implement distributed replication using the Raft consensus protocol to provide strong consistency guarantees across multiple nodes. The proposed architecture includes:

1. **Write Operations Flow**:
   - Client writes to the Raft leader node
   - Leader receives the write request and logs it
   - Leader replicates the change to all follower nodes in the Raft group
   - Leader waits for acknowledgment from a majority (quorum) of nodes
   - Only after quorum acknowledgment, leader returns success to client API call
   - Ensures strong consistency: all committed writes are durable across majority

2. **Raft Group Leadership**:
   - Raft group elects a single leader node
   - Leader is responsible for coordinating writes and flush operations
   - Leader orchestrates flushing buffered data from RocksDB to disk/S3 storage
   - Followers replicate leader's state machine and can serve read requests
   - Automatic leader election on failure for high availability

3. **Benefits**:
   - **Strong Consistency**: Linearizable writes across distributed nodes
   - **Durability**: Data replicated to multiple nodes before acknowledgment
   - **Fault Tolerance**: System continues operating with leader failure (automatic re-election)
   - **No Split Brain**: Raft's quorum-based approach prevents split-brain scenarios

4. **Implementation Considerations** (for future work):
   - Raft library integration (e.g., tikv/raft-rs for Rust)
   - Log replication mechanism between nodes
   - Snapshot management for state machine recovery
   - Read-after-write consistency guarantees
   - Network partition handling and quorum management
   - Performance impact of synchronous replication on write latency

**Why defer this?**: Distributed consensus adds significant complexity. The current architecture focuses on foundational capabilities (namespaces, tables, permissions, live queries) with single-node or simple async replication. Raft-based distribution should be added once core functionality is proven and scalability requirements demand strong consistency across distributed nodes.

**Design Compatibility**: The current architecture's separation of concerns (RocksDB for buffered writes, Parquet for storage, flush policies) naturally aligns with a future Raft implementation where the leader coordinates these operations across replicas.

---

### Deleted Row Retention and Cleanup Jobs

**Note**: The following deleted row cleanup implementation is **NOT** part of the current feature scope. It is documented here for future implementation.

**Deleted Row Cleanup Feature Overview**:

User and shared tables use soft deletes (_deleted = true) to support change tracking and "changes since timestamp" queries. However, soft-deleted rows accumulate over time and need periodic cleanup based on retention policies.

**Proposed Capabilities**:

1. **Retention Policy Configuration**:
   ```sql
   CREATE USER TABLE messages (...)
   WITH (deleted_retention = '7d');  -- Keep deleted rows for 7 days
   
   CREATE USER TABLE logs (...)
   WITH (deleted_retention = 'off');  -- Never cleanup deleted rows
   ```

2. **Automated Cleanup Jobs**:
   - Background scheduler runs cleanup jobs based on table retention policies
   - Jobs scan for rows where _deleted = true AND _updated < (NOW() - retention_period)
   - Physical removal of eligible soft-deleted rows from both RocksDB and Parquet
   - Cleanup jobs appear in system.jobs table with job_type='cleanup_deleted'

3. **Cleanup Job Execution**:
   ```sql
   -- View active cleanup jobs
   SELECT * FROM system.jobs 
   WHERE job_type = 'cleanup_deleted' AND status = 'running';
   
   -- Cleanup job details
   {
     job_id: "cleanup_12345",
     job_type: "cleanup_deleted",
     table_name: "messages",
     namespace: "production",
     user_id: "user123",  -- for user tables
     status: "running",
     parameters: ["retention_period=7d", "threshold_timestamp=2025-10-08T00:00:00Z"],
     result: "rows_removed: 1543, bytes_freed: 45MB",
     trace: "node-01, cleanup scheduler",
     start_time: "2025-10-15T10:00:00Z",
     end_time: "2025-10-15T10:05:23Z"
   }
   ```

4. **Implementation Considerations** (for future work):
   - Cleanup jobs must not interfere with active queries or live subscriptions
   - Parquet file rewriting required to physically remove deleted rows (compaction)
   - RocksDB tombstone cleanup for deleted rows
   - Transaction coordination to ensure consistency during cleanup
   - Progress tracking for long-running cleanup operations on large tables
   - Configurable cleanup schedule (e.g., daily, weekly)
   - Resource limits for cleanup jobs (max memory, max duration)

**Why defer this?**: Implementing physical deletion with Parquet file compaction adds complexity around concurrent access, transaction management, and resource control. The soft delete mechanism provides all core functionality (change tracking, CDC, data isolation). Cleanup can be added later when storage optimization becomes a priority.

**User Workaround**: Until cleanup jobs are implemented, users can:
- Query soft-deleted rows explicitly if needed: `SELECT * FROM table WHERE _deleted = true`
- Set deleted_retention='off' to retain all deleted rows indefinitely
- Manually manage storage if soft-deleted rows accumulate significantly

---

### Distributed Cluster Architecture with Specialized Nodes

**Note**: The following distributed cluster architecture is **NOT** part of the current feature scope. It is documented here for future planning.

**Cluster Node Types**:

KalamDB can implement a distributed cluster with two specialized node types, each optimized for different workloads:

1. **Database Server Replica Nodes** (`database-server-replica`):
   - Full-featured database nodes with REST API, core engine, and storage
   - Can handle write operations and buffer data in RocksDB
   - Persist data to disk/S3 storage
   - Participate in data replication and fault tolerance
   - Handle both CRUD operations and live query subscriptions
   - Run flush operations to convert buffered data to Parquet
   - Suitable for: Primary write workloads, data durability, backup operations

2. **Live Query Nodes** (`live-query`):
   - Specialized nodes for serving live WebSocket connections only
   - Do NOT persist data to disk (ephemeral, stateless)
   - Handle high volumes of concurrent WebSocket subscriptions
   - Receive change notifications from database-server-replica nodes
   - Forward real-time updates to connected subscribers
   - Optimized for: Connection handling, minimal latency, horizontal scaling
   - Can scale independently based on active subscriber count

**Node Management via System Table**:

The cluster should provide a `system.cluster_nodes` table for administrators to view and manage all nodes:

```sql
-- View all cluster nodes
SELECT * FROM system.cluster_nodes;

-- Columns: node_id, node_type, address, status, created_at, last_heartbeat, 
--          active_connections (for live-query nodes), 
--          buffered_rows (for database-server-replica nodes),
--          cpu_usage, memory_usage
```

**Architecture Benefits**:
- **Separation of Concerns**: Write-heavy workloads separate from connection-heavy workloads
- **Independent Scaling**: Scale live-query nodes for more subscribers, database-server-replica for more writes
- **Resource Optimization**: Live-query nodes don't need disk I/O, reducing infrastructure costs
- **Fault Tolerance**: Multiple database-server-replica nodes provide redundancy
- **Geographic Distribution**: Place live-query nodes close to users for low latency

**Implementation Considerations** (for future work):
- Node discovery and registration mechanism
- Health check and heartbeat protocols
- Load balancing for WebSocket connections across live-query nodes
- Change notification routing from database-server-replica to live-query nodes
- Node failure detection and automatic failover
- Metrics collection and monitoring per node type

**Why defer this?**: Distributed cluster architecture adds significant operational complexity. The current single-node architecture should be proven stable first, with clear performance bottlenecks identified before introducing node specialization.

## SQL Syntax Examples

### REST API Usage
```bash
# Single SQL command via REST API
curl -X POST http://localhost:8080/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "sql": "SELECT * FROM messages WHERE conversation_id = '\''conv123'\''"
  }'

# Response
{
  "status": "success",
  "results": [
    {
      "columns": ["message_id", "conversation_id", "content", "_updated", "_deleted"],
      "rows": [
        [1, "conv123", "Hello", "2025-10-15T10:00:00Z", false]
      ],
      "row_count": 1
    }
  ],
  "execution_time_ms": 45
}

# Multiple SQL commands in one request
curl -X POST http://localhost:8080/api/sql \
  -H "Content-Type: application/json" \
  -d '{
    "sql": "CREATE USER TABLE messages (message_id BIGINT, content STRING) LOCATION '\''/data/users/${user_id}/messages/'\''; INSERT INTO messages (content) VALUES ('\''Hello'\''); SELECT * FROM messages;"
  }'

# Response with results for each statement
{
  "status": "success",
  "results": [
    {"statement": "CREATE USER TABLE...", "status": "success"},
    {"statement": "INSERT INTO...", "affected_rows": 1},
    {"statement": "SELECT...", "rows": [...], "row_count": 1}
  ]
}

# WebSocket subscription with multiple queries
# Connect to: ws://localhost:8080/ws
# Send subscription message:
{
  "subscriptions": [
    {
      "id": "sub1",
      "sql": "SELECT * FROM messages WHERE conversation_id = '\''conv123'\''",
      "options": {
        "last_rows": 10  // Fetch last 10 rows initially
      }
    },
    {
      "id": "sub2", 
      "sql": "SELECT * FROM messages WHERE conversation_id = '\''conv456'\''",
      "options": {
        "last_rows": 5
      }
    }
  ]
}

# Initial data response (immediately after subscription)
{
  "type": "initial_data",
  "subscription_id": "sub1",
  "rows": [
    {"message_id": 1, "conversation_id": "conv123", "content": "Hello", "_updated": "2025-10-15T10:00:00Z", "_deleted": false},
    {"message_id": 2, "conversation_id": "conv123", "content": "Hi", "_updated": "2025-10-15T10:05:00Z", "_deleted": false}
  ],
  "row_count": 2
}

# Change notification (real-time)
{
  "type": "change",
  "subscription_id": "sub1",
  "change_type": "INSERT",
  "timestamp": "2025-10-15T12:00:00Z",
  "new_values": {
    "message_id": 3,
    "conversation_id": "conv123",
    "content": "New message",
    "_updated": "2025-10-15T12:00:00Z",
    "_deleted": false
  }
}

# UPDATE notification
{
  "type": "change",
  "subscription_id": "sub1",
  "change_type": "UPDATE",
  "timestamp": "2025-10-15T12:05:00Z",
  "old_values": {"message_id": 3, "content": "New message", "_updated": "2025-10-15T12:00:00Z"},
  "new_values": {"message_id": 3, "content": "Updated message", "_updated": "2025-10-15T12:05:00Z"}
}

# DELETE notification (soft delete)
{
  "type": "change",
  "subscription_id": "sub2",
  "change_type": "DELETE",
  "timestamp": "2025-10-15T12:10:00Z",
  "old_values": {
    "message_id": 5,
    "conversation_id": "conv456",
    "content": "Deleted message",
    "_updated": "2025-10-15T12:10:00Z",
    "_deleted": true
  }
}
```

### Namespace Management
```sql
-- Create namespace
CREATE NAMESPACE production;

-- List namespaces
SHOW NAMESPACES;

-- Edit namespace (options structure TBD)
ALTER NAMESPACE production SET OPTIONS (retention_days = 90);

-- Drop namespace
DROP NAMESPACE production;
```

### System Users Management
```sql
-- Add user to system
INSERT INTO system.users (user_id, username, permissions) 
VALUES ('user123', 'john_doe', ARRAY[
    'production.shared_table1.user_id = CURRENT_USER()',
    'production.messages.*'
]);

-- Update user permissions
UPDATE system.users 
SET permissions = ARRAY[
    'production.*.created_by = CURRENT_USER()',
    'analytics.*'
]
WHERE user_id = 'user123';

-- View user permissions
SELECT * FROM system.users WHERE user_id = 'user123';
```

### System Storage Locations Management
```sql
-- Add storage location
INSERT INTO system.storage_locations (location_name, location_type, path, credentials_ref)
VALUES ('s3-prod', 's3', 's3://prod-bucket/data/${user_id}/', 'aws-prod-creds');

INSERT INTO system.storage_locations (location_name, location_type, path)
VALUES ('local-dev', 'filesystem', '/var/data/kalamdb/${user_id}/');

-- View all storage locations
SELECT * FROM system.storage_locations;

-- View storage location usage
SELECT location_name, usage_count 
FROM system.storage_locations 
WHERE usage_count > 0;

-- Update storage location (affects new tables only)
UPDATE system.storage_locations 
SET path = 's3://new-prod-bucket/data/${user_id}/'
WHERE location_name = 's3-prod';
```

### System Live Queries Monitoring
```sql
-- View all active live subscriptions
SELECT * FROM system.live_queries;

-- View user's own subscriptions
SELECT * FROM system.live_queries WHERE user_id = CURRENT_USER();

-- View subscriptions by bandwidth usage
SELECT id, user_id, query, bandwidth 
FROM system.live_queries 
ORDER BY bandwidth DESC 
LIMIT 10;

-- Monitor specific user's subscriptions
SELECT * FROM system.live_queries 
WHERE user_id = 'user123';

-- Count active subscriptions per user
SELECT user_id, COUNT(*) as active_subscriptions
FROM system.live_queries
GROUP BY user_id;
```

### System Jobs Monitoring
```sql
-- View all active jobs
SELECT * FROM system.jobs 
WHERE status = 'running';

-- View completed jobs from the last hour
SELECT job_id, job_type, table_name, start_time, end_time, result, trace
FROM system.jobs 
WHERE status = 'completed' 
AND start_time > NOW() - INTERVAL '1h'
ORDER BY start_time DESC;

-- View failed jobs with errors
SELECT job_id, job_type, table_name, start_time, error_message, trace
FROM system.jobs 
WHERE status = 'failed'
ORDER BY start_time DESC;

-- View job performance metrics
SELECT 
    job_id,
    job_type,
    table_name,
    memory_used_mb,
    cpu_used_percent,
    result,
    EXTRACT(EPOCH FROM (end_time - start_time)) as duration_seconds
FROM system.jobs 
WHERE status = 'completed'
ORDER BY duration_seconds DESC
LIMIT 10;

-- Monitor jobs by node (in distributed setup)
SELECT node_id, job_type, COUNT(*) as job_count
FROM system.jobs 
WHERE status = 'completed'
GROUP BY node_id, job_type;

-- View user's own jobs
SELECT * FROM system.jobs 
WHERE user_id = CURRENT_USER()
ORDER BY start_time DESC;

-- View jobs with specific parameters
SELECT job_id, job_type, parameters, result
FROM system.jobs
WHERE 'table_name=messages' = ANY(parameters);

-- View flush jobs specifically
SELECT job_id, table_name, result, trace, start_time, end_time
FROM system.jobs
WHERE job_type = 'flush'
ORDER BY start_time DESC;

-- View cleanup jobs
SELECT job_id, job_type, parameters, result
FROM system.jobs
WHERE job_type = 'cleanup'
ORDER BY start_time DESC;
```

### User Table Operations
```sql
-- Create user table with auto-increment and location template
-- System automatically adds: _updated TIMESTAMP, _deleted BOOLEAN
CREATE USER TABLE messages (
    message_id BIGINT,
    conversation_id STRING,
    created_at TIMESTAMP,
    content STRING
)
LOCATION 's3://flowdb-data/users/${user_id}/messages/';

-- Create user table using predefined storage location
CREATE USER TABLE messages (
    message_id BIGINT,
    conversation_id STRING,
    created_at TIMESTAMP,
    content STRING
)
LOCATION REFERENCE 's3-prod';

-- Insert data (system sets _updated = NOW(), _deleted = false)
INSERT INTO messages (conversation_id, content)
VALUES ('conv123', 'Hello world');

-- Update data (system updates _updated = NOW())
UPDATE messages 
SET content = 'Updated message'
WHERE message_id = 12345;

-- Soft delete (system sets _deleted = true, _updated = NOW())
DELETE FROM messages WHERE message_id = 12345;

-- Query active messages (soft-deleted rows excluded by default)
SELECT * FROM messages WHERE conversation_id = 'conv123';
-- Equivalent to: SELECT * FROM messages WHERE conversation_id = 'conv123' AND _deleted = false

-- Query including soft-deleted rows (explicit filter)
SELECT * FROM messages WHERE conversation_id = 'conv123' AND _deleted = true;

-- Query changes since timestamp (uses _updated bloom filter)
SELECT * FROM messages WHERE _updated > '2025-10-14T00:00:00Z';
```
```
    content STRING
)
LOCATION 's3://flowdb-data/users/${user_id}/messages/';

-- Create user table with flush policy (row-based)
CREATE USER TABLE events (
    event_id BIGINT,
    event_type STRING,
    payload STRING
)
FLUSH POLICY ROWS 10000;

-- Create user table with soft delete retention
CREATE USER TABLE messages (
    message_id BIGINT,
    conversation_id STRING,
    content STRING
)
WITH (deleted_retention = '7d');  -- Keep deleted rows for 7 days

-- Create user table with no deleted row cleanup
CREATE USER TABLE audit_logs (
    log_id BIGINT,
    action STRING,
    details STRING
)
WITH (deleted_retention = 'off');  -- Never cleanup deleted rows

-- Create user table with flush policy (time-based)
CREATE USER TABLE logs (
    log_id BIGINT,
    message STRING,
    created_at TIMESTAMP
)
FLUSH POLICY INTERVAL '5 minutes';

-- Create user table with both flush policies
CREATE USER TABLE metrics (
    metric_id BIGINT,
    metric_name STRING,
    value DOUBLE,
    timestamp TIMESTAMP
)
FLUSH POLICY ROWS 5000 INTERVAL '1 minute';
```

### Stream Table Operations
```sql
-- Create stream table for ephemeral events (AI chat signals)
CREATE STREAM TABLE ai_signals (
    user_id STRING,
    conversation_id STRING,
    event_type STRING,  -- 'ai_typing', 'ai_thinking', 'user_offline', etc.
    payload JSON,
    created_at TIMESTAMP
)
WITH (
    retention = '10s',
    ephemeral = true,
    max_buffer = 1000
);

-- Insert ephemeral events
INSERT INTO ai_signals (user_id, conversation_id, event_type, payload, created_at)
VALUES ('user123', 'conv456', 'ai_typing', '{"text": ""}', NOW());

-- Subscribe to stream table events (real-time only)
SUBSCRIBE SELECT * FROM ai_signals 
WHERE conversation_id = 'conv456';

-- Drop stream table
DROP STREAM TABLE ai_signals;
```

### ALTER TABLE Operations (Schema Evolution)
```sql
-- Add a new column to existing user table
ALTER TABLE messages 
ADD COLUMN priority STRING DEFAULT 'normal';

-- Add multiple columns
ALTER TABLE messages
ADD COLUMN tags ARRAY<STRING>,
ADD COLUMN metadata JSON;

-- Drop a column (must not be referenced in active subscriptions)
ALTER TABLE messages
DROP COLUMN priority;

-- Modify column type (with validation)
ALTER TABLE messages
MODIFY COLUMN content TEXT;  -- Expand VARCHAR to TEXT

-- View schema history
DESCRIBE TABLE messages;
-- Output shows:
-- - current_version: 3
-- - schema_path: /configs/production/schemas/messages/current.json
-- - schema_history:
--   - v1 (2025-01-15): Initial schema
--   - v2 (2025-03-20): Added priority column
--   - v3 (2025-10-15): Removed priority, added tags and metadata

-- Querying after schema evolution
-- Old Parquet files (v1) will have NULL for new columns (tags, metadata)
SELECT * FROM messages WHERE tags IS NOT NULL;

-- Notes:
-- - ALTER TABLE NOT supported on stream tables (immutable)
-- - Cannot alter system columns (_updated, _deleted)
-- - Schema changes must be backwards-compatible with existing Parquet files
-- - Dropping columns fails if referenced in active live query subscriptions
```

### Live Query Subscriptions with Change Tracking
```sql
-- WebSocket subscription with single query
{
  "subscriptions": [
    {
      "id": "messages_sub",
      "sql": "SELECT * FROM messages WHERE conversation_id = 'conv456'",
      "options": {
        "last_rows": 20  // Get last 20 rows immediately
      }
    }
  ]
}

-- WebSocket subscription with multiple queries
{
  "subscriptions": [
    {
      "id": "conv1",
      "sql": "SELECT * FROM messages WHERE conversation_id = 'conv123'",
      "options": {"last_rows": 10}
    },
    {
      "id": "conv2",
      "sql": "SELECT * FROM messages WHERE conversation_id = 'conv456'",
      "options": {"last_rows": 10}
    },
    {
      "id": "recent",
      "sql": "SELECT * FROM messages WHERE _updated > '2025-10-14T00:00:00Z'",
      "options": {"last_rows": 50}
    }
  ]
}

-- Initial data response (for each subscription)
{
  "type": "initial_data",
  "subscription_id": "conv1",
  "rows": [
    {"message_id": 100, "conversation_id": "conv123", "content": "Message 1", "_updated": "2025-10-15T09:00:00Z", "_deleted": false},
    {"message_id": 101, "conversation_id": "conv123", "content": "Message 2", "_updated": "2025-10-15T09:30:00Z", "_deleted": false}
  ]
}

-- INSERT notification
{
  "type": "change",
  "subscription_id": "conv1",
  "change_type": "INSERT",
  "timestamp": "2025-10-15T12:00:00Z",
  "new_values": {
    "message_id": 102,
    "conversation_id": "conv123",
    "content": "New message",
    "_updated": "2025-10-15T12:00:00Z",
    "_deleted": false
  }
}

-- UPDATE notification
{
  "type": "change",
  "subscription_id": "conv1",
  "change_type": "UPDATE",
  "timestamp": "2025-10-15T12:05:00Z",
  "old_values": {
    "message_id": 102,
    "content": "New message",
    "_updated": "2025-10-15T12:00:00Z"
  },
  "new_values": {
    "message_id": 102,
    "content": "Updated message",
    "_updated": "2025-10-15T12:05:00Z"
  }
}

-- DELETE notification (soft delete)
{
  "type": "change",
  "subscription_id": "conv2",
  "change_type": "DELETE",
  "timestamp": "2025-10-15T12:10:00Z",
  "old_values": {
    "message_id": 200,
    "conversation_id": "conv456",
    "content": "Deleted message",
    "_updated": "2025-10-15T12:10:00Z",
    "_deleted": true
  }
}

-- Note: All subscriptions in the same WebSocket connection receive
-- their respective notifications independently
```

### Backup and Restore
```sql
-- Backup namespace (syntax inspired by DuckDB)
BACKUP DATABASE production TO 's3://backups/production_2025-10-14.backup';

-- Restore namespace
RESTORE DATABASE production FROM 's3://backups/production_2025-10-14.backup';

-- List backup contents
SHOW BACKUP 's3://backups/production_2025-10-14.backup';

-- Future: incremental backup
BACKUP DATABASE production TO 's3://backups/production_incremental.backup' 
MODE INCREMENTAL;
```

### Catalog Queries
```sql
-- List tables in current namespace
SHOW TABLES;
-- Output:
-- | table_name   | table_type | storage_location                      | flush_policy        |
-- | messages     | USER       | /data/users/${user_id}/messages/      | ROWS 10000          |
-- | conversations| SHARED     | /data/shared/conversations/           | ROWS 1000, 10m      |
-- | ai_signals   | STREAM     | N/A (ephemeral)                       | N/A                 |

-- Describe table structure
DESCRIBE TABLE messages;
-- Output includes:
-- Columns: message_id, conversation_id, created_at, content, _updated, _deleted
-- Auto-increment: message_id (snowflake)
-- System columns: _updated (TIMESTAMP), _deleted (BOOLEAN)
-- Storage: /data/users/${user_id}/messages/
-- Flush policy: ROWS 10000
-- Deleted retention: 7d

-- Show table stats
SHOW TABLE STATS messages;

-- View table metadata
SELECT * FROM information_schema.tables 
WHERE table_name = 'messages';

-- List system tables
SELECT * FROM information_schema.tables 
WHERE table_type = 'SYSTEM';
```
