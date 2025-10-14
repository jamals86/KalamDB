# Feature Specification: Simple KalamDB - User-Based Database with Live Queries

**Feature Branch**: `002-simple-kalamdb`  
**Created**: 2025-10-14  
**Status**: Draft  
**Input**: User description: "Simple user-based database with namespaces, user/shared/system tables, live queries with change tracking, and flush policies"

## User Scenarios & Testing *(mandatory)*

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

A database administrator or user wants to monitor active live query subscriptions in the system. They need to view all active subscriptions including connection details, bandwidth usage, and query information.

**Why this priority**: Visibility into active subscriptions is critical for monitoring, debugging, and resource management.

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

### User Story 2c - Distributed Replication via System Nodes (Priority: P1)

A database administrator wants to configure and monitor distributed replication nodes through a system table. When creating tables, they can specify replication policies by node count or node tags, and the system automatically replicates data during flush operations.

**Why this priority**: Distributed replication is foundational for data durability, high availability, and disaster recovery in production systems.

**Independent Test**: Can be fully tested by adding nodes to system.nodes, creating tables with replica policies, and verifying data is replicated to target nodes during flush operations.

**Acceptance Scenarios**:

1. **Given** I am a database administrator, **When** I add nodes to `system.nodes` with node_id, name, address (ip:port), and tags (e.g., ["us-east", "production"]), **Then** the nodes are registered and available for replication
2. **Given** nodes exist with different tags, **When** I create a table with `REPLICAS 3`, **Then** the system selects 3 nodes for replication based on availability and load balancing
3. **Given** nodes exist with tags, **When** I create a table with `REPLICA TAGS ['us-east', 'eu-west']`, **Then** the system replicates data only to nodes matching those tags
4. **Given** a table has a replica policy, **When** buffered rows in RocksDB reach the flush policy threshold, **Then** the system flushes data to S3/filesystem AND replicates to configured replica nodes
5. **Given** I query `SELECT * FROM system.nodes`, **When** I view node information, **Then** I see node_id, name, address, tags, created_at, updated_at, and connection status
6. **Given** a replica node becomes unavailable, **When** flush operation occurs, **Then** the system logs the failure and attempts retry or selects an alternative node with matching tags
7. **Given** multiple tables use the same replica nodes, **When** I monitor system.nodes, **Then** I can see which tables are replicating to each node

---

### User Story 3 - User Table Creation and Management (Priority: P1)

A developer wants to create user-scoped tables where each user has their own isolated table instance. They need to define the table schema with auto-increment fields, specify storage locations with user ID templates, configure flush policies, and optionally set up replication.

**Why this priority**: User tables are the core data model for isolated, per-user data storage. Essential for multi-tenant scenarios.

**Independent Test**: Can be fully tested by creating a user table definition, inserting data for different users, and verifying data isolation per user ID.

**Acceptance Scenarios**:

1. **Given** I am in namespace "production", **When** I create a user table "messages" with schema (message_id BIGINT, content STRING), **Then** the table definition is created
2. **Given** I create a user table "messages", **When** I specify a storage location with user ID template like `s3://bucket/users/${user_id}/messages/`, **Then** each user's data is stored in their own path
3. **Given** I create a user table, **When** I don't specify an auto-increment field, **Then** the system automatically adds a snowflake ID field
4. **Given** a user table "messages" exists, **When** user "user123" inserts data, **Then** the data is stored at the user-specific location and isolated from other users
5. **Given** a user table "messages" exists, **When** I query data for user "user123", **Then** I only see data belonging to that user
6. **Given** I create a user table, **When** I specify a flush policy with row limit, **Then** data is flushed to disk when the limit is reached
7. **Given** I create a user table, **When** I specify a flush policy with time interval, **Then** data is flushed to disk at the specified intervals
8. **Given** I create a user table, **When** I specify `REPLICAS 3`, **Then** the system configures replication to 3 nodes
9. **Given** I create a user table, **When** I specify `REPLICA TAGS ['us-east', 'eu-west']`, **Then** the system configures replication only to nodes with those tags
10. **Given** a user table has replica configuration, **When** flush policy triggers, **Then** data is written to primary storage AND replicated to configured nodes

---

