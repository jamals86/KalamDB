# Data Model: Docker Container, WASM Compilation, and TypeScript Examples

**Feature**: 006-docker-wasm-examples  
**Date**: 2025-10-25  
**Phase**: 1 - Design

## Overview

This document defines the data structures and schema changes for API key authentication, soft delete, and the TODO example application.

---

## Story 0: API Key Authentication and Soft Delete

### Entity: User (Modified)

**Purpose**: Represents a KalamDB user with authentication and namespace access

**Schema Changes**:

```rust
pub struct User {
    pub id: UserId,              // Existing: Unique user identifier
    pub name: String,            // Existing: User display name
    pub namespace_id: NamespaceId, // Existing: User's namespace
    pub apikey: String,          // NEW: API key for authentication (auto-generated UUID)
    pub role: String,            // NEW: User role (e.g., "admin", "user", "readonly")
    pub created_at: Timestamp,   // Existing: Creation timestamp
    pub updated_at: Timestamp,   // Existing: Last update timestamp
}
```

**Field Specifications**:

| Field | Type | Constraints | Purpose |
|-------|------|-------------|---------|
| id | UserId | PK, auto-increment | Unique user identifier |
| name | String | NOT NULL, max 255 chars | User display name |
| namespace_id | NamespaceId | FK to namespaces | User's namespace |
| apikey | String | UNIQUE, NOT NULL, auto-generated UUID | API key for X-API-KEY auth |
| role | String | NOT NULL, max 50 chars | User role for authorization |
| created_at | Timestamp | NOT NULL, auto-generated | Row creation time |
| updated_at | Timestamp | NOT NULL, auto-updated | Last modification time |

**Indexes**:
- PRIMARY KEY: `id`
- UNIQUE INDEX: `apikey` (for O(1) lookup by API key)
- INDEX: `namespace_id` (existing)

**Validation Rules**:
- `apikey` must be unique across all users (auto-generated on user creation)
- `apikey` format: UUID v4 (e.g., "550e8400-e29b-41d4-a716-446655440000")
- `role` must be one of predefined values (initially: "admin", "user", "readonly")

**RocksDB Storage**:
- Key: `user:{id}`
- Secondary index: `apikey_to_user:{apikey}` → `{id}`

---

### Soft Delete Pattern (Applied to User Tables)

**Purpose**: Mark rows as deleted instead of physical removal, enabling recovery and audit trails

**Schema Addition**:

```rust
// Add to all user table rows
pub struct UserTableRow {
    pub id: i64,                 // Existing: Row ID (auto-increment)
    pub data: HashMap<String, Value>, // Existing: Column data
    pub deleted: bool,           // NEW: Soft delete flag (default: false)
    pub created_at: Timestamp,   // Existing: Row creation time
    pub updated_at: Timestamp,   // Existing: Last update time
}
```

**Field Specifications**:

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| deleted | bool | false | Marks row as deleted without physical removal |

**Query Behavior**:
- SELECT: Automatically filter `WHERE deleted = false` unless `INCLUDE DELETED` specified
- UPDATE: Normal behavior (can update deleted rows if explicitly targeted)
- DELETE: Execute `UPDATE SET deleted = true, updated_at = now()` instead of physical removal

**Indexes**:
- Compound index: `(table_id, deleted, id)` for efficient deleted row filtering

**RocksDB Storage**:
- Key: `user_table:{table_id}:row:{id}`
- Value includes `deleted` boolean field
- Iteration filter applied during scan

---

## Story 3: TODO Example Application

### Entity: TODO Item

**Purpose**: Represents a single TODO item in the example application

**Schema**:

```sql
CREATE TABLE IF NOT EXISTS todos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**Field Specifications**:

| Field | Type | Constraints | Purpose |
|-------|------|-------------|---------|
| id | INTEGER | PK, auto-increment | Unique TODO identifier |
| title | TEXT | NOT NULL, max 500 chars | TODO description |
| completed | BOOLEAN | NOT NULL, default false | Completion status |
| created_at | TIMESTAMP | NOT NULL, auto-generated | Creation timestamp |

**Validation Rules**:
- `title` cannot be empty string
- `title` maximum 500 characters
- `completed` defaults to false on creation

**Indexes**:
- PRIMARY KEY: `id`
- No additional indexes needed (small dataset, simple queries)

**TypeScript Interface** (Client-side):

```typescript
interface Todo {
  id: number;
  title: string;
  completed: boolean;
  created_at: string; // ISO 8601 timestamp
}
```

---

## State Transitions

### User Authentication State

```
[Unauthenticated] 
    ↓ (provide valid X-API-KEY header)
[Authenticated with User context]
    ↓ (invalid/missing API key)
[401 Unauthorized Error]
```

### Row Lifecycle with Soft Delete

```
[Created (deleted=false)]
    ↓ (SELECT queries return row)
[Visible to queries]
    ↓ (DELETE operation)
[Soft Deleted (deleted=true)]
    ↓ (SELECT queries skip row by default)
[Hidden from queries]
    ↓ (future: UNDELETE operation)
[Created (deleted=false)] (restore)
```

### TODO Item Lifecycle

```
[Created via WASM client]
    ↓ (persisted to KalamDB + localStorage)
[Stored in DB and cache]
    ↓ (subscription broadcast)
[Synced to all connected tabs]
    ↓ (user marks completed)
