# Topic/PubSub Implementation Tasks

## Overview
Implement durable topic-based pub/sub system backed by RocksDB for change-event consumption with multiple consumer groups, at-least-once delivery semantics, and replay capabilities.

## Status Summary (As of Latest Update)

### ✅ Completed Phases
- **Phase 1**: Foundation (IDs, Types, Commons) - COMPLETE
- **Phase 2**: System Tables (Topics, TopicOffsets providers) - COMPLETE
- **Phase 3**: Storage (TopicMessageStore, TopicOffsetStore) - COMPLETE
- **Phase 4**: TopicPublisherService Integration - COMPLETE
- **Phase 5**: SQL Parsing (CREATE TOPIC, DROP TOPIC, ALTER TOPIC ADD SOURCE, CONSUME, ACK) - COMPLETE
- **Phase 6**: SQL Handlers (CREATE, DROP, ALTER, CONSUME, ACK) - COMPLETE
- **Phase 7**: HTTP API (/v1/api/topics/consume, /v1/api/topics/ack) - COMPLETE
- **Phase 8**: Background Jobs - Topic Retention Executor - COMPLETE
- **Phase 9**: CDC Integration - Automatic topic routing from DML writes - COMPLETE

### 🚧 Remaining Work (Optional Enhancements)
- **Phase 8.2**: Implement actual message cleanup logic in TopicRetentionExecutor (deferred until production use)
- **Phase 10**: Testing - Integration tests for CDC workflow
- **Phase 11**: Documentation - Update architecture docs with CDC flow

---

## Phase 1: Foundation - IDs, Types, and Commons (kalamdb-commons) ✅ COMPLETE

### Task 1.1: Add Topic ID Types ✅
**File**: `backend/crates/kalamdb-commons/src/models/ids/`

- [x] Create `topic_id.rs` following the pattern in `user_id.rs`/`job_id.rs`
  - `pub struct TopicId(String)` with `StorageKey` trait implementation
  - Add `new()`, `as_str()`, `into_string()` methods
  - Add `Serialize`, `Deserialize`, `Encode`, `Decode`, `Clone`, `Debug`, `PartialEq`, `Eq`, `Hash`
  - Implement `Display` and `From<String>`
  
- [x] Create `consumer_group_id.rs` following same pattern
  - `pub struct ConsumerGroupId(String)`
  - All same traits as TopicId

- [x] Update `backend/crates/kalamdb-commons/src/models/ids/mod.rs`
  - Add `pub mod topic_id;`
  - Add `pub mod consumer_group_id;`
  - Re-export `pub use topic_id::TopicId;`
  - Re-export `pub use consumer_group_id::ConsumerGroupId;`

### Task 1.2: Add Topic Enums ✅
**File**: `backend/crates/kalamdb-commons/src/models/`

- [x] Create `topic_op.rs` (similar to `role.rs` structure)
  ```rust
  pub enum TopicOp {
      Insert,
      Update,
      Delete,
  }
  ```
  - Add `Serialize`, `Deserialize`, `Encode`, `Decode`, `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`
  - Add `Display` implementation
  - Add `from_str()` method

- [x] Create `payload_mode.rs`
  ```rust
  pub enum PayloadMode {
      Key,    // Primary key values only
      Full,   // Full row snapshot
      Diff,   // Changed columns (future)
  }
  ```
  - Same traits as TopicOp

- [x] Update `backend/crates/kalamdb-commons/src/models/mod.rs`
  - Add `mod topic_op;`
  - Add `mod payload_mode;`
  - Re-export both types

---

## Phase 2: System Tables and Models (kalamdb-system) ✅ COMPLETE

### Task 2.1: Create Topic Model ✅
**File**: `backend/crates/kalamdb-system/src/providers/topics/models/topic.rs`

- [x] Create directory structure: `backend/crates/kalamdb-system/src/providers/topics/`
- [x] Create subdirectories: `models/`, `indexes/` (if needed)

- [x] Define `TopicRoute` struct first:
  ```rust
  #[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
  pub struct TopicRoute {
      pub table_id: TableId,
      pub op: TopicOp,
      pub payload_mode: PayloadMode,
      pub filter_expr: Option<String>,
      pub partition_key_expr: Option<String>,
  }
  ```

- [x] Define `Topic` struct using `#[table(...)]` macro (follow `Job` and `User` patterns)
  ```rust
  #[table(
      name = "topics",
      comment = "Durable topics for pub/sub messaging"
  )]
  #[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
  pub struct Topic {
      #[column(id = 1, ordinal = 1, data_type(KalamDataType::String), primary_key = true)]
      pub topic_id: TopicId,
      
      #[column(id = 2, ordinal = 2, data_type(KalamDataType::String), nullable = false)]
      pub name: String,
      
      #[column(id = 3, ordinal = 3, data_type(KalamDataType::String), nullable = true)]
      pub alias: Option<String>,
      
      #[column(id = 4, ordinal = 4, data_type(KalamDataType::Int), nullable = false, default = "1")]
      pub partitions: u32,
      
      #[column(id = 5, ordinal = 5, data_type(KalamDataType::BigInt), nullable = true)]
      pub retention_seconds: Option<i64>,
      
      #[column(id = 6, ordinal = 6, data_type(KalamDataType::BigInt), nullable = true)]
      pub retention_max_bytes: Option<i64>,
      
      #[column(id = 7, ordinal = 7, data_type(KalamDataType::String), nullable = false)]
      pub routes: Vec<TopicRoute>, // Stored as JSON/bincode
      
      #[column(id = 8, ordinal = 8, data_type(KalamDataType::Timestamp), nullable = false)]
      pub created_at: i64,
      
      #[column(id = 9, ordinal = 9, data_type(KalamDataType::Timestamp), nullable = false)]
      pub updated_at: i64,
  }
  ```