### User Story 4 - Shared Table Creation and Management (Priority: P1)

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

### User Story 5 - Live Query Subscriptions with Change Tracking (Priority: P2)

A user wants to subscribe to real-time changes on their user table with filtered queries. When data matching their query criteria is inserted, updated, or deleted, they receive notifications with the change type and affected rows.

**Why this priority**: Live queries enable real-time applications, but basic CRUD operations are needed first.

**Independent Test**: Can be fully tested by subscribing to a filtered query, performing insert/update/delete operations, and verifying the subscriber receives all change notifications with correct change types.

**Acceptance Scenarios**:

1. **Given** user "user123" has a table "messages", **When** they subscribe to `SELECT * FROM messages WHERE conversation_id = 'conv456'`, **Then** the subscription is created and active
2. **Given** a subscription exists for conversation_id = 'conv456', **When** a message with that conversation_id is inserted, **Then** the subscriber receives an INSERT notification with the new row
3. **Given** a subscription exists, **When** a matching row is updated, **Then** the subscriber receives an UPDATE notification with old and new row values
4. **Given** a subscription exists, **When** a matching row is deleted, **Then** the subscriber receives a DELETE notification with the deleted row data
5. **Given** a subscription exists, **When** data not matching the filter is changed, **Then** the subscriber does not receive any notification
6. **Given** multiple subscriptions exist for the same user, **When** data is changed, **Then** only matching subscriptions receive notifications
7. **Given** a subscription is active, **When** the subscriber disconnects, **Then** the subscription is cleaned up automatically

---

### User Story 6 - Table Export Functionality (Priority: P2)

A user wants to export data from tables to files for backup, migration, or external processing. They need a simple SQL-like syntax to export entire tables or query results.

**Why this priority**: Data portability is important but secondary to core data storage and query capabilities.

**Independent Test**: Can be fully tested by exporting a table to a file and verifying the file contains all expected data in the correct format.

**Acceptance Scenarios**:

1. **Given** a user table "messages" has data for user "user123", **When** I execute `EXPORT FROM messages TO 'backup.parquet'`, **Then** the user's data is exported to the file
2. **Given** a shared table "conversations" exists, **When** I execute `EXPORT FROM conversations TO 'conversations.parquet'`, **Then** all shared data is exported (filtered by permissions)
3. **Given** I want to export filtered data, **When** I execute `EXPORT FROM (SELECT * FROM messages WHERE created_at > '2025-01-01') TO 'recent.parquet'`, **Then** only matching rows are exported
4. **Given** I export to a file, **When** the export completes, **Then** I receive a confirmation with row count and file size

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
5. **Given** I want to see table dependencies, **When** I query table metadata, **Then** I see which storage locations and replicas are configured

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
- What happens when export is attempted on a table the user has limited permission to access?
- What happens when backup is attempted on a namespace that is actively being written to?
- How does the system handle user IDs with special characters in storage location paths?
- What happens when S3 storage is unavailable during a flush operation?
- How does the system handle concurrent exports of the same table?
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
- What happens when creating a table with REPLICAS N but fewer than N nodes are available?
- What happens when creating a table with REPLICA TAGS but no nodes match those tags?
- How does the system handle flush operations when replica nodes are unreachable?
- What happens when a replica node fails during a flush operation?
- How does the system handle adding nodes to system.nodes with duplicate node_id or address?
- What happens when multiple tables replicate to the same node simultaneously during flush?
- How does the system handle replica node tag updates when tables are already using those tags?
- What happens when buffered data in RocksDB needs to flush but the replica policy cannot be satisfied?
- How does the system handle partial replication failures (some nodes succeed, some fail)?

## Requirements *(mandatory)*

### Functional Requirements

#### Namespace Management
- **FR-001**: System MUST allow creation of namespaces with unique names
- **FR-002**: System MUST support listing all namespaces with their metadata (creation date, table count, options)
- **FR-003**: System MUST allow editing namespace options (options structure to be defined in implementation)
- **FR-004**: System MUST allow dropping empty namespaces
- **FR-005**: System MUST prevent deletion of namespaces that contain tables and show which tables exist
- **FR-006**: System MUST enforce namespace name uniqueness across the entire database

