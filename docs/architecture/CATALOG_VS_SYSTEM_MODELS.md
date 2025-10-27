# Catalog Models vs System Table Models - Architecture Guide

**Date**: October 27, 2025  
**Purpose**: Clarify the distinction between catalog models and system table models

## Overview

KalamDB has TWO separate sets of models that may appear similar but serve fundamentally different purposes:

1. **Catalog Models** (`kalamdb-core/src/catalog/`) - Runtime entities
2. **System Table Models** (`kalamdb-commons/src/models/system.rs`) - Persisted rows

## Key Differences

| Aspect | Catalog Models | System Table Models |
|--------|---------------|---------------------|
| **Location** | `kalamdb-core/src/catalog/` | `kalamdb-commons/src/models/system.rs` |
| **Purpose** | In-memory runtime entities | Database persistence (RocksDB) |
| **Serialization** | Not directly serialized | Bincode for RocksDB storage |
| **Domain Logic** | Rich validation, business methods | Simple DTOs (data transfer) |
| **Lifetime** | Process lifetime (cached) | Persistent across restarts |
| **Timestamp Format** | `DateTime<Utc>` (chrono) | `i64` milliseconds |
| **Options/Metadata** | `HashMap<String, Value>` | `Option<String>` (JSON blob) |

## Namespace Models Comparison

### catalog::Namespace (Runtime Entity)

**File**: `backend/crates/kalamdb-core/src/catalog/namespace.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub name: NamespaceId,                                 // Just the ID
    pub created_at: DateTime<Utc>,                         // Rich timestamp
    pub options: HashMap<String, serde_json::Value>,       // Parsed JSON
    pub table_count: u32,                                  // Unsigned int
}
```

**Domain Methods**:
- `new(name)` - Constructor
- `validate_name(name)` - Validation logic
- `can_delete()` - Business logic
- `increment_table_count()` - State management
- `decrement_table_count()` - State management

**Use Cases**:
- In-memory catalog management
- Namespace validation during CREATE NAMESPACE
- Table count tracking for deletion safety
- Runtime namespace lookups

---

### system::Namespace (System Table Row)

**File**: `backend/crates/kalamdb-commons/src/models/system.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Namespace {
    pub namespace_id: NamespaceId,      // Primary key
    pub name: String,                    // Namespace name (redundant with ID)
    pub created_at: i64,                 // Unix milliseconds
    pub options: Option<String>,         // JSON string blob
    pub table_count: i32,                // Signed int (for SQL compatibility)
}
```

**No Domain Methods** - Just a data container

**Use Cases**:
- Persisted in `system.namespaces` table
- Queried via SQL: `SELECT * FROM system.namespaces`
- Bincode serialization for RocksDB storage
- API responses (JSON)

---

## Table Models Comparison

### catalog::TableMetadata (Runtime Entity)

**File**: `backend/crates/kalamdb-core/src/catalog/table_metadata.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetadata {
    pub table_name: TableName,
    pub table_type: TableType,
    pub namespace: NamespaceId,
    pub created_at: DateTime<Utc>,           // Rich timestamp
    pub storage_location: String,
    pub flush_policy: FlushPolicy,           // Complex struct
    pub schema_version: u32,
    pub deleted_retention_hours: Option<u32>,
}
```

**Domain Methods**:
- `new(...)` - Constructor with defaults
- `validate_table_name(name)` - Validation
- `column_family_name()` - CF naming logic
- `schema_directory()` - Path generation
- `with_flush_policy()` - Builder pattern
- `increment_schema_version()` - Version management

**Use Cases**:
- Table creation validation
- Column family name generation: `user_table:app:messages`
- Schema path computation: `conf/app/schemas/messages/`
- Flush policy management

---

### system::SystemTable (System Table Row)

**File**: `backend/crates/kalamdb-commons/src/models/system.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SystemTable {
    pub table_id: String,                    // Primary key
    pub table_name: TableName,
    pub namespace: NamespaceId,
    pub table_type: TableType,
    pub created_at: i64,                     // Unix milliseconds
    pub storage_location: String,
    pub storage_id: Option<StorageId>,       // FK to system.storages
    pub use_user_storage: bool,
    pub flush_policy: String,                // JSON string blob
    pub schema_version: i32,                 // Signed int
    pub deleted_retention_hours: i32,        // Signed int
}
```

**No Domain Methods** - Just a data container

