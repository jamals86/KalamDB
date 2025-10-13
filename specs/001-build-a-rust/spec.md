# Feature Specification: Chat and AI Message History Storage System

**Feature Branch**: `001-build-a-rust`  
**Created**: October 13, 2025  
**Status**: Draft  
**Input**: User description: "Build a database server optimized for chat and AI message history that buffers writes and periodically consolidates data into optimized storage files in a Parquet columnar format for better storage size. It should support querying data via SQL query language, provide APIs for data access, and stream real-time updates to clients. Each user's messages are isolated by userId in hierarchical partitions, and the system runs as a standalone server process."

## Overview

### Core Value Proposition

This system provides a unified solution that combines persistent message storage with real-time streaming capabilities in a single tool. Instead of requiring separate infrastructure components—a database for storage, Redis or Apache Kafka for real-time messaging, and integration code to keep them synchronized—users can deploy one standalone server that handles both concerns seamlessly. This simplifies architecture, reduces operational complexity, eliminates data synchronization challenges, and lowers infrastructure costs for chat and AI message history use cases.

**Key Benefits**:
- **Unified Architecture**: Single system replaces database + message broker combination
- **Simplified Operations**: One server to deploy, configure, monitor, and maintain
- **No Synchronization**: Eliminates complexity of keeping storage and real-time streams in sync
- **Cost Efficiency**: Reduced infrastructure footprint and operational overhead
- **Purpose-Built**: Optimized specifically for chat/AI message patterns (high write throughput, time-series queries, conversation isolation)

## Clarifications

### Session 2025-10-13