#### System Tables and User Management
- **FR-007**: System MUST provide a system users table containing all users added to the system
- **FR-008**: System MUST store user permissions in the system users table
- **FR-009**: System MUST support row-level permissions using column value filters (e.g., `namespace.shared_table1.field1 = CURRENT_USER()`)
- **FR-010**: System MUST support regex-based permissions for table access patterns (e.g., `namespace.messages.*`)
- **FR-011**: System MUST automatically apply permission filters to all user queries
- **FR-012**: System MUST support CURRENT_USER() function in permission expressions
- **FR-013**: System MUST enforce permission checks before allowing any data access
- **FR-014**: System MUST handle multiple permissions per user with appropriate precedence rules

#### System Live Queries Table
- **FR-015**: System MUST provide a system.live_queries table showing all active live query subscriptions
- **FR-016**: System.live_queries table MUST include columns: id, user_id, query, created_at, bandwidth
- **FR-017**: System MUST automatically add entries to system.live_queries when subscriptions are created
- **FR-018**: System MUST automatically remove entries from system.live_queries when subscriptions disconnect
- **FR-019**: System MUST update bandwidth metrics in real-time for active subscriptions
- **FR-020**: System MUST apply user permissions to system.live_queries queries (users see only their own subscriptions by default)
- **FR-021**: System MUST track bandwidth usage per subscription (bytes sent, messages sent, or similar metric)

#### System Storage Locations Table
- **FR-022**: System MUST provide a system.storage_locations table for predefined storage locations
- **FR-023**: System.storage_locations table MUST include columns: location_name, location_type, path, credentials_ref, created_at, usage_count
- **FR-024**: System MUST allow administrators to add storage locations to system.storage_locations
- **FR-025**: System MUST support LOCATION REFERENCE '<name>' syntax for table creation using predefined locations
- **FR-026**: System MUST resolve location references from system.storage_locations at table creation time
- **FR-027**: System MUST track usage count showing how many tables reference each storage location
- **FR-028**: System MUST prevent deletion of storage locations that are referenced by existing tables
- **FR-029**: System MUST enforce unique location names in system.storage_locations
- **FR-030**: System MUST validate storage location accessibility when adding to system.storage_locations

#### System Nodes Table and Distributed Replication
- **FR-031**: System MUST provide a system.nodes table for managing distributed replication nodes
- **FR-032**: System.nodes table MUST include columns: node_id, name, address (ip:port format), tags (array), created_at, updated_at
- **FR-033**: System MUST allow administrators to add nodes to system.nodes with unique node_id and address
- **FR-034**: System MUST support node tags as an array of strings for logical grouping (e.g., ["us-east", "production"])
- **FR-035**: System MUST enforce unique node_id and address in system.nodes
- **FR-036**: System MUST validate node address connectivity when adding to system.nodes
- **FR-037**: System MUST support REPLICAS N syntax for table creation to specify number of replica nodes
- **FR-038**: System MUST support REPLICA TAGS ['tag1', 'tag2'] syntax to specify nodes by tags
- **FR-039**: System MUST select replica nodes based on availability, tags, and load balancing when REPLICAS N is specified
- **FR-040**: System MUST select only nodes matching specified tags when REPLICA TAGS is specified
- **FR-041**: System MUST fail table creation if REPLICA TAGS match no available nodes
- **FR-042**: System MUST warn if REPLICAS N is specified but fewer than N nodes are available
- **FR-043**: System MUST flush buffered data from RocksDB to primary storage (S3/filesystem) when flush policy triggers
- **FR-044**: System MUST replicate flushed data to configured replica nodes immediately after primary storage flush
- **FR-045**: System MUST copy the exact Parquet files written to primary storage to replica nodes during flush
- **FR-046**: System MUST track replication status per flush operation (success/failure per node)
- **FR-047**: System MUST retry failed replications with exponential backoff up to a configurable limit
- **FR-048**: System MUST log replication failures and alert administrators
- **FR-049**: System MUST support querying system.nodes to view node status, connectivity, and replication health
- **FR-050**: System MUST handle partial replication failures gracefully without blocking primary storage writes

