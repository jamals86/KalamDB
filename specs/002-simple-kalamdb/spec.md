# Feature Specification: Simple KalamDB - Entity-Based Database with Live Queries

**Feature Branch**: `002-simple-kalamdb`  
**Created**: 2025-10-14  
**Status**: Draft  
**Input**: User description: "Simple entity-based database with namespaces, entity/shared tables, live queries, and backup/export capabilities"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Schema Creation and Namespace Management (Priority: P1)

A database administrator wants to organize their data into logical namespaces (schemas) where each schema contains isolated user spaces. They need to create a schema that serves as the top-level namespace for all user data within that context.

**Why this priority**: This is the foundational capability that enables multi-tenancy and data isolation. Without schemas, no other functionality can work.

**Independent Test**: Can be fully tested by creating a schema via API/CLI and verifying it exists in the system catalog. Delivers the core namespace isolation needed for multi-tenant deployments.

**Acceptance Scenarios**:

1. **Given** I have database access, **When** I create a schema named "analytics", **Then** the schema is created and appears in the schema list
2. **Given** a schema "analytics" exists, **When** I create another schema with the same name, **Then** the system returns an error indicating the schema already exists
3. **Given** multiple schemas exist, **When** I list all schemas, **Then** I see all created schemas with their metadata

---

### User Story 2 - User-Specific Table Creation with Auto-Increment Fields (Priority: P1)

A user within a schema wants to create their own custom tables with specific column definitions and auto-increment capabilities. Each user's tables are isolated within their own namespace under the schema.

**Why this priority**: This delivers the core value proposition - allowing users to define their own data structures. Essential for the first usable version.

**Independent Test**: Can be fully tested by creating a table under a schema+user namespace, inserting data with auto-increment fields, and verifying the table structure and data isolation.

**Acceptance Scenarios**:

1. **Given** I am user "user123" in schema "analytics", **When** I create a table "events" with columns (id, event_type, timestamp), **Then** the table is created at path "analytics.user123.events"
2. **Given** I create a table with an auto-increment field, **When** I specify the field type as "snowflake", **Then** the field automatically generates unique snowflake IDs on insert
3. **Given** I create a table with an auto-increment field, **When** I don't specify a type, **Then** the system defaults to snowflake ID generation
4. **Given** user "user123" has a table "events", **When** user "user456" in the same schema creates a table "events", **Then** both tables exist independently at "analytics.user123.events" and "analytics.user456.events"
5. **Given** I create a table with supported DataFusion datatypes, **When** I insert data, **Then** the data is stored correctly in Parquet format

---

### User Story 3 - Storage Location Configuration (Priority: P2)

A database administrator wants to configure where table data is physically stored. They need the flexibility to use different storage backends (filesystem, S3) for different use cases.

**Why this priority**: Provides flexibility for deployment scenarios, but tables can function with a default storage location initially.

**Independent Test**: Can be fully tested by configuring a storage location, creating tables on that storage, and verifying data persists to the correct location.

**Acceptance Scenarios**:

1. **Given** I have admin access, **When** I configure a storage location with type "filesystem" and path "/data/kalamdb", **Then** the storage location is registered and available for use
2. **Given** I have admin access, **When** I configure a storage location with type "s3" and bucket details, **Then** the storage location is registered with S3 credentials
3. **Given** multiple storage locations exist, **When** I create a table and specify a storage location, **Then** the table's data is stored at that location
4. **Given** I create a table without specifying storage, **Then** the system uses the default configured storage location
5. **Given** a storage location is in use by tables, **When** I attempt to delete it, **Then** the system prevents deletion and shows which tables depend on it

---

### User Story 4 - Multi-Storage Management (Priority: P3)

A power user wants to create and manage multiple storage locations for different data tiers (hot, warm, cold) or geographic regions. They need to register multiple storage backends and assign them to different tables based on access patterns or compliance requirements.

**Why this priority**: Advanced capability for optimization and compliance, but not essential for MVP functionality.

**Independent Test**: Can be fully tested by registering multiple storage locations, distributing tables across them, and verifying data access works correctly from all locations.

**Acceptance Scenarios**:

1. **Given** I have multiple storage locations configured, **When** I list all storage locations, **Then** I see all registered storages with their types and connection details
2. **Given** I have tables on different storage locations, **When** I query data from any table, **Then** the system transparently retrieves data from the correct storage
3. **Given** I want to migrate data, **When** I move a table from one storage to another, **Then** the data is copied and the table metadata is updated

---

### Edge Cases

- What happens when a user tries to create a table with an invalid schema name that doesn't exist?
- How does the system handle auto-increment ID collisions if two instances generate IDs simultaneously?
- What happens when storage location becomes unavailable (S3 credentials expire, filesystem unmounted)?
- How does the system handle schema name conflicts with reserved keywords?
- What happens when a user tries to create a table with DataFusion datatypes that cannot be stored in Parquet?
- How does the system handle extremely long schema/user/table names that might exceed path limits?
- What happens when S3 storage quota is exceeded during a write operation?
- How does the system handle concurrent table creation by the same user?