- Q: How should consolidated batch files be organized to optimize both user-level queries (latest messages across all conversations) and conversation-specific queries? → A: User-level directory with conversation sorting metadata: `<userId>/batch-<timestamp>-<index>.parquet` with conversationId indexed in Parquet row groups - balanced performance for both query patterns
- Q: What categories of optional metadata should the system support for messages beyond the core fields (userId, conversationId, timestamp, content)? → A: Flexible key-value metadata: Accept arbitrary JSON/map of metadata fields with basic type support (string, number, boolean, array)
- Q: What is the maximum acceptable message size, and how should oversized messages be handled? → A: Configurable limit via configuration file with 1MB default; messages exceeding limit are rejected with clear error
- Q: Should the authentication system support permission scopes or is simple user identity verification sufficient? → A: User identity only: Token identifies the user; user has full access to their own data (all conversations, read/write)
- Q: How should the system handle missed messages when a subscription client reconnects? → A: Each message has unique snowflake ID; clients specify lastMsgId when subscribing to receive all newer messages; clients can also query historical messages by conversationId via API
- Q: What is the exact schema for message storage in Parquet files? → A: Columns: msgId (snowflake ID for ordering), conversationId, from (userId of sender, can be AI or user's own ID), timestamp (Instant), metadata (JSON, future: support custom columns)
- Q: Should the system maintain conversation metadata for each user? → A: Yes, track per-user conversations with: conversationId, firstMsgId, lastMsgId (updated periodically), created timestamp, updated timestamp
- Q: Should the system include administrative tooling for operations and monitoring? → A: Yes, provide web-based Admin UI with: SQL query interface, subscription viewer, configuration management, dashboard (storage/throughput metrics), performance monitoring, and schema management

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Store Chat Messages with Immediate Confirmation (Priority: P1)

Applications send chat and AI message history to the system for persistent storage. Messages are accepted quickly and confirmed, ensuring minimal latency for user-facing chat applications. Each message belongs to a specific user and conversation context.

**Why this priority**: Core value proposition - without reliable message storage, the system has no purpose. This is the foundation that all other features build upon.

**Independent Test**: Can be fully tested by sending messages via the API and verifying immediate acknowledgment responses. Delivers value by ensuring messages are not lost and applications receive confirmation.

**Acceptance Scenarios**:

1. **Given** a chat application with an active conversation, **When** the application sends a message with userId, conversationId, timestamp, and content, **Then** the system accepts the message and returns a success confirmation within 100ms
2. **Given** multiple messages arriving in rapid succession, **When** the application sends 100 messages per second, **Then** all messages are accepted without rejection or data loss
3. **Given** a message with required fields (userId, conversationId, message content, timestamp), **When** submitted to the storage endpoint, **Then** the message is persisted and retrievable

---

### User Story 2 - Query Historical Messages by User and Conversation (Priority: P2)

Applications query past messages for a specific user or conversation using flexible search criteria. Queries return results efficiently even when spanning large time ranges or multiple conversations.

**Why this priority**: Enables applications to display conversation history, search past interactions, and provide context to AI models. Critical for user experience but depends on P1 being functional.

**Independent Test**: Can be tested independently by pre-loading messages, then executing queries with various filters (date range, user, conversation). Delivers value by enabling history retrieval without real-time updates.

**Acceptance Scenarios**:

1. **Given** a conversation with 10,000 stored messages, **When** a user requests the last 50 messages from that conversation, **Then** the system returns the correct messages sorted by timestamp within 200ms
2. **Given** multiple conversations for a single user, **When** querying all messages for that user across conversations within a date range, **Then** the system returns all matching messages organized by conversation and time
3. **Given** a query using filtering criteria (e.g., messages containing specific text, date ranges, participant filters), **When** the query is executed, **Then** results match the filter criteria and are returned in a paginated format

---

### User Story 3 - Receive Real-Time Message Notifications (Priority: P3)

Applications subscribe to live message streams for specific users or conversations. When new messages arrive, subscribed clients receive immediate notifications without polling.

**Why this priority**: Enhances user experience with real-time updates but is not essential for basic storage and retrieval functionality. Applications can poll as a fallback.

**Independent Test**: Can be tested by establishing a subscription connection, then sending messages and verifying notifications are received. Delivers value by enabling real-time chat experiences.

**Acceptance Scenarios**:

1. **Given** a client subscribed his own stream, **When** a new message is stored for this userId, **Then** the client receives a notification containing the message details within 500ms
1. **Given** the system should support millions or concurrent users connected and subscribing to their own messages, each user will be able to subscribe to his own messages, and can open multiple connections from multiple devices
3. **Given** a client connection is interrupted, **When** the client reconnects and re-subscribes with the lastMsgId of the last received message, **Then** all messages with IDs greater than lastMsgId are delivered

---

### User Story 4 - Isolate Data by User and Conversation (Priority: P1)

Each user's messages are stored in isolation from other users, ensuring data privacy and efficient querying. Conversations within a user's data are also organized separately to enable targeted access.

**Why this priority**: Critical for data privacy, security, and query performance. Without proper isolation, the system cannot scale or meet privacy requirements.

**Independent Test**: Can be tested by storing messages for multiple users and conversations, then verifying queries only return data for the specified scope. Delivers value by ensuring privacy and performance.

**Acceptance Scenarios**:

1. **Given** messages stored for multiple users, **When** querying data for User A, **Then** the system returns only User A's messages and never exposes User B's data
2. **Given** multiple conversations for a single user, **When** querying a specific conversationId, **Then** only messages from that conversation are returned
3. **Given** concurrent queries from different users, **When** executed simultaneously, **Then** each query completes without cross-contamination of results

---

### User Story 5 - Efficient Storage with Periodic Consolidation (Priority: P2)

The system manages storage efficiently by consolidating frequently written small message batches into larger optimized files for long-term storage. This process happens automatically without impacting availability.

**Why this priority**: Essential for cost-effective long-term operation and query performance, but the system can function initially without optimization. Becomes critical as data volume grows.

**Independent Test**: Can be tested by observing storage patterns over time and verifying query performance remains consistent as data accumulates. Delivers value through reduced storage costs and improved query speed.

**Acceptance Scenarios**:

1. **Given** the system has been receiving messages continuously, **When** a consolidation cycle triggers, **Then** recent messages are merged into optimized storage files without service interruption
2. **Given** optimized storage files exist alongside recent message buffers, **When** executing queries, **Then** results include data from both sources seamlessly
3. **Given** storage options configured for local or cloud object storage, **When** consolidation occurs, **Then** files are written to the configured destination successfully

---

### User Story 6 - Flexible Querying with SQL (Priority: P3)

Advanced users can query message data using SQL syntax for complex analytical queries, aggregations, and custom reporting needs that go beyond simple retrieval.

**Why this priority**: Powerful feature for analytics and reporting but not essential for core chat functionality. Many use cases are satisfied by simpler query endpoints.

**Independent Test**: Can be tested by executing SQL queries (SELECT, WHERE, GROUP BY) against the message data and verifying correct results. Delivers value for analytical use cases.

**Acceptance Scenarios**:

1. **Given** stored message history, **When** executing a SQL query with aggregation (e.g., "count messages per conversation per day"), **Then** the system returns accurate aggregated results
2. **Given** a complex SQL query with filters, and sorting, **When** executed, **Then** results match the query specification and are returned in a reasonable timeframe
3. **Given** a user with SQL query access, **When** submitting an invalid query, **Then** the system returns a clear error message describing the issue

---

### User Story 7 - Track Conversation Metadata (Priority: P2)

The system automatically maintains metadata for each conversation within a user's data, tracking the first and last message IDs, creation time, and last update time. This enables efficient conversation listing and navigation without scanning all messages.

**Why this priority**: Essential for user experience (conversation lists, last message preview) and query optimization, but core message storage (P1) must work first.

**Independent Test**: Can be tested by creating conversations with messages, then querying conversation metadata and verifying accuracy of firstMsgId, lastMsgId, and timestamps. Delivers value by enabling efficient conversation management UIs.

**Acceptance Scenarios**:

1. **Given** a new conversation is started, **When** the first message is stored, **Then** the system creates conversation metadata with conversationId, firstMsgId, and created timestamp
2. **Given** an existing conversation with metadata, **When** new messages arrive, **Then** the system updates the lastMsgId and updated timestamp periodically
3. **Given** a user with multiple conversations, **When** requesting conversation list, **Then** the system returns all conversations with their metadata (conversationId, firstMsgId, lastMsgId, created, updated) without scanning message files

---

### User Story 8 - Administrative Web UI for System Management (Priority: P2)

System administrators access a web-based administrative interface to monitor system health, manage configurations, browse data, and observe active connections. The UI provides visibility into system operations without requiring command-line access or external monitoring tools.

**Why this priority**: Essential for production operations and troubleshooting, but core storage/query functionality (P1) must work first. Enables operators to manage the system effectively and diagnose issues quickly.

**Independent Test**: Can be tested by accessing the admin UI, executing each management function (run queries, view subscriptions, modify config, check metrics), and verifying results match system state. Delivers value by providing operational visibility and control.

**Acceptance Scenarios**:

1. **Given** an administrator accesses the admin UI, **When** navigating to the SQL query interface, **Then** the administrator can execute SQL queries against message/conversation data and view results in a browsable table format
2. **Given** the admin UI subscriptions view, **When** requesting active subscriptions, **Then** the UI displays all currently subscribed users/connections with their connection details, subscribed conversationIds, and lastMsgId values
3. **Given** an administrator in the configuration section, **When** viewing or modifying system settings (message size limits, consolidation thresholds, storage backends), **Then** changes are validated and applied according to system restart requirements
4. **Given** the dashboard view, **When** loading system metrics, **Then** the UI displays storage statistics (total messages, storage size by user), current throughput (messages/second), active subscriptions count, and consolidation status
5. **Given** the performance monitoring view, **When** observing server metrics, **Then** the UI shows real-time server performance data including CPU usage, memory consumption, disk I/O, query latency percentiles, and error rates
6. **Given** an administrator wants to create or alter table schemas, **When** using the schema management interface, **Then** the UI allows defining message and conversation table structures with validation of schema changes

---

### Edge Cases

- What happens when a message arrives without a required field (userId, conversationId, or timestamp)?
- How does the system handle messages exceeding the configured size limit (default 1MB)?
- What happens when storage becomes full (local disk or cloud storage quota exceeded)?
- How does the system behave when a query spans a time range with millions of messages?
- What happens to subscribed clients when the server restarts or undergoes maintenance?
- How does the system handle concurrent writes to the same conversation from multiple sources?
- What happens when network connectivity to cloud object storage is temporarily lost during consolidation?
- How does the system manage queries that attempt to access unauthorized user data?
- What happens when a client subscribes to a non-existent conversation or user?
- How does the system handle time zone differences in message timestamps?
- How does the admin UI handle SQL queries that take a very long time to execute or consume excessive resources?
- What happens when an unauthorized user attempts to access the admin UI?
- How does the admin UI behave when configuration changes require a server restart but active clients are connected?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST accept message submissions containing userId, conversationId, message content, timestamp, and optional metadata as key-value pairs with support for string, number, boolean, and array types
- **FR-002**: System MUST assign a unique snowflake ID to each message upon acceptance and return it in the acknowledgment response
- **FR-003**: System MUST return acknowledgment within 100ms of receiving a message submission
- **FR-004**: System MUST persist all accepted messages without data loss, even during unexpected shutdowns
- **FR-005**: System MUST support querying messages by userId, conversationId, date range, messageId range (since lastMsgId), and custom filters
- **FR-006**: System MUST return query results in paginated format with configurable page sizes
- **FR-007**: System MUST support real-time message notifications to subscribed clients
- **FR-008**: System MUST allow subscription clients to specify optional lastMsgId parameter to receive all messages with IDs greater than the specified ID
- **FR-009**: System MUST maintain separate storage isolation for each userId to prevent data leakage
- **FR-010**: System MUST organize messages within each user by conversationId for efficient access
- **FR-011**: System MUST consolidate buffered message writes into optimized storage files using a hybrid trigger: every N minutes OR M messages, whichever comes first (default: 5 minutes or 10,000 messages)
- **FR-012**: System MUST support both local file system and cloud object storage as storage backends
- **FR-013**: System MUST expose API endpoints for message submission and querying
- **FR-014**: System MUST support subscription management for real-time notifications
- **FR-015**: System MUST validate message submissions and reject invalid data with clear error messages
- **FR-016**: System MUST enforce configurable maximum message body size limit (default 1MB) and reject messages exceeding this limit with descriptive error indicating the size limit
- **FR-017**: System MUST support SQL query execution against message data
- **FR-018**: System MUST run as a standalone server process that can be started, stopped, and monitored
- **FR-019**: System MUST read configuration from a configuration file supporting settings such as message size limits, consolidation thresholds, storage backend options, and operational parameters
- **FR-020**: System MUST log all critical operations (message storage, queries, errors) for observability
- **FR-021**: System MUST handle concurrent requests from multiple clients safely
- **FR-022**: System MUST enforce data access controls using OAuth/JWT token-based authentication to verify user identity; authenticated users have full read/write access to all their own conversations
- **FR-023**: System MUST support graceful shutdown, completing in-flight operations before stopping
- **FR-024**: System MUST provide health check endpoints for monitoring system status
- **FR-025**: System MUST recover automatically from crashes, resuming operations with buffered data intact
- **FR-026**: System MUST organize stored data in directory hierarchies partitioned by userId with consolidated batch files named `batch-<timestamp>-<index>.parquet` and conversationId indexed within Parquet row groups for efficient filtering
- **FR-027**: System MUST support efficient queries for latest messages at user level across all conversations and for specific conversationId without full data scans
- **FR-028**: System MUST maintain conversation metadata for each user including conversationId, firstMsgId, lastMsgId, created timestamp, and updated timestamp
- **FR-029**: System MUST update conversation lastMsgId periodically (not necessarily in real-time) as new messages arrive
- **FR-030**: System MUST store messages in Parquet format with columns: msgId (snowflake ID), conversationId, from (sender userId), timestamp (Instant), content, and metadata (JSON)
- **FR-031**: System MUST support future extensibility to allow users to define custom columns in addition to the base message schema
- **FR-032**: System MUST provide a web-based administrative UI accessible to authorized administrators
- **FR-033**: Admin UI MUST include an interactive SQL query interface for executing queries against message and conversation data with results displayed in browsable table format
- **FR-034**: Admin UI MUST display all active subscriptions showing user/connection details, subscribed conversationIds, and lastMsgId values
- **FR-035**: Admin UI MUST provide a configuration management interface for viewing and modifying system settings with validation and restart requirement indicators
- **FR-036**: Admin UI MUST include a dashboard displaying storage statistics (total messages, storage size per user), throughput metrics (messages/second), active subscription count, and consolidation status
- **FR-037**: Admin UI MUST provide performance monitoring views showing server metrics including CPU, memory, disk I/O, query latency percentiles, and error rates
- **FR-038**: Admin UI MUST support schema management allowing administrators to view and alter message/conversation table structures with validation
- **FR-039**: Admin UI MUST enforce administrator authentication and authorization separate from regular user access controls

### Key Entities

- **Message**: Represents a single chat or AI interaction entry stored in Parquet files with the following schema:
  - `msgId`: Snowflake ID (unique, time-ordered identifier used for ordering and range queries)
  - `conversationId`: Identifier of the conversation this message belongs to
  - `from`: UserId of the message sender (can be the AI, the stored user's own ID, or another user)
  - `timestamp`: Instant timestamp (equivalent to Java Instant, UTC-based)
  - `content`: Message body/text (subject to configurable size limit)
  - `metadata`: JSON object for flexible key-value pairs (future: support user-defined custom columns)
  
- **User**: Identified by userId, represents an individual whose messages are isolated from others. Multiple conversations belong to each user.

- **Conversation**: Represents a thread or session of related messages within a user's data. Each user's conversations are tracked with metadata including:
  - `conversationId`: Unique identifier for the conversation
  - `firstMsgId`: Snowflake ID of the first message in the conversation
  - `lastMsgId`: Snowflake ID of the most recent message (updated periodically, not always in real-time)
  - `created`: Timestamp when conversation was created
  - `updated`: Timestamp when conversation was last modified
  
- **Subscription**: Represents an active client connection listening for real-time updates on specific users or conversations. Includes connection state, optional lastMsgId filter, and delivery status.

- **Query Request**: Represents a search or retrieval operation with filtering criteria (userId, conversationId, date range, msgId range, text search), pagination parameters, and result format preferences.

- **Storage Partition**: Logical organization of data by userId in directory hierarchies (e.g., `<userId>/batch-<timestamp>-<index>.parquet`), with conversationId indexed within files using row group metadata for efficient query filtering across both user-level and conversation-level access patterns.

- **Administrator**: Authorized personnel with access to the administrative UI for system management, monitoring, and configuration. Has elevated privileges to view all data, modify configurations, execute arbitrary SQL queries, and observe system internals.

- **System Metrics**: Real-time and historical operational data including storage statistics, throughput rates, active connections, performance indicators, and health status exposed through the admin dashboard.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Message submissions are acknowledged within 100ms for 99% of requests under normal load (1000 messages/second)
- **SC-002**: System handles 10,000 concurrent subscriptions without degradation in notification delivery times
- **SC-003**: Queries returning 50 messages complete within 200ms for conversations with up to 100,000 total messages
- **SC-004**: Zero message loss during normal operations and unexpected shutdowns (verified through message count reconciliation)
- **SC-005**: Real-time notifications are delivered to subscribed clients within 500ms of message storage
- **SC-006**: System processes 1000 message writes per second continuously without backlog accumulation
- **SC-007**: SQL queries with aggregations over 1 million messages complete within 5 seconds
- **SC-008**: Storage consolidation completes without service interruption or increased latency for client requests
- **SC-009**: System successfully recovers from crashes within 30 seconds, resuming full operations automatically
- **SC-010**: Data isolation is maintained with 100% accuracy - no cross-user data exposure in queries or subscriptions
- **SC-011**: System scales to store 100 million messages with consistent query performance
- **SC-012**: Storage costs are optimized through consolidation, achieving at least 3x compression compared to raw message storage
- **SC-013**: Single server deployment provides both persistent storage and real-time streaming without requiring separate database or message broker infrastructure
- **SC-014**: Administrative UI provides complete operational visibility enabling administrators to monitor, query, and configure the system without requiring command-line access or external tools

## Assumptions

- Message timestamps are provided by clients in UTC or a standardized format; the system does not generate timestamps unless explicitly required
- Clients are responsible for authentication tokens or API keys when submitting requests; the system validates these tokens but does not implement user registration or login flows
- Server operators configure operational parameters (message size limits, consolidation thresholds, storage backends) via configuration file before starting the server; runtime reconfiguration requires restart
- Network connectivity to cloud storage is generally reliable with occasional temporary interruptions that can be retried
- Message content is primarily text-based or serialized data; large binary attachments (images, files) are stored as references/URLs rather than embedded in message content
- Parquet files use a fixed base schema (msgId, conversationId, from, timestamp, content, metadata) with metadata stored as JSON; future extensibility for custom user-defined columns is planned but not required initially
- Conversation metadata (lastMsgId, updated timestamp) is updated periodically during consolidation cycles rather than immediately with each message for performance optimization
- Consolidation cycles trigger based on configurable thresholds (time interval, message count, or buffer size) with a default of every 5 minutes or 10,000 messages, whichever comes first
- Queries are executed with reasonable limits; extremely broad queries (e.g., "all messages for all conversations across all time") may require pagination or timeouts
- Real-time subscriptions use persistent connection protocols and clients can reconnect automatically on disconnection
- The system runs on infrastructure with sufficient disk space, memory, and CPU to handle expected load; capacity planning is performed by operators
- Data retention policies are configured externally; the system stores messages indefinitely unless instructed to delete via API or configuration
- Message ordering within a conversation is based on timestamps provided by clients; concurrent messages with identical timestamps are ordered consistently but arbitrarily