#### User Table Management
- **FR-051**: System MUST support creating user tables where each user ID gets their own table instance
- **FR-052**: System MUST support CREATE USER TABLE syntax with column definitions
- **FR-053**: System MUST always add auto-increment fields to user tables (snowflake IDs by default)
- **FR-054**: System MUST support LOCATION clause with user ID templating like `s3://bucket/path/${user_id}/table/`
- **FR-055**: System MUST support LOCATION REFERENCE '<name>' syntax to use predefined storage locations
- **FR-056**: System MUST substitute ${user_id} in storage paths with actual user IDs at runtime
- **FR-057**: System MUST isolate data between different user IDs in user tables
- **FR-058**: System MUST support DataFusion-compatible datatypes for user table columns
- **FR-059**: System MUST store user table data in Parquet format
- **FR-060**: System MUST support flush-to-disk policy with row limit configuration
- **FR-061**: System MUST support flush-to-disk policy with time-based interval configuration
- **FR-062**: System MUST flush data to disk when either row limit or time interval is reached
- **FR-063**: System MUST support replica configuration with REPLICAS N or REPLICA TAGS syntax
- **FR-064**: System MUST combine flush policies with replica policies (flush triggers both storage and replication)

#### Shared Table Management  
- **FR-065**: System MUST support creating shared tables accessible to all users in a namespace
- **FR-066**: System MUST support CREATE SHARED TABLE syntax with column definitions
- **FR-067**: System MUST store shared table data in a single location (no user ID templating)
- **FR-068**: System MUST apply user permissions to shared table access
- **FR-069**: System MUST support DataFusion-compatible datatypes for shared table columns
- **FR-070**: System MUST store shared table data in Parquet format
- **FR-071**: System MUST support flush-to-disk policy with row limit configuration for shared tables
- **FR-072**: System MUST support flush-to-disk policy with time-based interval configuration for shared tables
- **FR-073**: System MUST support replica configuration for shared tables with REPLICAS N or REPLICA TAGS syntax
- **FR-074**: System MUST replicate shared table data to configured nodes during flush operations

#### Storage Management
- **FR-075**: System MUST support filesystem storage backend with configurable paths
- **FR-076**: System MUST support S3 storage backend with bucket and credential configuration
- **FR-077**: System MUST validate storage location accessibility before table creation
- **FR-078**: System MUST support storage location templates with ${user_id} variable substitution
- **FR-079**: System MUST track which tables use which storage locations

#### Live Query Subscriptions with Change Tracking
- **FR-080**: System MUST allow users to subscribe to live queries on their user tables
- **FR-081**: System MUST support filtered subscriptions with WHERE clauses (e.g., `SELECT * FROM messages WHERE conversation_id = 'testid'`)
- **FR-082**: System MUST notify subscribers when matching data is inserted with INSERT change type
- **FR-083**: System MUST notify subscribers when matching data is updated with UPDATE change type and both old and new values
- **FR-084**: System MUST notify subscribers when matching data is deleted with DELETE change type and deleted row data
- **FR-085**: System MUST only send notifications for data matching the subscription filter
- **FR-086**: System MUST isolate subscriptions per user ID (users can only subscribe to their own data)
- **FR-087**: System MUST automatically clean up subscriptions when clients disconnect
- **FR-088**: System MUST support multiple concurrent subscriptions per user
- **FR-089**: System MUST register all active subscriptions in system.live_queries table

#### Table Export
- **FR-090**: System MUST support exporting table data to files using SQL-like syntax
- **FR-091**: System MUST support EXPORT FROM <table> TO <file> syntax for user tables
- **FR-092**: System MUST support EXPORT FROM <table> TO <file> syntax for shared tables
- **FR-093**: System MUST support exporting query results: `EXPORT FROM (SELECT ...) TO <file>`
- **FR-094**: System MUST export data in Parquet format by default
- **FR-095**: System MUST provide export confirmation with row count and file size
- **FR-096**: System MUST apply user permissions during export operations
- **FR-097**: System MUST handle file path conflicts during export operations

#### Namespace Backup and Restore
- **FR-098**: System MUST support backing up entire namespaces including all tables and data
- **FR-099**: System MUST support restoring namespaces from backup files
- **FR-100**: System MUST preserve all table schemas, data, and metadata during backup/restore
- **FR-101**: System MUST support listing backup contents without restoring
- **FR-102**: System MUST handle backup of namespaces with active write operations
- **FR-103**: System MUST support incremental backups (future enhancement, documented for planning)