- [x] Create `models/mod.rs` re-exporting `Topic` and `TopicRoute`

### Task 2.2: Create Topic Offset Model ✅
**File**: `backend/crates/kalamdb-system/src/providers/topic_offsets/models/topic_offset.rs`

- [x] Create directory: `backend/crates/kalamdb-system/src/providers/topic_offsets/`

- [x] Define `TopicOffset` struct (implemented in kalamdb-tables)

### Task 2.3: Create System Table Providers ✅
**Files**: `backend/crates/kalamdb-system/src/providers/topics/` and `topic_offsets/`

- [x] Create `topics_table.rs` (follow `jobs_table.rs` pattern)
  - Define `TopicsTableSchema` struct
  - Implement schema generation using Arrow DataTypes

- [x] Create `topics_provider.rs` (follow `jobs_provider.rs` pattern)
  - `pub struct TopicsTableProvider`
  - Implement `SystemTableProviderExt` trait
  - Implement DataFusion `TableProvider` trait

- [x] Create `topic_offsets_table.rs` and `topic_offsets_provider.rs`

### Task 2.4: Register System Tables ✅
**File**: `backend/crates/kalamdb-system/src/registry.rs`

- [x] Add `TopicsTableProvider` initialization in `SystemTableRegistry::new()`
- [x] Add `TopicOffsetsTableProvider` initialization
- [x] Register tables in catalog under `system.topics` and `system.topic_offsets`

### Task 2.5: Update System Models Re-exports ✅
- [x] Topics and TopicRoute models are properly exported

---

## Phase 3: Storage Layer - RocksDB (kalamdb-tables) ✅ COMPLETE

### Task 3.1: Topic Message Store ✅
**File**: `backend/crates/kalamdb-tables/src/topics/topic_message_store.rs`

- [x] Create `TopicMessageStore` struct
- [x] Implement `publish()` method
- [x] Implement `fetch_messages()` method

### Task 3.2: Topic Offset Store ✅
**File**: `backend/crates/kalamdb-tables/src/topics/topic_offset_store.rs`

- [x] Create `TopicOffsetStore` struct
- [x] Implement `ack_offset()` method
- [x] Implement `get_group_offsets()` method

### Task 3.3: Topic Message Model ✅
**File**: `backend/crates/kalamdb-tables/src/topics/topic_message_models.rs`

- [x] Define `TopicMessage` struct with all required fields

---

## Phase 4: Publisher Integration - Unified Notification Point (kalamdb-core) ✅ MOSTLY COMPLETE

### Task 4.1-4.4: TopicPublisherService ✅
**File**: `backend/crates/kalamdb-core/src/live/topic_publisher.rs`

- [x] Create unified `TopicPublisherService` that consolidates:
  - Topic registry (in-memory cache + RocksDB persistence)
  - Message publishing and consumption
  - Consumer group offset tracking
  - TableId → Topics routing for CDC
- [x] Owns `TopicMessageStore` and `TopicOffsetStore` internally
- [x] DashMap-based caching for lock-free concurrent access
- [x] Methods: `has_topics_for_table()`, `route_and_publish()`, `fetch_messages()`, `ack_offset()`
- [x] Full test suite (6/6 tests passing)

### Task 4.5: Integrate TopicPublisherService with AppContext ✅
**File**: `backend/crates/kalamdb-core/src/app_context.rs`

- [x] Add `topic_publisher: Arc<TopicPublisherService>` field to `AppContext`
- [x] Initialize `TopicPublisherService` during `AppContext::new()`
- [x] Add getter method: `pub fn topic_publisher(&self) -> Arc<TopicPublisherService>`

### Task 4.6: Wire NotificationService to TopicPublisherService ✅ COMPLETE
**File**: `backend/crates/kalamdb-core/src/live/notification.rs`