## Requirements *(mandatory)*

### Functional Requirements

#### Schema Management
- **FR-001**: System MUST allow creation of schemas with unique names that serve as top-level namespaces
- **FR-002**: System MUST enforce schema name uniqueness across the entire database
- **FR-003**: System MUST support listing all schemas with their metadata (creation date, owner, table count)
- **FR-004**: System MUST prevent deletion of schemas that contain user tables
- **FR-005**: System MUST organize data using hierarchical namespace: `<schema_name>.<user_id>.<table_name>`

#### Table Management
- **FR-006**: System MUST allow users to create tables within their user namespace under a schema
- **FR-007**: System MUST isolate each user's tables within their own namespace (schema.user_id.*)
- **FR-008**: System MUST support all DataFusion-compatible datatypes for table columns
- **FR-009**: System MUST store table data in Parquet format
- **FR-010**: System MUST support auto-increment fields with configurable types (snowflake by default)
- **FR-011**: System MUST default to snowflake ID generation when auto-increment type is not specified
- **FR-012**: System MUST support user-specified auto-increment field per table
- **FR-013**: System MUST allow users to specify storage location per table at creation time
- **FR-014**: System MUST use default storage location when not specified during table creation

#### Storage Management
- **FR-015**: System MUST allow one-time configuration of storage locations as a service
- **FR-016**: System MUST support filesystem storage backend with configurable paths
- **FR-017**: System MUST support S3 storage backend with bucket and credential configuration
- **FR-018**: System MUST allow registration of multiple storage locations
- **FR-019**: System MUST track which tables use which storage locations
- **FR-020**: System MUST prevent deletion of storage locations that are in use by tables
- **FR-021**: System MUST validate storage accessibility before allowing table creation on that storage

#### Data Operations
- **FR-022**: System MUST automatically generate auto-increment IDs on data insertion
- **FR-023**: System MUST ensure auto-increment IDs are unique within their table scope
- **FR-024**: System MUST support DataFusion query capabilities across all user-defined tables
- **FR-025**: System MUST maintain data isolation between different users' tables even within the same schema

### Key Entities

- **Schema**: Represents a top-level namespace that contains multiple user namespaces. Attributes: name (unique), creation timestamp, owner, description
- **User Namespace**: Represents a user's isolated space within a schema. Identified by: schema_name + user_id. Contains the user's tables
- **Table**: Represents a user-defined table structure. Attributes: fully qualified name (schema.user.table), columns with datatypes, auto-increment field specification, storage location reference, creation timestamp
- **Storage Location**: Represents a configured storage backend. Attributes: location_id, type (filesystem/s3), connection details (path/bucket/credentials), capacity info, status (active/inactive)
- **Auto-Increment Configuration**: Defines ID generation strategy for a table. Attributes: field_name, generator_type (snowflake/uuid/sequential), sequence_state

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can create a schema and immediately create tables within it in under 30 seconds
- **SC-002**: System supports at least 1000 concurrent users each with their own isolated table namespaces
- **SC-003**: Auto-increment ID generation completes in under 1 millisecond per ID
- **SC-004**: Table creation with schema.user.table namespace succeeds 99.9% of the time without naming conflicts
- **SC-005**: Storage location configuration takes less than 2 minutes to complete and verify
- **SC-006**: System correctly routes data operations to the appropriate storage location 100% of the time
- **SC-007**: Users can create tables with any DataFusion-supported datatype and store data successfully
- **SC-008**: Data queries return results in under 100ms for tables with up to 1 million rows on local storage
- **SC-009**: 95% of users successfully create their first custom table without errors on first attempt
- **SC-010**: System handles storage backend failures gracefully with clear error messages and no data corruption

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
- Users understand the three-level namespace hierarchy (schema.user.table)
- Storage locations are configured by administrators or during initial setup
- Default storage location is configured during system initialization
- Network latency to S3 storage is acceptable for the use cases (no strict SLA defined)
- Table schemas are defined at creation time and do not require dynamic schema evolution initially

## Dependencies

- DataFusion library for query execution and datatype support
- Parquet library for data serialization/deserialization
- S3 SDK (e.g., aws-sdk-s3 for Rust) for S3 storage backend
- Snowflake ID generator library or implementation
- Existing RocksDB or similar metadata store for schema/table catalog
- File system access for local storage backend

## Out of Scope

- Table schema migration/evolution after creation
- Built-in message or conversation tables (these become user-defined tables)
- Cross-schema queries or data access
- Cross-user table access within same schema (strict isolation)
- Storage location migration tools (manual migration only)
- Real-time replication between storage locations
- Advanced storage features like versioning, snapshots, or point-in-time recovery
- Authentication and authorization mechanisms (assumed to be handled externally)
- Storage cost optimization or tiering automation
- Table partitioning strategies