#### Catalog and Introspection
- **FR-104**: System MUST provide SQL-like catalog queries to list namespaces
- **FR-105**: System MUST provide SQL-like catalog queries to list tables within a namespace
- **FR-106**: System MUST distinguish between user tables, shared tables, and system tables in catalog listings
- **FR-107**: System MUST support DESCRIBE TABLE commands to show table schema
- **FR-108**: System MUST show auto-increment field configuration in table descriptions
- **FR-109**: System MUST show storage location configuration in table descriptions
- **FR-110**: System MUST show flush policy configuration in table descriptions
- **FR-111**: System MUST show replica configuration in table descriptions
- **FR-112**: System MUST provide table statistics (row counts, storage size)
- **FR-113**: System MUST support querying table metadata including creation date and last modified date

### Key Entities

- **Namespace**: A logical container for tables and configuration. Attributes: name (unique), creation timestamp, options (key-value pairs), table count
- **System Users Table**: A system table containing user information and permissions. Attributes: user_id, username, permissions (array of permission rules), created_at, last_login
- **System Live Queries Table**: A system table showing active live query subscriptions. Attributes: id (subscription_id), user_id, query (SQL text), created_at, bandwidth (bytes/messages sent)
- **System Storage Locations Table**: A system table containing predefined storage locations. Attributes: location_name (unique), location_type (filesystem/s3), path, credentials_ref, created_at, usage_count
- **System Nodes Table**: A system table containing distributed replication nodes. Attributes: node_id (unique), name, address (ip:port format), tags (array of strings), created_at, updated_at, status, replication_health
- **Permission Rule**: Defines access control for a user. Attributes: rule_pattern (namespace.table.column or regex), filter_expression (e.g., `field1 = CURRENT_USER()`), rule_type (column_filter/regex)
- **User Table**: A table template where each user ID gets their own isolated table instance. Attributes: table_name, namespace, column definitions, auto-increment field, storage location template or reference, flush_policy, replica_policy (count or tags)
- **Shared Table**: A single table accessible by all users in a namespace (subject to permissions). Attributes: table_name, namespace, column definitions, storage location or reference, flush_policy, replica_policy (count or tags)
- **Flush Policy**: Configuration for when to write data to disk. Attributes: policy_type (row_limit/time_based), row_limit (optional), time_interval (optional)
- **Replica Policy**: Configuration for distributed replication. Attributes: policy_type (node_count/tag_based), node_count (optional), tags (optional array of strings), selected_nodes (resolved at table creation)
- **Replication Node**: A physical or virtual node for data replication. Attributes: node_id, name, address (ip:port), tags, connectivity_status, last_health_check, replication_lag
- **User**: Represents an authenticated user in the system. Attributes: user_id, username
- **Live Query Subscription**: An active real-time query subscription. Attributes: subscription_id, user_id, table_name, query_filter, connection_handle, change_types (INSERT/UPDATE/DELETE), bandwidth_metrics
- **Change Notification**: A notification sent to subscribers. Attributes: change_type (INSERT/UPDATE/DELETE), affected_rows, old_values (for UPDATE/DELETE), new_values (for INSERT/UPDATE), timestamp
- **Storage Location**: Physical storage configuration (now managed via system table). Attributes: location_name, location_type (filesystem/s3), path_template, credentials_ref, usage_count, accessibility_status
- **Backup**: A snapshot of a namespace. Attributes: backup_id, namespace, creation_timestamp, file_location, size, table_count

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
- **SC-010**: Table export operations complete at a rate of at least 100,000 rows per second
- **SC-011**: Namespace backup completes at a rate of at least 50 MB per second
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
- **SC-024**: System.nodes table supports at least 100 replication nodes with lookup under 5ms
- **SC-025**: Replication to configured nodes completes within 500ms of flush operation for files up to 10MB
- **SC-026**: System successfully replicates data to at least 95% of configured replica nodes during flush
- **SC-027**: Replica node selection based on tags completes in under 10ms
- **SC-028**: System handles replica node failures with automatic retry within 30 seconds
- **SC-029**: Buffered data in RocksDB flushes to primary storage and replicates to nodes within flush policy threshold + 1 second
- **SC-030**: System tracks replication health with status updates every 30 seconds in system.nodes table

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
- Live query subscriptions use WebSocket or similar persistent connection mechanism
- Notification delivery guarantees are "at least once" for live query subscriptions
- Change tracking (INSERT/UPDATE/DELETE) can be implemented efficiently without significant performance overhead
- Flush policies can be enforced at the table level without global synchronization
- Row-based and time-based flush policies can coexist, with flush happening when either condition is met
- Permission filters are simple enough to be translated into DataFusion query predicates
- CURRENT_USER() function can be resolved at query time from the session context
- Regex-based permissions are evaluated at query planning time, not at runtime
- System users table is protected and only accessible by administrators
- Export file formats default to Parquet but may support other formats in future
- Backup/restore operations are performed during maintenance windows or low-traffic periods
- Namespace options structure will be defined during implementation phase
- Replica nodes are accessible via network (ip:port) and support file copy operations
- Replication is synchronous during flush operations, with configurable retry mechanisms
- Replica node tags are static strings defined at node registration time
- Buffered data in RocksDB is written to primary storage first, then replicated to nodes
- Replication copies complete Parquet files to replica nodes, not incremental changes
- Replica node failures do not block primary storage writes
- System can select alternative replica nodes when configured nodes are unavailable
- Table schemas are defined at creation time and do not require dynamic schema evolution initially