- [x] Add `row_to_record_batch()` helper function to convert Row to single-row RecordBatch
- [x] Integrate topic publisher in notification worker (line 180-211)
- [x] Map ChangeType (Insert/Update/Delete) to TopicOp
- [x] Check for topic routes via `has_topics_for_table()` before conversion
- [x] Convert Row to RecordBatch and call `route_and_publish()`
- [x] Handle errors gracefully (log warnings, don't block live query notifications)

**Implementation Details**:
- **CDC Flow**: All table changes (INSERT/UPDATE/DELETE) → NotificationService → TopicPublisherService → Topics
- **Conversion**: Single Row → Single-row RecordBatch (supports Int64, UInt64, Float64, String, Boolean types)
- **Error Handling**: Conversion failures logged as warnings, don't break notification pipeline
- **Performance**: Topic routing only happens if topics are registered for that table (fast path check)

**How It Works**:
1. Table write operations call `notification_service.notify_async(user_id, table_id, change_notification)`
2. NotificationService worker receives notification task
3. **Step 1 (NEW)**: If TopicPublisher is configured and has routes for table:
   - Map ChangeType → TopicOp (Insert→Insert, Update→Update, Delete→Delete)
   - Convert Row to RecordBatch using `row_to_record_batch()`
   - Call `topic_publisher.route_and_publish(table_id, operation, batch)`
   - Topic messages written to RocksDB and available for CONSUME
4. **Step 2 (Existing)**: Route to live query WebSocket subscribers
5. Both flows happen in single worker task (no blocking, fire-and-forget)

---

## Phase 5: SQL Parsing for Topic Commands (kalamdb-sql) ✅ COMPLETE

### Task 5.1: Add Topic SQL AST Types ✅
**File**: `backend/crates/kalamdb-sql/src/ddl/topic_commands.rs`

- [x] `CreateTopicStatement` - parsed and working
- [x] `DropTopicStatement` - parsed and working
- [x] `AddTopicSourceStatement` (ALTER TOPIC ADD SOURCE) - parsed and working
- [x] `ConsumeStatement` - parsed and working
- [x] `AckStatement` - IMPLEMENTED

### Task 5.2: Implement SQL Parsers ✅
- [x] Implement `parse_create_topic()` 
- [x] Implement `parse_drop_topic()`
- [x] Implement `parse_add_topic_source()` (ALTER TOPIC ADD SOURCE)
- [x] Implement `parse_consume()`
- [x] Implement `parse_ack()` - COMPLETED

### Task 5.3: Update SQL Classifier ✅
**File**: `backend/crates/kalamdb-sql/src/classifier/types.rs`

- [x] Add `CreateTopic`, `DropTopic`, `AlterTopicAddSource`, `Consume` to classifier
- [x] Add `Ack` to classifier - COMPLETED

---

## Phase 6: Core Handlers - Topic Management (kalamdb-core) ✅ COMPLETE

### Task 6.1: Implement CREATE TOPIC Handler ✅
**File**: `backend/crates/kalamdb-core/src/sql/executor/handlers/topics/create.rs`

- [x] Handler creates topics via `system_tables().topics()`
- [x] Duplicate detection
- [x] Configurable partitions (default: 1)
- [x] Updates TopicPublisherService cache

### Task 6.2: Implement ALTER TOPIC ADD SOURCE Handler ✅
**File**: `backend/crates/kalamdb-core/src/sql/executor/handlers/topics/add_source.rs`

- [x] Adds routes to existing topics
- [x] Duplicate route detection (same table + operation)
- [x] Updates TopicPublisherService cache

### Task 6.3: Implement DROP TOPIC Handler ✅
**File**: `backend/crates/kalamdb-core/src/sql/executor/handlers/topics/drop.rs`

- [x] Deletes topics via `system_tables().topics()`
- [x] Updates TopicPublisherService cache
- [ ] Background job for message cleanup (TODO - Phase 8)

### Task 6.4: Implement CONSUME Handler ✅
**File**: `backend/crates/kalamdb-core/src/sql/executor/handlers/topics/consume.rs`

- [x] Uses `topic_publisher().fetch_messages()`
- [x] Consumer group support with offset tracking
- [x] Position-based consumption (Latest, Earliest, Offset)
- [x] Auto-commits offset after consumption

### Task 6.5: Implement ACK Handler ✅ COMPLETE
**File**: `backend/crates/kalamdb-core/src/sql/executor/handlers/topics/ack.rs`

- [x] Parse ACK statement
- [x] Update offset via `topic_publisher().ack_offset()`
- [x] Return success message with acknowledged offset

---

## Phase 7: API Layer (kalamdb-api) ✅ COMPLETE

### Task 7.1: Add Request/Response Types ✅
**File**: `backend/crates/kalamdb-api/src/handlers/topics.rs`

- [x] Create `ConsumeRequest` and `ConsumeResponse` structs
- [x] Create `AckRequest` and `AckResponse` structs
- [x] Create `StartPosition` enum for consumption position
- [x] Create `TopicMessage` response struct

### Task 7.2: Add Authorization Checks ✅
- [x] Only allow `service`, `dba`, `system` roles to consume topics (enforced in handlers)

### Task 7.3: Implement Long Polling Consume Endpoint ✅
**Endpoint**: `POST /v1/api/topics/consume`

- [x] Create async handler with long polling support
- [x] Add timeout configuration (optional field in request)
- [x] Return messages with base64-encoded payloads
- [x] Handle consumer group offset resolution

### Task 7.4: Implement ACK Endpoint ✅
**Endpoint**: `POST /v1/api/topics/ack`

- [x] Create async handler
- [x] Update offset via TopicPublisherService
- [x] Return success response with acknowledged offset

### Task 7.5: Register Routes ✅
- [x] Add `/v1/api/topics` scope with consume and ack routes
- [x] Wire handlers in routes.rs

---

## Phase 8: Background Jobs - Retention Cleanup (kalamdb-core) ✅ COMPLETE

### Task 8.1: Create Topic Retention Executor ✅
**File**: `backend/crates/kalamdb-core/src/jobs/executors/topic_retention.rs`

- [x] Define `TopicRetentionExecutor` struct following job executor pattern
- [x] Implement `TopicRetentionParams` with validation
- [x] Add `TopicRetention` variant to `JobType` enum with "TR" prefix
- [x] Register executor in `JobRegistry` during AppContext initialization
- [x] Implement `execute()` method with retention policy enforcement logic
- [x] Add cancellation support
- [x] Add unit tests for parameter validation and serialization
- [x] Export from executors module
- [x] Update test helpers to register TopicRetentionExecutor

**Implementation Complete**:
- TopicRetentionExecutor registered in AppContext (9 total executors)
- JobType enum updated with TopicRetention variant
- Full test coverage for parameter validation
- Ready for actual message cleanup implementation when needed

### Task 8.2: Implement Actual Message Cleanup Logic 🚧 TODO
**File**: `backend/crates/kalamdb-core/src/jobs/executors/topic_retention.rs`

- [ ] Access TopicMessageStore from TopicPublisherService
- [ ] Scan messages by prefix: "topic/{topic_id}/{partition_id}/"
- [ ] Parse TopicMessage and filter by cutoff timestamp
- [ ] Delete expired messages in batches via RocksDB
- [ ] Track metrics (messages_deleted, bytes_freed)
- [ ] Add integration tests with real messages

**Note**: The executor framework is complete. Message cleanup implementation can be added when topic message storage is actively used in production.

### Task 3.2: Create Topic Message Envelope
**File**: `backend/crates/kalamdb-publisher/src/models/topic_message.rs` (to be created)

- [ ] Define `TopicMessage` struct:
  ```rust
  #[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
  pub struct TopicMessage {
      pub topic_id: TopicId,
      pub partition_id: u32,
      pub offset: u64,
      pub message_id: Option<String>, // Snowflake
      pub source_table: TableId,
      pub op: TopicOp,
      pub ts: i64, // UTC millis
      pub payload_mode: PayloadMode,
      pub payload: Vec<u8>, // Bincode-encoded payload
  }
  ```

- [ ] Implement key serialization for `topic/<topic_id>/<partition_id>/<offset>`

---

## Phase 4: Publisher Integration - Unified Notification Point (kalamdb-core)

### Task 4.1: Extend NotificationService with Topic Support
**File**: `backend/crates/kalamdb-core/src/live/notification.rs`

**CRITICAL: This is the ONLY connectivity point for pushing change notifications to consumers from shared, user, and stream tables.**

- [ ] Extend `NotificationService` to handle topic-based notifications in addition to live queries:
  - Add internal `TopicRouter` reference to `NotificationService` struct
  - Extend `notify_async()` to dispatch to both live query subscriptions AND topic routes
  - Reuse existing `has_subscribers()` optimization to avoid unnecessary work

- [ ] Implement topic notification dispatch:
  ```rust
  pub fn notify_async(&self, user_id: UserId, table_id: TableId, notification: ChangeNotification) {
      // Step 1: Early exit if no subscribers (existing optimization)
      if !self.has_subscribers(&user_id, &table_id) {
          return; // No one listening - avoid any work
      }
      
      // Step 2: Dispatch to live query subscribers (existing code)
      // ...send to mpsc channel...
      
      // Step 3: Dispatch to topic routes (NEW)
      // - Extract operation type (INSERT/UPDATE/DELETE) from notification
      // - Call topic_router.on_change() with change event
      // - Let topic_router handle filtering, payload extraction, offset allocation
  }
  ```

- [ ] Integration points:
  - Reuse existing `filter_matches()` from live query logic for topic filtering
  - Reuse existing `apply_projections()` from live query logic for topic payloads
  - Reuse existing column/type metadata infrastructure
  - **Ensure all changes to shared, user, and stream tables flow through this single NotificationService**

### Task 4.2: Create Topic Router Service
**File**: `backend/crates/kalamdb-core/src/live/topic_router.rs` (new)

- [ ] Define `TopicRouter` struct:
  ```rust
  pub struct TopicRouter {
      backend: Arc<dyn StorageBackend>,
      topics_store: Arc<IndexedEntityStore<TopicId, Topic>>,
      route_index: Arc<DashMap<(TableId, TopicOp), Vec<TopicId>>>, // In-memory cache
  }
  ```

- [ ] Implement `new()` and `build_route_index()` methods:
  - Load all topics from `topics_store`
  - For each topic, iterate routes and populate `route_index`
  - Build reverse index: `(table_id, op) -> [topic_id]` for fast lookup

- [ ] Implement `on_change()` method:
  ```rust
  pub async fn on_change(
      &self,
      table_id: &TableId,
      op: TopicOp,
      row_data: &RecordBatch,
  ) -> Result<(), KalamDbError>
  ```
  - Lookup routes from `route_index` by `(table_id, op)`
  - For each matching route:
    - Evaluate `filter_expr` if present (reuse live query filter logic)
    - Extract payload based on `payload_mode` (Key/Full/Diff)
    - Allocate offset via internal offset allocator
    - Build `TopicMessage`
    - Write to `topic_logs` partition in RocksDB

### Task 4.3: Implement Offset Allocation
**File**: `backend/crates/kalamdb-core/src/live/topic_router.rs` (same file as above)

- [ ] Implement atomic offset allocation within `TopicRouter`:
  - Key format: `counter/<topic_id>/<partition_id>`
  - Read current counter value, increment, write back atomically
  - Use RocksDB WriteBatch for atomicity with topic message write
  - Support multiple partitions via partition ID

### Task 4.4: Implement Payload Extraction
**File**: `backend/crates/kalamdb-core/src/live/topic_router.rs` (same file as above)

- [ ] Implement payload extraction logic:
  ```rust
  fn extract_topic_payload(
      &self,
      row: &RecordBatch,
      mode: PayloadMode,
      primary_keys: &[String],
  ) -> Result<Vec<u8>, KalamDbError>
  ```
  - `Key`: Extract primary key columns, bincode serialize
  - `Full`: Serialize entire row as bincode/Arrow IPC format
  - `Diff`: Return error for now (future enhancement)
  - Leverage existing column metadata infrastructure from schema registry

### Task 4.5: Integrate TopicRouter with AppContext
**File**: `backend/crates/kalamdb-core/src/app_context.rs`

- [ ] Add `TopicRouter` field to `AppContext` (similar to NotificationService):
  ```rust
  pub topic_router: Arc<TopicRouter>,
  ```

- [ ] Initialize `TopicRouter` during `AppContext::new()`:
  - Create `TopicRouter` with backend and topics_store references
  - Build initial route index

- [ ] Add getter method:
  ```rust
  pub fn topic_router(&self) -> &Arc<TopicRouter>
  ```

---

## Phase 5: SQL Parsing for Topic Commands (kalamdb-sql)

**NOTE: Topic routing is handled transparently by NotificationService (Phase 4). SQL parsing below is for explicit topic management (CREATE TOPIC, ALTER TOPIC, CONSUME, ACK).**

### Task 5.1: Add Topic SQL AST Types
**File**: `backend/crates/kalamdb-sql/src/ddl/topic_commands.rs` (new file)

- [ ] Create structs following `user_commands.rs` pattern:
  ```rust
  pub struct CreateTopicStatement {
      pub name: String,
      pub alias: Option<String>,
      pub partitions: u32,
      pub retention_seconds: Option<i64>,
      pub retention_max_bytes: Option<i64>,
  }
  
  pub struct AlterTopicAddRouteStatement {
      pub topic_name: String,
      pub source_table: String,
      pub op: TopicOp,
      pub filter_expr: Option<String>,
      pub payload_mode: PayloadMode,
      pub partition_key_expr: Option<String>,
  }
  
  pub struct ConsumeStatement {
      pub topic_name: String,
      pub group_id: String,
      pub start: ConsumeStart,
      pub limit: usize,
      pub partition_id: u32,
  }
  
  pub enum ConsumeStart {
      Latest,
      Earliest,
      Offset(u64),
  }
  
  pub struct AckStatement {
      pub topic_name: String,
      pub group_id: String,
      pub partition_id: u32,
      pub upto_offset: u64,
  }
  ```

### Task 5.2: Implement SQL Parsers
**File**: Same as above

- [ ] Implement `parse_create_topic()` using sqlparser tokenizer
- [ ] Implement `parse_alter_topic_add_route()` with WHERE clause support
- [ ] Implement `parse_consume()` with FROM LATEST/EARLIEST/OFFSET
- [ ] Implement `parse_ack()`

### Task 5.3: Update SQL Classifier
**File**: `backend/crates/kalamdb-sql/src/classifier/types.rs`

- [ ] Add to `SqlStatementKind` enum:
  ```rust
  CreateTopic(CreateTopicStatement),
  AlterTopicAddRoute(AlterTopicAddRouteStatement),
  Consume(ConsumeStatement),
  Ack(AckStatement),
  ```

- [ ] Update classification logic in `backend/crates/kalamdb-sql/src/parser/extensions.rs`:
  - Check for `CREATE TOPIC`
  - Check for `ALTER TOPIC ... ADD SOURCE`
  - Check for `CONSUME FROM`
  - Check for `ACK`

---

## Phase 6: Core Handlers - Topic Management (kalamdb-core)

**CRITICAL: ALL table writes (INSERT/UPDATE/DELETE on shared, user, and stream tables) automatically notify subscribers through NotificationService. No additional plumbing needed in individual table write paths.**

### Task 6.1: Implement CREATE TOPIC Handler
**File**: `backend/crates/kalamdb-core/src/sql/executor/topic_handlers.rs` (new)

- [ ] Create handler function following patterns in existing DDL handlers:
  ```rust
  pub async fn handle_create_topic(
      stmt: CreateTopicStatement,
      ctx: Arc<AppContext>,
  ) -> Result<DataFrame, KalamDbError>
  ```
  - Generate `TopicId` (e.g., using Snowflake or UUID)
  - Validate `name` uniqueness and `alias` uniqueness
  - Create `Topic` struct with empty `routes` vec
  - Insert into `topics_store` via `IndexedEntityStore`
  - Refresh `TopicRouter` route index
  - Return success DataFrame

### Task 6.2: Implement ALTER TOPIC ADD SOURCE Handler
**File**: Same as above

- [ ] Create handler function:
  ```rust
  pub async fn handle_alter_topic_add_route(
      stmt: AlterTopicAddRouteStatement,
      ctx: Arc<AppContext>,
  ) -> Result<DataFrame, KalamDbError>
  ```
  - Lookup topic by name (or alias)
  - Parse `source_table` into `TableId` (validate namespace/table exists)
  - Create `TopicRoute` struct
  - Append to `Topic.routes` vec
  - Update topic in store
  - Refresh `TopicRouter` route index
  - Return success DataFrame

### Task 6.3: Implement CONSUME Handler
**File**: `backend/crates/kalamdb-core/src/sql/executor/consume_handler.rs` (new)

- [ ] Create handler function:
  ```rust
  pub async fn handle_consume(
      stmt: ConsumeStatement,
      ctx: Arc<AppContext>,
  ) -> Result<DataFrame, KalamDbError>
  ```
  - Resolve topic by name or alias
  - Get consumer group offset from `topic_offsets` store (or default to start position)
  - Scan `topic_logs` partition from start offset with limit
  - Deserialize `TopicMessage` envelopes
  - Convert to Arrow RecordBatch
  - Return as DataFrame

### Task 6.4: Implement ACK Handler
**File**: Same as above

- [ ] Create handler function:
  ```rust
  pub async fn handle_ack(
      stmt: AckStatement,
      ctx: Arc<AppContext>,
  ) -> Result<DataFrame, KalamDbError>
  ```
  - Resolve topic by name
  - Update or insert `TopicOffset` record with new `last_acked_offset`
  - Use `IndexedEntityStore::upsert()` or `put()`
  - Return success DataFrame

### Task 6.5: Automatic Topic Notification - No Additional Plumbing Needed
**File**: Integration happens automatically through NotificationService

- [x] **ALL changes to shared, user, and stream tables ALREADY flow through NotificationService**:
  - The existing `notify_table_change_async()` calls in write paths are already present
  - `NotificationService.notify_async()` will dispatch to both live queries AND topic routes
  - No additional code needed in individual write handlers - reuse existing notification flow
  - This is the unified connectivity point for all consumers (live queries + topic subscribers)

### Task 6.6: Update SQL Executor Dispatcher
**File**: `backend/crates/kalamdb-core/src/sql/executor/mod.rs` or main executor entry point

- [ ] Add routing for new SQL statement types:
  - `SqlStatementKind::CreateTopic(stmt)` → `handle_create_topic()`
  - `SqlStatementKind::AlterTopicAddRoute(stmt)` → `handle_alter_topic_add_route()`
  - `SqlStatementKind::Consume(stmt)` → `handle_consume()`
  - `SqlStatementKind::Ack(stmt)` → `handle_ack()`

---

## Phase 7: API Layer (kalamdb-api)

### Task 7.1: Add Request/Response Types
**File**: `backend/crates/kalamdb-api/src/routes/topics/types.rs` (new)

- [ ] Create request/response structs:
  ```rust
  #[derive(Deserialize)]
  pub struct ConsumeRequest {
      pub topic: String,
      pub group_id: String,
      pub start: ConsumeStart,
      pub limit: Option<usize>,
      pub partition_id: Option<u32>,
      pub timeout_seconds: Option<u64>,
  }
  
  #[derive(Deserialize)]
  #[serde(tag = "type")]
  pub enum ConsumeStart {
      Latest,
      Earliest,
      Offset { offset: u64 },
  }
  
  #[derive(Serialize)]
  pub struct ConsumeResponse {
      pub messages: Vec<TopicMessageDto>,
      pub next_offset: Option<u64>,
      pub has_more: bool,
  }
  
  #[derive(Serialize)]
  pub struct TopicMessageDto {
      pub topic_id: String,
      pub partition_id: u32,
      pub offset: u64,
      pub message_id: Option<String>,
      pub source_table: String,
      pub op: String,
      pub ts: i64,
      pub payload_mode: String,
      pub payload: String, // base64-encoded
  }
  
  #[derive(Deserialize)]
  pub struct AckRequest {
      pub topic: String,
      pub group_id: String,
      pub partition_id: Option<u32>,
      pub upto_offset: u64,
  }
  
  #[derive(Serialize)]
  pub struct AckResponse {
      pub success: bool,
      pub acknowledged_offset: u64,
  }
  ```

### Task 7.2: Add Authorization Checks
**File**: `backend/crates/kalamdb-api/src/routes/topics/auth.rs` (new)

- [ ] Create authorization guard, check deeply what already been implemented in other endpoints and follow the same pattern, only allow `service`, `dba`, `system` roles to consume topics:

### Task 7.3: Implement Long Polling Consume Endpoint
**File**: `backend/crates/kalamdb-api/src/routes/topics/consume.rs` (new)

- [ ] Create async handler with long polling:
  ```rust
  #[post("/consume")]
  pub async fn consume(
      req: web::Json<ConsumeRequest>,
      session: SessionContext,
      ctx: web::Data<Arc<AppContext>>,
  ) -> Result<HttpResponse, ApiError> {
      // 1. Authentication check (done by SessionContext middleware)
      
      // 2. Authorization check (role validation)
      check_topic_consume_auth(&session)?;
      
      // 3. Get timeout from request or config
      let timeout_secs = req.timeout_seconds
          .unwrap_or(ctx.config.topics.default_consume_timeout)
          .min(ctx.config.topics.max_consume_timeout);
      
      let timeout = Duration::from_secs(timeout_secs);
      
      // 4. Long polling loop with timeout
      let deadline = Instant::now() + timeout;
      
      loop {
          // Try to fetch messages
          let messages = ctx.topic_consumer
              .consume(
                  &req.topic,
                  &req.group_id,
                  req.start.clone(),
                  req.limit.unwrap_or(100),
                  req.partition_id.unwrap_or(0),
              )
              .await?;
          
          // If messages found, return immediately
          if !messages.is_empty() {
              let next_offset = messages.last().map(|m| m.offset + 1);
              return Ok(HttpResponse::Ok().json(ConsumeResponse {
                  messages: messages.into_iter().map(|m| m.into()).collect(),
                  next_offset,
                  has_more: true, // TODO: determine based on limit
              }));
          }
          
          // Check timeout
          if Instant::now() >= deadline {
              // Timeout - return empty array
              return Ok(HttpResponse::Ok().json(ConsumeResponse {
                  messages: vec![],
                  next_offset: None,
                  has_more: false,
              }));
          }
          
          // Wait briefly before next check (avoid tight loop)
          tokio::time::sleep(Duration::from_millis(100)).await;
      }
  }
  ```

- [ ] Add poll interval configuration to avoid tight loop
- [ ] Add metrics for long polling duration and message counts
- [ ] Verify that the long polling is non-blocking and efficient
- [ ] Handle cancellation if client disconnects
- [ ] Add test cases for consume endpoint with and without messages
- [ ] Cover also test cases where authentication/authorization fails if the user does not have the required roles

### Task 7.4: Implement ACK Endpoint
**File**: `backend/crates/kalamdb-api/src/routes/topics/ack.rs` (new)

- [ ] Create async handler:
  ```rust
  #[post("/ack")]
  pub async fn ack(
      req: web::Json<AckRequest>,
      session: SessionContext,
      ctx: web::Data<Arc<AppContext>>,
  ) -> Result<HttpResponse, ApiError> {
      // 1. Authentication check (done by SessionContext middleware)
      
      // 2. Authorization check (role validation)
      check_topic_consume_auth(&session)?;
      
      // 3. Execute ACK
      ctx.topic_consumer
          .ack(
              &req.topic,
              &req.group_id,
              req.partition_id.unwrap_or(0),
              req.upto_offset,
          )
          .await?;
      
      Ok(HttpResponse::Ok().json(AckResponse {
          success: true,
          acknowledged_offset: req.upto_offset,
      }))
  }
  ```

### Task 7.5: Register Routes and Configuration
**File**: `backend/crates/kalamdb-api/src/routes/topics/mod.rs` (new)

- [ ] Create module structure:
  ```rust
  mod auth;
  mod consume;
  mod ack;
  mod types;
  
  pub use consume::consume;
  pub use ack::ack;
  pub use types::*;
  
  pub fn configure(cfg: &mut web::ServiceConfig) {
      cfg.service(
          web::scope("/api/topics")
              .wrap(AuthenticationMiddleware) // Ensure auth middleware is applied
              .service(consume)
              .service(ack)
      );
  }
  ```

**File**: `backend/crates/kalamdb-api/src/lib.rs` or main Actix-Web config

- [ ] Register topics routes: `.configure(topics::configure)`

### Task 7.6: Add Topic Configuration Settings
**File**: `backend/crates/kalamdb-configs/src/config/types.rs`

- [ ] Add `TopicsSettings` struct:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct TopicsSettings {
      /// Default timeout for long polling CONSUME requests (seconds)
      pub default_consume_timeout: u64,
      
      /// Maximum timeout allowed for CONSUME requests (seconds)
      pub max_consume_timeout: u64,
      
      /// Poll interval when waiting for new messages (milliseconds)
      pub poll_interval_ms: u64,
      
      /// Allow user role to consume topics (false = only service/dba/system)
      pub allow_user_role_consume: bool,
  }
  
  impl Default for TopicsSettings {
      fn default() -> Self {
          Self {
              default_consume_timeout: 30,
              max_consume_timeout: 300,
              poll_interval_ms: 100,
              allow_user_role_consume: false,
          }
      }
  }
  ```

- [ ] Add `topics: TopicsSettings` field to `ServerConfig` struct

**File**: `backend/server.example.toml`

- [ ] Add topics configuration section:
  ```toml
  [topics]
  default_consume_timeout = 30
  max_consume_timeout = 300
  poll_interval_ms = 100
  allow_user_role_consume = false
  ```

---

## Phase 8: Background Jobs - Retention Cleanup (kalamdb-core)

### Task 8.1: Create Topic Retention Executor
**File**: `backend/crates/kalamdb-core/src/jobs/executors/topic_retention.rs` (new)

- [ ] Define `TopicRetentionExecutor` struct following `job_cleanup.rs` pattern:
  ```rust
  pub struct TopicRetentionExecutor;
  
  impl JobExecutor for TopicRetentionExecutor {
      type Params = TopicRetentionParams;
      
      async fn execute(&self, ctx: JobContext<Self::Params>) -> Result<(), KalamDbError> {
          // For each topic:
          // - Read retention policy (retention_seconds, retention_max_bytes)
          // - Scan topic_logs by prefix
          // - Delete entries older than threshold
      }
  }
  ```

### Task 8.2: Register Executor and Schedule Jobs
**File**: `backend/crates/kalamdb-core/src/jobs/executors/registry.rs`

- [ ] Add `TopicRetentionExecutor` to `JobRegistry`
- [ ] Schedule periodic retention jobs (e.g., daily or configurable)

---

## Phase 9: AppContext Integration - Unified Consumer Notification

### Task 9.1: NotificationService is Already Central Hub
**File**: `backend/crates/kalamdb-core/src/app_context.rs`

- [x] **NotificationService already exists in AppContext and handles both live queries and topics**:
  - Field: `pub notification_service: Arc<NotificationService>`
  - This is the single unified point for all change notifications
  - Called by all table write paths (INSERT, UPDATE, DELETE)
  - Dispatches to both live query subscribers AND topic routes

### Task 9.2: Add TopicRouter to AppContext
**File**: `backend/crates/kalamdb-core/src/app_context.rs`

- [ ] Add `TopicRouter` field alongside NotificationService:
  ```rust
  pub topic_router: Arc<TopicRouter>,
  ```

- [ ] Initialize in `AppContext::new()`:
  - Create `TopicRouter` with backend and topics_store references
  - Build initial route index
  - Pass reference to `NotificationService` so it can call `topic_router.on_change()`

---

## Phase 10: Testing

### Task 10.1: Unit Tests

- [ ] Test `TopicId` and `ConsumerGroupId` serialization/deserialization
- [ ] Test `TopicRouter::on_change()` with mock change events
- [ ] Test `OffsetAllocator` atomicity
- [ ] Test SQL parsers for all topic commands
- [ ] Test payload extraction (Key, Full modes)

### Task 10.2: Integration Tests
**File**: `backend/tests/topic_pubsub_tests.rs` (new)

- [ ] Test CREATE TOPIC → verify in system.topics
- [ ] Test ALTER TOPIC ADD SOURCE → verify routes updated
- [ ] Test INSERT into source table → verify message in topic_logs
- [ ] Test CONSUME FROM EARLIEST → verify messages returned
- [ ] Test ACK → verify offset updated
- [ ] Test CONSUME with filter_expr
- [ ] Test multiple consumer groups with different offsets
- [ ] Test retention cleanup job
- [ ] **Test long polling timeout behavior (empty response after timeout)**
- [ ] **Test long polling returns immediately when messages available**
- [ ] **Test CONSUME with user role → expect 403 Forbidden**
- [ ] **Test CONSUME with service role → expect success**
- [ ] **Test CONSUME with dba role → expect success**
- [ ] **Test CONSUME with system role → expect success**
- [ ] **Test ACK with user role → expect 403 Forbidden**
- [ ] **Test unauthenticated CONSUME → expect 401 Unauthorized**
- [ ] **Test timeout configuration (custom vs default vs max)**

### Task 10.3: Smoke Tests
**File**: `cli/tests/smoke/` (if smoke tests exist)

- [ ] Add basic topic workflow test: CREATE TOPIC → ALTER → INSERT → CONSUME → ACK

---

## Phase 11: Documentation

### Task 11.1: Update API Docs
- [ ] Document CREATE TOPIC syntax
- [ ] Document ALTER TOPIC ADD SOURCE syntax (with WHERE clause example)
- [ ] Document CONSUME syntax (LATEST/EARLIEST/OFFSET)
- [ ] Document ACK syntax

### Task 11.2: Update Architecture Docs
- [ ] Add pub/sub architecture diagram
- [ ] Document storage layout (topic_logs, topic_counters, system tables)
- [ ] Document delivery semantics (at-least-once)

---

## Phase 12: Future Work Notes

### Task 12.1: Live Query Integration
- [ ] Research shared `ChangeEventEvaluator` for both topics and live queries
- [ ] Design abstraction to avoid duplicating filter/projection logic

### Task 12.2: Multi-Partition Support
- [ ] Implement partition key hashing
- [ ] Shard offset counters by partition
- [ ] Distribute partitions across cluster nodes

### Task 12.3: Query Payload Mode (Optional)
- [ ] Re-evaluate if `payload = 'query'` mode is needed
- [ ] If yes, implement SQL execution on change event context

---

## Dependencies and Ordering

**Critical Path:**
1. Phase 1 (Commons) → Phase 2 (System Tables) → Phase 3 (Storage) → Phase 4 (Notification Integration)
2. Phase 5 (SQL Parsing) → Phase 6 (Handlers) → Phase 7 (API)
3. Phase 8 (Retention) can start after Phase 2
4. Phase 9 (AppContext) finalizes integration
5. Phase 10 (Testing) validates all phases

**Key Architectural Insight:**
- Phase 4 extends existing `NotificationService` in `kalamdb-core/src/live/notification.rs`
- This is the ONLY connectivity point for pushing changes to consumers
- All table writes (shared/user/stream) already call `NotificationService.notify_async()`
- Minimal plumbing needed - reuse existing live query infrastructure

**Parallelizable:**
- Phase 5 (SQL Parsing) can start while Phase 4 is in progress
- Phase 8 (Retention) can be developed independently
- Documentation (Phase 11) can be written alongside implementation

---

## Validation Checklist (from Spec)

**CRITICAL: Unified Consumer Notification Through NotificationService**
- [ ] Changes to SHARED tables flow through `NotificationService` to topic subscribers
- [ ] Changes to USER tables flow through `NotificationService` to topic subscribers
- [ ] Changes to STREAM tables flow through `NotificationService` to topic subscribers
- [ ] `NotificationService.notify_async()` dispatches to BOTH live queries AND topic routes
- [ ] `has_subscribers()` check prevents unnecessary work when no consumers exist
- [ ] No separate/duplicate notification paths - single unified `notification.rs` only

**Topic System Tables and Functionality**
- [ ] System tables exist and are queryable
- [ ] CREATE TOPIC persists metadata
- [ ] ALTER TOPIC ADD SOURCE creates route
- [ ] Change events append to topic logs (via NotificationService)
- [ ] CONSUME supports EARLIEST/LATEST/OFFSET
- [ ] ACK updates offsets correctly
- [ ] Retention job prunes old log entries
- [ ] WHERE filters work correctly on routes
- [ ] Topic name and alias uniqueness enforced
- [ ] Multiple consumer groups can track separate offsets
- [ ] Payload modes (Key/Full) work correctly

**API and Authentication**
- [ ] **Long polling works correctly with configurable timeout**
- [ ] **Authentication required for CONSUME and ACK endpoints**
- [ ] **Authorization checks: only service/dba/system roles can consume**
- [ ] **User role is rejected with forbidden error**
- [ ] **Empty response returned after timeout if no messages**
- [ ] **Clients can reconnect immediately after response**

---

## Estimated Complexity

| Phase | Complexity | Estimated Time | Notes |
|-------|-----------|----------------|-------|
| 1. Commons | Low | 2-4 hours | ID and enum types |
| 2. System Tables | Medium | 6-8 hours | Topic and offset models |
| 3. Storage Layer | Low | 2-3 hours | RocksDB partition setup |
| 4. Publisher Integration | Medium | 6-8 hours | **Extends existing NotificationService** |
| 5. SQL Parsing | Medium | 6-8 hours | CREATE/ALTER/CONSUME/ACK commands |
| 6. Core Handlers | Medium | 8-10 hours | Topic management handlers |
| 7. API Layer | Medium | 6-8 hours | Long polling + auth |
| 8. Retention Job | Medium | 4-6 hours | Background cleanup |
| 9. AppContext | Low | 2-3 hours | Add TopicRouter reference |
| 10. Testing | Medium-High | 10-12 hours | Incl. auth, polling, shared/user/stream tables |
| 11. Documentation | Low | 3-4 hours | API and architecture docs |
| **Total** | - | **55-68 hours** | Reduced due to unified NotificationService |

---

## Next Steps

1. Review this task list with stakeholders
2. Prioritize phases based on business needs
3. Start with Phase 1 (Commons) as foundation
4. Implement in order to maintain dependencies
5. Write tests alongside each phase (TDD approach recommended)
6. Update this document as tasks are completed