[Updated (completed=true)]
    ↓ (user deletes)
[Soft Deleted (deleted=true)]
    ↓ (subscription broadcast delete event)
[Removed from UI and cache]
```

### WebSocket Connection State

```
[Disconnected]
    ↓ (initialize with url + apikey)
[Connecting]
    ↓ (handshake success)
[Connected]
    ↓ (subscribe to table)
[Subscribed to TODO table]
    ↓ (receive events: insert/update/delete)
[Processing events, updating UI]
    ↓ (connection lost)
[Disconnected] → Disable write operations
    ↓ (reconnect)
[Connecting] → Subscribe from last known ID
```

---

## Data Flow Diagrams

### API Key Authentication Flow

```
Client Request
    ↓
[HTTP Request with X-API-KEY header]
    ↓
[Actix-Web Middleware]
    ↓
[Extract X-API-KEY value]
    ↓
[Query user table by apikey]
    ↓
[API key found?] ──No──> [Return 401 Unauthorized]
    ↓ Yes
[Inject User into request extensions]
    ↓
[Route handler accesses User context]
    ↓
[Execute query in user's namespace]
```

### Soft Delete Query Flow

```
DELETE FROM todos WHERE id = 5
    ↓
[Parse DELETE statement]
    ↓
[Rewrite as UPDATE]
    ↓
UPDATE todos SET deleted = true, updated_at = now() WHERE id = 5
    ↓
[Execute UPDATE in RocksDB]
    ↓
[Return success]

SELECT * FROM todos
    ↓
[Parse SELECT statement]
    ↓
[Add filter: WHERE deleted = false]
    ↓
[Execute query with filter]
    ↓
[Return non-deleted rows only]
```

### React App Data Sync Flow

```
[App Loads]
    ↓
[Read localStorage: todos + lastId]
    ↓
[Display cached TODOs immediately]
    ↓
[Initialize WASM client with url + apikey]
    ↓
[Connect WebSocket]
    ↓
[Subscribe to 'todos' table from lastId]
    ↓
[Receive missed events (insert/update/delete)]
    ↓
[Update UI + localStorage]
    ↓
[User adds TODO]
    ↓
[Send INSERT via WASM client]
    ↓
[Server broadcasts to all subscribers]
    ↓
[All tabs receive insert event]
    ↓
[Update UI + localStorage with new TODO]
```

---

## Relationships

### User ↔ API Key
- **Type**: One-to-One (optional)
- **Direction**: User owns API key
- **Constraints**: API key is unique, optional (NULL allowed)

### User ↔ Namespace
- **Type**: Many-to-One (existing)
- **Direction**: User belongs to Namespace
- **Constraints**: Every user has exactly one namespace

### User ↔ User Tables
- **Type**: One-to-Many (existing)
- **Direction**: User owns tables
- **Constraints**: Tables scoped to user's namespace

### TODO Table ↔ TODO Rows
- **Type**: One-to-Many
- **Direction**: Table contains rows
- **Constraints**: Soft delete applies to all rows

---

## Data Constraints and Validation

### API Key Generation
- **Format**: UUID v4 or 32-character hexadecimal string
- **Uniqueness**: Must be unique across all users
- **Generation**: `uuid::Uuid::new_v4()` or `rand::thread_rng().gen::<[u8; 16]>()`
- **Storage**: Plain text (not hashed - API keys, not passwords)
- **Rotation**: Manual (future enhancement)

### Soft Delete Filtering
- **Default Behavior**: All SELECT queries auto-filter `deleted = false`
- **Override**: `SELECT * FROM table INCLUDE DELETED` (future enhancement)
- **Performance**: Indexed on `deleted` field to avoid full scans

### TODO Validation
- **Title**: 1-500 characters, non-empty
- **Completed**: Boolean (true/false)
- **ID**: Auto-increment (server-assigned)

### localStorage Constraints
- **Storage Limit**: ~5-10MB (browser-dependent)
- **Serialization**: JSON.stringify/parse
- **Fallback**: If localStorage full/disabled, app works without cache (slower initial load)

---

## Migration Strategy

### Adding API Key to User Table

```sql
-- Add apikey column (nullable for backward compatibility)
ALTER TABLE users ADD COLUMN apikey TEXT;

-- Create unique index
CREATE UNIQUE INDEX idx_users_apikey ON users(apikey) WHERE apikey IS NOT NULL;
```

**Backward Compatibility**: Existing users have NULL apikey (can still use other auth methods)

### Adding Soft Delete to User Tables

```sql
-- Add deleted column with default false
ALTER TABLE user_table_rows ADD COLUMN deleted BOOLEAN NOT NULL DEFAULT FALSE;

-- Create compound index for efficient filtering
CREATE INDEX idx_user_table_rows_deleted ON user_table_rows(table_id, deleted, id);
```

**Backward Compatibility**: Existing rows default to deleted=false (visible in queries)

---

## Summary

**Schema Changes**:
- User table: +1 field (`apikey`)
- User table rows: +1 field (`deleted`)
- New TODO table: 4 fields (id, title, completed, created_at)

**Indexes Added**:
- UNIQUE index on `users.apikey`
- INDEX on `user_table_rows.deleted`

**Data Flows**:
- API key authentication via middleware
- Soft delete via query rewriting
- WebSocket subscription with localStorage caching

All data model changes are backward compatible and support the feature requirements.