## Dependencies

- DataFusion library for query execution and datatype support
- Parquet library for data serialization/deserialization
- S3 SDK (e.g., aws-sdk-s3 for Rust) for S3 storage backend
- Snowflake ID generator library or implementation
- RocksDB or similar metadata store for namespace/table catalog, system tables, and buffered writes
- File system access for local storage backend
- WebSocket or similar protocol for live query subscriptions
- Query parser for EXPORT syntax and catalog commands
- Template engine for ${user_id} path substitution
- Backup/restore library compatible with Parquet and metadata formats
- Change Data Capture (CDC) mechanism for tracking INSERT/UPDATE/DELETE operations
- Permissions evaluation engine for filter and regex-based access control
- Background task scheduler for time-based flush policies
- Network communication library for node-to-node replication (e.g., Tokio for async Rust)
- Health check mechanism for monitoring replica node availability
- Retry logic with exponential backoff for failed replication operations

## Out of Scope

- Table schema migration/evolution after creation
- Cross-namespace queries or data access
- Cross-user data access (strict user isolation)
- Advanced storage features like versioning, snapshots, or point-in-time recovery beyond basic backup
- Complex permission expressions beyond column filters and regex patterns (e.g., JOIN-based permissions)
- Role-based access control (RBAC) hierarchy - only direct user permissions
- Storage cost optimization or tiering automation
- Table partitioning strategies
- Custom export formats beyond Parquet (CSV, JSON support is future work)
- Incremental backup implementation (syntax reserved, implementation deferred)
- Live query subscriptions on shared tables (user tables only)
- Query optimization and performance tuning beyond basic DataFusion capabilities
- Distributed query execution across multiple nodes (data replication supported, not distributed queries)
- Transaction support and ACID guarantees beyond DataFusion's capabilities
- Data compression algorithm selection (use Parquet defaults)
- Audit logging of permission checks and data access
- Dynamic permission updates without system restart
- Permission inheritance or delegation mechanisms
- Cross-region replication with consistency guarantees
- Automatic failover to replica nodes for read operations
- Replica conflict resolution for write operations (write-only replication)
- Real-time replica lag monitoring and alerting
- Replica node auto-discovery and dynamic cluster membership
- Geo-distributed consensus protocols (Raft, Paxos)

## SQL Syntax Examples

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