**Use Cases**:
- Persisted in `system.tables` table
- Queried via SQL: `SELECT * FROM system.tables WHERE namespace = 'app'`
- Bincode serialization for RocksDB storage
- API responses (JSON)

---

## Conversion Pattern

The two model types are intentionally separate and convert between each other:

```rust
// Catalog → System Table (for persistence)
fn to_system_table(metadata: &TableMetadata) -> SystemTable {
    SystemTable {
        table_id: generate_id(),
        table_name: metadata.table_name.clone(),
        namespace: metadata.namespace.clone(),
        table_type: metadata.table_type,
        created_at: metadata.created_at.timestamp_millis(),
        storage_location: metadata.storage_location.clone(),
        storage_id: None,
        use_user_storage: false,
        flush_policy: serde_json::to_string(&metadata.flush_policy).unwrap(),
        schema_version: metadata.schema_version as i32,
        deleted_retention_hours: metadata.deleted_retention_hours.unwrap_or(24) as i32,
    }
}

// System Table → Catalog (for loading)
fn to_catalog_metadata(table: &SystemTable) -> TableMetadata {
    TableMetadata {
        table_name: table.table_name.clone(),
        table_type: table.table_type,
        namespace: table.namespace.clone(),
        created_at: DateTime::from_timestamp_millis(table.created_at).unwrap(),
        storage_location: table.storage_location.clone(),
        flush_policy: serde_json::from_str(&table.flush_policy).unwrap(),
        schema_version: table.schema_version as u32,
        deleted_retention_hours: if table.deleted_retention_hours > 0 {
            Some(table.deleted_retention_hours as u32)
        } else {
            None
        },
    }
}
```

## Why Both Exist

### Architectural Benefits

1. **Separation of Concerns**
   - Catalog: Business logic, validation, runtime operations
   - System: Data persistence, SQL queries, API responses

2. **Type Safety**
   - Catalog: Rich types (`DateTime<Utc>`, `FlushPolicy` struct)
   - System: Simple types (i64, String JSON blobs) for serialization

3. **Performance**
   - Catalog: In-memory caching, fast lookups
   - System: Persistent storage, query optimization

4. **Flexibility**
   - Catalog: Can change internal representation without DB migration
   - System: Stable schema for backward compatibility

### Common Misconceptions

❌ **WRONG**: "We should merge catalog and system models to avoid duplication"
- They serve different purposes - this is intentional separation

❌ **WRONG**: "catalog::Namespace is a duplicate of system::Namespace"
- Different fields, different purposes, different lifetimes

✅ **CORRECT**: "Catalog models are runtime entities, system models are persistence DTOs"
- This is the intended architecture

✅ **CORRECT**: "We convert between them when loading/saving"
- This conversion is the bridge between memory and storage

## Models That ARE Duplicates (Need Consolidation)

The following models WERE duplicates and have been consolidated into `kalamdb-commons/src/models/system.rs`:

- ✅ `User` - Was in kalamdb-sql, kalamdb-core (NOW: only in commons)
- ✅ `Job` - Was in kalamdb-sql, kalamdb-core, jobs_provider (NOW: only in commons)
- ✅ `LiveQuery` - Was in kalamdb-sql, live_queries_provider (NOW: only in commons)
- ✅ `Storage` - Was in kalamdb-sql (NOW: only in commons)
- ✅ `TableSchema` - Was in kalamdb-sql (NOW: only in commons)

These are PURE system table models with no catalog equivalent.

## Models That Are NOT Duplicates (Keep Separate)

- ✅ `catalog::Namespace` vs `system::Namespace` - Different purposes
- ✅ `catalog::TableMetadata` vs `system::SystemTable` - Different purposes
- ✅ `catalog::TableCache` - Catalog-only (no system table)

## Summary

**Catalog models** (`kalamdb-core/src/catalog/`):
- Rich domain entities
- Validation and business logic
- In-memory runtime use
- Keep as-is

**System table models** (`kalamdb-commons/src/models/system.rs`):
- Simple persistence DTOs
- Single source of truth
- Bincode + JSON serialization
- All consolidated in commons

**No consolidation needed** between catalog and system models - they are architecturally distinct by design.

---

**Conclusion**: The existence of both catalog and system models is intentional and correct architecture. Only TRUE duplicates (same model defined in multiple places for the SAME purpose) should be consolidated.
