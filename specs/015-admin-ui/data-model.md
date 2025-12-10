# Data Model: Admin UI with Token-Based Authentication

**Feature**: 015-admin-ui  
**Date**: December 3, 2025

## Entities

### 1. Access Token (JWT Claims)

Represents an authenticated session stored in HttpOnly cookie.

| Field | Type | Description | Validation |
|-------|------|-------------|------------|
| sub | string | User ID (UUID) | Required, valid UUID |
| iss | string | Issuer ("kalamdb") | Required |
| exp | number | Expiration (Unix timestamp) | Required, future timestamp |
| iat | number | Issued at (Unix timestamp) | Required |
| username | string | Username | Required |
| role | string | User role | Required, one of: user, service, dba, system |

**State Transitions**: None (stateless JWT)

**Relationships**: None (self-contained)

### 2. User (Existing Entity - Extended)

Database user account. Already exists in `system.users` table.

| Field | Type | Description | Validation |
|-------|------|-------------|------------|
| id | UUID | Primary key | Auto-generated |
| username | string | Unique username | Required, 3-64 chars, alphanumeric + underscore |
| password_hash | string | bcrypt hash | Required, min 8 chars original |
| role | enum | Access level | Required: user, service, dba, system |
| email | string? | Optional email | Valid email format if present |
| created_at | timestamp | Creation time | Auto-set |
| updated_at | timestamp | Last update | Auto-set on change |
| deleted_at | timestamp? | Soft delete marker | Null for active users |

**State Transitions**:
- Active → Deleted (soft delete sets deleted_at)
- No resurrection (create new user instead)

**Relationships**:
- Owns multiple sessions (via JWT tokens)
- Has one role (enum, not FK)

**Admin UI Constraints**:
- Only dba/system roles visible in admin UI
- Cannot delete self
- Cannot demote last system user

### 3. Namespace (Existing Entity)

Logical container for tables. Already exists in `system.namespaces` table.

| Field | Type | Description | Validation |
|-------|------|-------------|------------|
| id | UUID | Primary key | Auto-generated |
| name | string | Unique name | Required, 1-64 chars, alphanumeric + underscore |
| created_at | timestamp | Creation time | Auto-set |
| updated_at | timestamp | Last update | Auto-set on change |

**State Transitions**: None (namespaces are created/deleted, not modified)

**Relationships**:
- Contains multiple tables (via table definitions)

### 4. Storage (Existing Entity)

Configuration for data persistence. Already exists in `system.storages` table.

| Field | Type | Description | Validation |
|-------|------|-------------|------------|
| id | UUID | Primary key | Auto-generated |
| name | string | Storage name | Required, unique |
| storage_type | enum | Type | Required: local, s3 |
| path | string | Base path | Required for local |
| config | JSON | Type-specific config | S3: bucket, region, etc. |
| created_at | timestamp | Creation time | Auto-set |

**State Transitions**: Read-only in admin UI (storage is configured at deployment)

**Relationships**:
- Used by multiple namespaces/tables

### 5. Table Metadata (For Autocomplete)

Schema information for SQL autocomplete. Read from schema registry.

| Field | Type | Description |
|-------|------|-------------|
| namespace | string | Namespace name |
| name | string | Table name |
| table_type | enum | user, shared, stream |
| columns | Column[] | Column definitions |

### 6. Column Metadata (For Autocomplete)

Column information for SQL autocomplete.

| Field | Type | Description |
|-------|------|-------------|
| name | string | Column name |
| data_type | string | Arrow data type name |
| nullable | boolean | Allows nulls |
| is_primary_key | boolean | Part of primary key |

### 7. Query Result

Output from SQL execution (transient, not persisted).

| Field | Type | Description |
|-------|------|-------------|
| columns | ColumnDef[] | Column names and types |
| rows | any[][] | Row data as 2D array |
| row_count | number | Total rows returned |
| truncated | boolean | True if limit (10,000) reached |
| execution_time_ms | number | Query execution time |
| error | string? | Error message if failed |

### 8. File Entry (For Storage Browser)

File/folder information for storage browser (transient).

| Field | Type | Description |
|-------|------|-------------|
| name | string | File or folder name |
| path | string | Full path from storage root |
| is_directory | boolean | True if folder |
| size_bytes | number? | File size (null for directories) |
| modified_at | timestamp? | Last modification time |

### 9. Settings Entry

Configuration settings for display (read-only).

| Field | Type | Description |
|-------|------|-------------|
| category | string | Setting category |
| key | string | Setting key |
| value | string | Current value |
| description | string | Human-readable description |

## Entity Relationship Diagram

```
┌─────────────┐
│   User      │──────────┐
└─────────────┘          │ authenticates
       │                 ▼
       │          ┌─────────────┐
       │          │ AccessToken │ (JWT in cookie)
       │          └─────────────┘
       │ manages
       ▼
┌─────────────┐     contains    ┌─────────────┐
│  Namespace  │────────────────▶│   Table     │
└─────────────┘                 └─────────────┘
                                       │
                                       │ has
                                       ▼
                                ┌─────────────┐
                                │   Column    │
                                └─────────────┘

┌─────────────┐     stores      ┌─────────────┐
│   Storage   │────────────────▶│  FileEntry  │
└─────────────┘                 └─────────────┘
```

## Validation Rules Summary

1. **User Creation**: Username unique, password ≥8 chars, valid role
2. **User Update**: Cannot change own role, cannot delete self
3. **Namespace Creation**: Name unique, alphanumeric + underscore
4. **Query Execution**: Max 10,000 rows, 30s timeout
5. **Admin Access**: Only dba and system roles can access admin UI