### System Nodes Management and Replication
```sql
-- Add replication nodes
INSERT INTO system.nodes (node_id, name, address, tags)
VALUES ('node-001', 'US East Primary', '10.0.1.100:9000', ARRAY['us-east', 'production']);

INSERT INTO system.nodes (node_id, name, address, tags)
VALUES ('node-002', 'EU West Primary', '10.0.2.100:9000', ARRAY['eu-west', 'production']);

INSERT INTO system.nodes (node_id, name, address, tags)
VALUES ('node-003', 'US East Secondary', '10.0.1.101:9000', ARRAY['us-east', 'backup']);

-- View all nodes
SELECT * FROM system.nodes;

-- View nodes by tags
SELECT * FROM system.nodes 
WHERE ARRAY_CONTAINS(tags, 'production');

-- View node health status
SELECT node_id, name, address, status, updated_at
FROM system.nodes 
ORDER BY updated_at DESC;

-- Update node tags
UPDATE system.nodes 
SET tags = ARRAY['us-east', 'production', 'high-priority']
WHERE node_id = 'node-001';

-- Monitor replication health
SELECT node_id, name, replication_health 
FROM system.nodes 
WHERE replication_health < 0.95;  -- Less than 95% success rate
```

### User Table Operations
```sql
-- Create user table with auto-increment and location template
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

-- Create user table with replication by node count
CREATE USER TABLE messages (
    message_id BIGINT,
    conversation_id STRING,
    created_at TIMESTAMP,
    content STRING
)
LOCATION 's3://flowdb-data/users/${user_id}/messages/'
REPLICAS 3;

-- Create user table with replication by tags
CREATE USER TABLE events (
    event_id BIGINT,
    event_type STRING,
    payload STRING
)
LOCATION REFERENCE 's3-prod'
REPLICA TAGS ['us-east', 'eu-west'];

-- Create user table with flush policy and replication
-- When flush triggers (rows OR time), data is written to primary storage
-- and replicated to configured nodes
CREATE USER TABLE logs (
    log_id BIGINT,
    message STRING,
    created_at TIMESTAMP
)
FLUSH POLICY ROWS 10000 INTERVAL '5 minutes'
REPLICAS 2;

-- Complete example: user table with all options
CREATE USER TABLE metrics (
    metric_id BIGINT,
    metric_name STRING,
    value DOUBLE,
    timestamp TIMESTAMP
)
LOCATION REFERENCE 'metrics-storage'
FLUSH POLICY ROWS 5000 INTERVAL '1 minute'
REPLICA TAGS ['production', 'us-east'];
```

### Shared Table Operations
```sql
-- Create shared table with flush policy
CREATE SHARED TABLE conversations (
    conversation_id STRING,
    created_at TIMESTAMP,
    participants ARRAY<STRING>
)
LOCATION 's3://flowdb-data/shared/conversations/'
FLUSH POLICY ROWS 1000 INTERVAL '10 minutes';

-- Create shared table with replication
CREATE SHARED TABLE global_config (
    config_key STRING,
    config_value STRING,
    updated_at TIMESTAMP
)
LOCATION 's3://flowdb-data/shared/config/'
REPLICAS 5
FLUSH POLICY ROWS 100;
```

### Live Query Subscriptions with Change Tracking
```sql
-- Subscribe to filtered user table (user context: user123)
-- Returns INSERT, UPDATE, DELETE notifications
SUBSCRIBE SELECT * FROM messages 
WHERE conversation_id = 'conv456';

-- Subscribe to all changes (user context: user123)
SUBSCRIBE SELECT * FROM messages;

-- Notification format (conceptual):
-- {
--   "change_type": "INSERT" | "UPDATE" | "DELETE",
--   "timestamp": "2025-10-14T12:00:00Z",
--   "new_values": { ... },  -- for INSERT and UPDATE
--   "old_values": { ... }   -- for UPDATE and DELETE
-- }
```

### Export Operations
```sql
-- Export entire user table (respects permissions)
EXPORT FROM messages TO 's3://backups/user123_messages.parquet';

-- Export shared table (respects permissions)
EXPORT FROM conversations TO '/local/backups/conversations.parquet';

-- Export query results
EXPORT FROM (
    SELECT * FROM messages 
    WHERE created_at > '2025-01-01'
) TO 'recent_messages.parquet';
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

-- Describe table structure (includes flush policy)
DESCRIBE TABLE messages;
DESCRIBE messages;

-- Show table stats
SHOW TABLE STATS messages;

-- View table metadata
SELECT * FROM information_schema.tables 
WHERE table_name = 'messages';

-- List system tables
SELECT * FROM information_schema.tables 
WHERE table_type = 'SYSTEM';
```
