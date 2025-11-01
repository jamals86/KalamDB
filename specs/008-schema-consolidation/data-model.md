# Data Model: Schema Consolidation & Unified Data Type System

**Feature**: 008-schema-consolidation  
**Date**: 2025-11-01  
**Phase**: Phase 1 - Design & Contracts

This document defines the entity models, relationships, validation rules, and state transitions for the schema consolidation feature.

---

## Entity Relationship Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      TableDefinition                        │
├─────────────────────────────────────────────────────────────┤
│ table_id: TableId (PK - unique, contains namespace.name)   │
│ table_name: String                                          │
│ namespace_id: NamespaceId (FK → Namespace)                 │
│ table_type: TableType (SYSTEM/USER/SHARED/STREAM)         │
│ schema_version: u32                                         │
│ created_at: DateTime<Utc>                                   │
│ updated_at: DateTime<Utc>                                   │
│ storage_id: Option<StorageId>                               │
│ use_user_storage: bool                                      │
│ flush_policy: FlushPolicy                                   │
│ deleted_retention_hours: Option<u32>                        │
│ ttl_seconds: Option<u64>                                    │
│ columns: Vec<ColumnDefinition> (1:N embedded)               │
│ schema_history: Vec<SchemaVersion> (1:N embedded)           │
│ table_options: HashMap<String, String>                      │
└─────────────────────────────────────────────────────────────┘
                │
                │ 1:N (embedded)
                ├────────────────────────┐
                │                        │
                ▼                        ▼
┌──────────────────────────────┐  ┌─────────────────────────┐
│     ColumnDefinition         │  │    SchemaVersion        │
├──────────────────────────────┤  ├─────────────────────────┤
│ column_name: String          │  │ version: u32            │
│ ordinal_position: u32 (1+)   │  │ created_at: DateTime    │
│ data_type: KalamDataType (→) │  │ changes: String         │
│ is_nullable: bool            │  │ arrow_schema_json: Str  │
│ is_primary_key: bool         │  └─────────────────────────┘
│ is_partition_key: bool       │
│ default_value: ColumnDefault │
│ column_comment: Option<Str>  │
└──────────────────────────────┘
                │
                │ references
                ▼
┌──────────────────────────────────────────────────────────┐
│              KalamDataType (enum)                        │
├──────────────────────────────────────────────────────────┤
│ BOOLEAN                                                  │
│ INT                                                      │
│ BIGINT                                                   │
│ DOUBLE                                                   │
│ FLOAT                                                    │
│ TEXT                                                     │
│ TIMESTAMP                                                │
│ DATE                                                     │
│ DATETIME                                                 │
│ TIME                                                     │
│ JSON                                                     │
│ BYTES                                                    │
│ EMBEDDING(usize)  ← Parameterized type for ML/AI        │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│            ColumnDefault (enum)                          │
├──────────────────────────────────────────────────────────┤
│ None                      ← No default value             │
│ Literal(Value)            ← Static value (e.g., 42, "x") │
│ FunctionCall {            ← Parameterized functions      │
│   name: String,           ← e.g., "SNOWFLAKE", "UUID"    │
│   args: Vec<String>       ← e.g., ["1", "42"]            │
│ }                                                        │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│              TableType (enum)                            │
├──────────────────────────────────────────────────────────┤
│ SYSTEM       ← System tables (users, jobs, namespaces)  │
│ USER         ← User-created regular tables               │
│ SHARED       ← Cross-user shared tables                  │
│ STREAM       ← Stream tables with TTL and eviction       │
└──────────────────────────────────────────────────────────┘

Storage Layer:
┌──────────────────────────────────────────────────────────┐
│  EntityStore<TableId, TableDefinition>                   │
├──────────────────────────────────────────────────────────┤
│  Location: kalamdb-core/src/tables/system/schemas       │
│  Primary Key: TableId (unique, contains namespace.name) │
│  Secondary Indexes: NONE (TableId uniqueness sufficient)│
│  Serialization: bincode                                  │
│  Backend: RocksDB                                        │
└──────────────────────────────────────────────────────────┘

Caching Layer:
┌──────────────────────────────────────────────────────────┐
│  SchemaCache (DashMap<TableId, Arc<TableDefinition>>)   │
├──────────────────────────────────────────────────────────┤
│  Concurrency: Lock-free (DashMap sharding)               │
│  Max Size: 1000 entries (LRU eviction)                   │
│  Cache Invalidation: On ALTER/DROP TABLE                 │
└──────────────────────────────────────────────────────────┘
```

---

## Entity Definitions

### TableDefinition

**Purpose**: Primary table metadata entity, single source of truth for table schema.

**Fields**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    /// Unique table identifier (immutable)
    pub table_id: TableId,
    
    /// Human-readable table name (unique within namespace)
    pub table_name: String,
    
    /// Namespace this table belongs to (indexed for namespace queries)
    pub namespace_id: NamespaceId,
    
    /// Table type: User, System, Information (indexed)
    pub table_type: TableType,
    
    /// Current schema version (increments on ALTER TABLE)
    pub schema_version: u32,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Last modification timestamp
    pub updated_at: DateTime<Utc>,
    
    /// Optional storage backend identifier
    pub storage_id: Option<StorageId>,
    
    /// Whether to use user-specific storage
    pub use_user_storage: bool,
    
    /// Flush policy for writes (immediate, batched, time-based)
    pub flush_policy: FlushPolicy,
    
    /// Retention hours for deleted rows (soft delete)
    pub deleted_retention_hours: Option<u32>,
    
    /// TTL for all rows (automatic expiration)
    pub ttl_seconds: Option<u64>,
    
    /// Column definitions (sorted by ordinal_position)
    pub columns: Vec<ColumnDefinition>,
    
    /// Schema history (sorted by version, embedded for atomic updates)
    pub schema_history: Vec<SchemaVersion>,
    
    /// Extensibility: compression, replication_factor, etc.
    pub table_options: HashMap<String, String>,
}
```

**Invariants**:
- `table_id` is globally unique (enforced by EntityStore)
- `schema_version` starts at 1, increments monotonically
- `columns` is non-empty (at least 1 column required)
- `columns` is sorted by `ordinal_position` (ascending)
- `schema_history` contains `schema_version` entries (1 per version)
- `schema_history` is sorted by `version` (ascending)

**Methods**:

```rust
impl TableDefinition {
    /// Create new table definition (initial schema version = 1)
    pub fn new(
        table_name: String,
        namespace_id: NamespaceId,
        table_type: TableType,
        columns: Vec<ColumnDefinition>,
    ) -> Self;
    
    /// Convert to Arrow Schema for DataFusion
    pub fn to_arrow_schema(&self) -> Result<Arc<Schema>>;
    
    /// Get schema at specific version (O(log N) binary search)
    pub fn get_schema_at_version(&self, version: u32) -> Option<&SchemaVersion>;
    
    /// Add new schema version (after ALTER TABLE)
    pub fn add_schema_version(&mut self, changes: String, arrow_schema_json: String);
    
    /// Get column by name (O(N) scan, use sparingly)
    pub fn get_column(&self, column_name: &str) -> Option<&ColumnDefinition>;
    
    /// Get columns sorted by ordinal_position (for SELECT *)
    pub fn get_columns_ordered(&self) -> &[ColumnDefinition];
    
    /// Validate table definition (check invariants)
    pub fn validate(&self) -> Result<()>;
}
```

---

### ColumnDefinition

**Purpose**: Column metadata, defines structure and constraints for a single column.

**Fields**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Column name (unique within table)
    pub column_name: String,
    
    /// 1-indexed position (immutable, determines SELECT * order)
    pub ordinal_position: u32,
    
    /// Data type (unified KalamDataType enum)
    pub data_type: KalamDataType,
    
    /// Whether NULL values are allowed
    pub is_nullable: bool,
    
    /// Whether this column is part of primary key
    pub is_primary_key: bool,
    
    /// Whether this column is used for partitioning
    pub is_partition_key: bool,
    
    /// Default value specification
    pub default_value: ColumnDefault,
    
    /// Optional human-readable comment
    pub column_comment: Option<String>,
}
```

**Invariants**:
- `column_name` is non-empty
- `ordinal_position >= 1` (1-indexed)
- `ordinal_position` is unique within table
- If `is_primary_key == true`, then `is_nullable == false`
- If `is_partition_key == true`, column exists in table

**Methods**:

```rust
impl ColumnDefinition {
    /// Create new column definition
    pub fn new(
        column_name: String,
        ordinal_position: u32,
        data_type: KalamDataType,
        is_nullable: bool,
    ) -> Self;
    
    /// Convert to Arrow Field for schema construction
    pub fn to_arrow_field(&self) -> Field;
    
    /// Validate column definition
    pub fn validate(&self) -> Result<()>;
}
```

---

### SchemaVersion

**Purpose**: Schema history entry, tracks schema evolution over time.

**Fields**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Version number (matches TableDefinition.schema_version)
    pub version: u32,
    
    /// When this version was created
    pub created_at: DateTime<Utc>,
    
    /// Human-readable description of changes
    pub changes: String,
    
    /// Serialized Arrow schema (for exact historical schema)
    pub arrow_schema_json: String,
}
```

**Invariants**:
- `version >= 1`
- `version` is unique within `TableDefinition.schema_history`
- `arrow_schema_json` is valid JSON

**Methods**:

```rust
impl SchemaVersion {
    /// Create new schema version
    pub fn new(version: u32, changes: String, arrow_schema_json: String) -> Self;
    
    /// Parse Arrow schema from JSON
    pub fn get_arrow_schema(&self) -> Result<Arc<Schema>>;
}
```

---

### KalamDataType

**Purpose**: Unified data type enum, single source of truth for all type representations.

**Definition**:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KalamDataType {
    BOOLEAN,
    INT,              // i32
    BIGINT,           // i64
    DOUBLE,           // f64
    FLOAT,            // f32
    TEXT,             // UTF-8 string
    TIMESTAMP,        // Microseconds since epoch
    DATE,             // Days since epoch
    DATETIME,         // Microseconds since epoch
    TIME,             // Microseconds since midnight
    JSON,             // UTF-8 JSON string
    BYTES,            // Raw binary data
    EMBEDDING(usize), // Fixed-length f32 vector (dimension)
}
```

**Wire Format Tags**:

```rust
pub const TAG_BOOLEAN: u8 = 0x01;
pub const TAG_INT: u8 = 0x02;
pub const TAG_BIGINT: u8 = 0x03;
pub const TAG_DOUBLE: u8 = 0x04;
pub const TAG_FLOAT: u8 = 0x05;
pub const TAG_TEXT: u8 = 0x06;
pub const TAG_TIMESTAMP: u8 = 0x07;
pub const TAG_DATE: u8 = 0x08;
pub const TAG_DATETIME: u8 = 0x09;
pub const TAG_TIME: u8 = 0x0A;
pub const TAG_JSON: u8 = 0x0B;
pub const TAG_BYTES: u8 = 0x0C;
pub const TAG_EMBEDDING: u8 = 0x0D;
```

**Methods**:

```rust
impl KalamDataType {
    /// Convert to Arrow DataType (cached via DashMap)
    pub fn to_arrow_type(&self) -> DataType;
    
    /// Convert from Arrow DataType (bidirectional, lossless)
    pub fn from_arrow_type(dt: &DataType) -> Result<Self>;
    
    /// Get wire format tag byte
    pub fn wire_tag(&self) -> u8;
    
    /// Serialize to wire format (tag + value bytes)
    pub fn serialize_value(&self, value: &Value) -> Result<Vec<u8>>;
    
    /// Deserialize from wire format
    pub fn deserialize_value(bytes: &[u8]) -> Result<(Self, Value)>;
    
    /// Validate dimension for parameterized types
    pub fn validate(&self) -> Result<()>;
}
```

**Validation Rules**:
- `EMBEDDING(dim)`: `1 <= dim <= 8192`
- All types: Must map to valid Arrow DataType

---

### ColumnDefault

**Purpose**: Represent default value specifications for columns with support for parameterized functions.

**Definition**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColumnDefault {
    /// No default value (NULL for nullable columns, error for non-nullable)
    None,
    
    /// Static literal value
    Literal(Value),
    
    /// Function call with optional parameters
    FunctionCall {
        name: String,
        args: Vec<String>,
    },
}
```

**Examples**:

```rust
// No default
ColumnDefault::None

// Literal defaults
ColumnDefault::Literal(Value::Int(42))
ColumnDefault::Literal(Value::Text("unknown".into()))

// Function defaults without parameters
ColumnDefault::FunctionCall {
    name: "NOW".into(),
    args: vec![],
}

ColumnDefault::FunctionCall {
    name: "UUID".into(),
    args: vec![],
}

// Function defaults WITH parameters
ColumnDefault::FunctionCall {
    name: "SNOWFLAKE".into(),
    args: vec!["1".into(), "42".into()],  // SNOWFLAKE(datacenter_id=1, worker_id=42)
}

ColumnDefault::FunctionCall {
    name: "RANDOM_STRING".into(),
    args: vec!["16".into()],  // RANDOM_STRING(length=16)
}
```

---

### TableType

**Purpose**: Enum representing the four categories of tables in KalamDB.

**Definition**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TableType {
    /// System tables (users, jobs, namespaces, storages, live_queries, etc.)
    SYSTEM,
    
    /// User-created regular tables
    USER,
    
    /// Cross-user shared tables (accessible by multiple users)
    SHARED,
    
    /// Stream tables with TTL and eviction policies
    STREAM,
}
```

**Characteristics**:

- **SYSTEM**: Built-in system tables managed by KalamDB, not user-modifiable schema
- **USER**: Standard user tables, each belongs to a specific namespace, normal CRUD operations
- **SHARED**: Special tables accessible across namespaces, used for shared data
- **STREAM**: Ephemeral tables with automatic TTL-based eviction, optimized for time-series data

**All four types** have TableDefinition entries in the schema store.

---

## Entity Relationships

### TableDefinition → ColumnDefinition (1:N embedded)

- **Cardinality**: 1 TableDefinition contains 1+ ColumnDefinitions
- **Storage**: Embedded in TableDefinition (not separate EntityStore)
- **Ordering**: Columns sorted by `ordinal_position` for SELECT * determinism
- **Lifecycle**: Columns created/modified/deleted via ALTER TABLE commands

### TableDefinition → SchemaVersion (1:N embedded)

- **Cardinality**: 1 TableDefinition contains 1+ SchemaVersions (1 per schema change)
- **Storage**: Embedded in TableDefinition for atomic updates
- **Ordering**: Sorted by `version` for O(log N) binary search
- **Lifecycle**: New SchemaVersion added on each ALTER TABLE operation

### ColumnDefinition → KalamDataType (N:1 reference)

- **Cardinality**: Many ColumnDefinitions can reference the same KalamDataType
- **Storage**: KalamDataType is enum value (not entity), stored inline
- **Caching**: Arrow conversions cached in DashMap for performance

### TableDefinition → EntityStore (N:1 storage)

- **Cardinality**: Many TableDefinitions stored in 1 EntityStore
- **Location**: `backend/crates/kalamdb-core/src/tables/system/schemas` (single source of truth)
- **Key**: TableId (globally unique, contains namespace.tableName)
- **Serialization**: bincode for compact storage
- **Secondary Indexes**: NONE (TableId uniqueness makes them unnecessary)

### EntityStore → SchemaCache (1:1 layering)

- **Cardinality**: 1 EntityStore backed by 1 SchemaCache
- **Purpose**: Cache frequently accessed TableDefinitions to avoid EntityStore reads
- **Invalidation**: Cache invalidated on ALTER TABLE, DROP TABLE
- **Max Size**: 1000 entries (LRU eviction)

### TableDefinition → TableType (N:1 reference)

- **Cardinality**: Each TableDefinition has one TableType (SYSTEM, USER, SHARED, or STREAM)
- **Storage**: TableType is enum value (Copy type), stored inline in TableDefinition
- **No Indexing**: Table type filtering done in-memory, not via secondary index

---

## State Transitions

### CREATE TABLE

**Initial State**: No table exists  
**Action**: User executes `CREATE TABLE` statement  
**Transitions**:
1. Parse SQL → Extract table_name, columns, options
2. Generate TableId (new unique ID)
3. Create TableDefinition with schema_version = 1
4. Create initial SchemaVersion with changes = "Initial schema"
5. Sort columns by ordinal_position
6. Validate TableDefinition
7. Insert into EntityStore with secondary indexes
8. Insert into SchemaCache
9. Commit transaction

**Final State**: TableDefinition exists with 1 column, 1 SchemaVersion

**Rollback**: On error, no state persisted (transaction aborted)

---

### ALTER TABLE ADD COLUMN

**Initial State**: TableDefinition exists with N columns  
**Action**: User executes `ALTER TABLE ADD COLUMN`  
**Transitions**:
1. Parse SQL → Extract column definition
2. Load current TableDefinition from SchemaCache (or EntityStore)
3. Assign ordinal_position = max(existing ordinal_positions) + 1
4. Create new ColumnDefinition
5. Append to TableDefinition.columns
6. Increment schema_version
7. Create new SchemaVersion with changes = "Added column X"
8. Append to schema_history
9. Validate TableDefinition
10. Update EntityStore (atomic write)
11. Invalidate SchemaCache for this table_id
12. Commit transaction

**Final State**: TableDefinition has N+1 columns, schema_version incremented, new SchemaVersion in history

**Rollback**: On error, revert to previous TableDefinition (transaction aborted)

---

### ALTER TABLE DROP COLUMN

**Initial State**: TableDefinition exists with N columns  
**Action**: User executes `ALTER TABLE DROP COLUMN`  
**Transitions**:
1. Parse SQL → Extract column_name
2. Load current TableDefinition
3. Find ColumnDefinition by column_name
4. Remove ColumnDefinition from columns Vec
5. **DO NOT** renumber ordinal_positions (preserve existing ordinals)
6. Increment schema_version
7. Create new SchemaVersion with changes = "Dropped column X"
8. Append to schema_history
9. Validate TableDefinition (ensure at least 1 column remains)
10. Update EntityStore
11. Invalidate SchemaCache
12. Commit transaction

**Final State**: TableDefinition has N-1 columns, schema_version incremented, ordinal_positions preserved for remaining columns

**Rollback**: On error, revert to previous TableDefinition

**Edge Case**: If dropping last column, reject operation (tables must have ≥1 column)

---

### DROP TABLE

**Initial State**: TableDefinition exists  
**Action**: User executes `DROP TABLE`  
**Transitions**:
1. Parse SQL → Extract table_name
2. Lookup TableDefinition by table_name
3. **Soft Delete**: Mark TableDefinition as deleted (add deleted_at timestamp)
4. Update EntityStore (preserve for recovery)
5. Invalidate SchemaCache
6. Commit transaction

**Final State**: TableDefinition marked as deleted, still in EntityStore for retention period

**Hard Delete** (after retention period):
1. Background job queries deleted tables
2. If deleted_at > retention hours, remove from EntityStore
3. Remove from secondary indexes

---

### Query Schema (SELECT *)

**Initial State**: TableDefinition exists in EntityStore  
**Action**: User executes `SELECT * FROM table`  
**Transitions**:
1. Lookup TableDefinition in SchemaCache by table_id
2. **Cache Hit**: Return Arc<TableDefinition> (fast path)
3. **Cache Miss**: Load from EntityStore → Insert into cache → Return
4. Get columns sorted by ordinal_position
5. Build Arrow RecordBatch with columns in order
6. Execute query

**Final State**: No state change (read-only operation)

**Performance**: <100μs for cached schemas (target from spec)

---

### Query Historical Schema

**Initial State**: TableDefinition with schema_history  
**Action**: User executes query with `@version=5` clause  
**Transitions**:
1. Load TableDefinition from cache/EntityStore
2. Binary search schema_history for version = 5
3. Parse arrow_schema_json to get historical Arrow schema
4. Use historical schema for query execution

**Final State**: No state change

**Performance**: O(log N) for schema_history lookup (binary search)

---

## Validation Rules

### TableDefinition Validation

```rust
impl TableDefinition {
    pub fn validate(&self) -> Result<()> {
        // Must have at least 1 column
        if self.columns.is_empty() {
            return Err(KalamDbError::InvalidSchema("Table must have at least 1 column".into()));
        }
        
        // Columns must be sorted by ordinal_position
        for i in 1..self.columns.len() {
            if self.columns[i].ordinal_position <= self.columns[i-1].ordinal_position {
                return Err(KalamDbError::InvalidSchema("Columns not sorted by ordinal_position".into()));
            }
        }
        
        // Ordinal positions must be 1-indexed and unique
        let mut seen = HashSet::new();
        for col in &self.columns {
            if col.ordinal_position < 1 {
                return Err(KalamDbError::InvalidSchema("ordinal_position must be >= 1".into()));
            }
            if !seen.insert(col.ordinal_position) {
                return Err(KalamDbError::InvalidSchema(format!("Duplicate ordinal_position: {}", col.ordinal_position)));
            }
        }
        
        // Schema history must have entry for each version
        if self.schema_history.len() != self.schema_version as usize {
            return Err(KalamDbError::InvalidSchema("schema_history length != schema_version".into()));
        }
        
        // Schema history must be sorted by version
        for i in 1..self.schema_history.len() {
            if self.schema_history[i].version <= self.schema_history[i-1].version {
                return Err(KalamDbError::InvalidSchema("schema_history not sorted by version".into()));
            }
        }
        
        // Validate each column
        for col in &self.columns {
            col.validate()?;
        }
        
        Ok(())
    }
}
```

### ColumnDefinition Validation

```rust
impl ColumnDefinition {
    pub fn validate(&self) -> Result<()> {
        // Column name must be non-empty
        if self.column_name.is_empty() {
            return Err(KalamDbError::InvalidSchema("column_name cannot be empty".into()));
        }
        
        // Primary key columns must be non-nullable
        if self.is_primary_key && self.is_nullable {
            return Err(KalamDbError::InvalidSchema(format!("Primary key column '{}' cannot be nullable", self.column_name)));
        }
        
        // Validate data type
        self.data_type.validate()?;
        
        Ok(())
    }
}
```

### KalamDataType Validation

```rust
impl KalamDataType {
    pub fn validate(&self) -> Result<()> {
        match self {
            KalamDataType::EMBEDDING(dim) => {
                if *dim == 0 {
                    return Err(KalamDbError::InvalidDataType("EMBEDDING dimension must be >= 1".into()));
                }
                if *dim > 8192 {
                    return Err(KalamDbError::InvalidDataType("EMBEDDING dimension exceeds maximum (8192)".into()));
                }
                Ok(())
            }
            _ => Ok(()), // Other types have no validation constraints
        }
    }
}
```

---

## Performance Characteristics

| Operation | Complexity | Target Performance | Notes |
|-----------|------------|-------------------|-------|
| Load TableDefinition (cached) | O(1) | <100μs | DashMap lookup |
| Load TableDefinition (uncached) | O(log N) | <1ms | EntityStore read + bincode deserialize |
| ALTER TABLE ADD COLUMN | O(N + log M) | <10ms | N=columns, M=schema_history (binary search) |
| Query schema at version V | O(log M) | <1ms | Binary search schema_history |
| SELECT * column ordering | O(N log N) | <1ms | Sort by ordinal_position (usually pre-sorted) |
| Namespace-scoped query | O(log K + R) | <10ms | K=total tables, R=results (secondary index scan) |
| Cache invalidation | O(1) | <10μs | DashMap remove |
| Schema validation | O(N) | <100μs | N=columns, linear scan |

**Memory Usage**:
- TableDefinition (typical): ~2KB (10 columns, 5 schema versions)
- ColumnDefinition: ~200 bytes (name + metadata)
- SchemaVersion: ~500 bytes (JSON schema)
- SchemaCache (1000 entries): ~2MB (bounded by LRU eviction)

---

## Integration Points

### EntityStore Integration

```rust
// TableSchemaStore implements EntityStore<TableId, TableDefinition>
// Location: backend/crates/kalamdb-core/src/tables/system/schemas/table_schema_store.rs
pub struct TableSchemaStore {
    entity_store: Arc<SystemTableStore<TableId, TableDefinition>>,
}

impl EntityStore<TableId, TableDefinition> for TableSchemaStore {
    fn get(&self, key: &TableId) -> Result<Option<TableDefinition>> {
        self.entity_store.get(key)
    }
    
    fn put(&self, key: TableId, value: TableDefinition) -> Result<()> {
        // Simple put - no secondary indexes needed
        self.entity_store.put(key, value)
    }
    
    fn delete(&self, key: &TableId) -> Result<()> {
        self.entity_store.delete(key)
    }
    
    // Helper method for namespace-scoped queries (in-memory filter)
    fn get_by_namespace(&self, namespace_id: &NamespaceId) -> Result<Vec<TableDefinition>> {
        // Load all tables and filter by namespace (acceptable for Alpha)
        let all_tables = self.get_all()?;
        Ok(all_tables.into_iter()
            .filter(|t| &t.namespace_id == namespace_id)
            .collect())
    }
    
    // Helper method for table type queries (in-memory filter)
    fn get_by_type(&self, table_type: TableType) -> Result<Vec<TableDefinition>> {
        let all_tables = self.get_all()?;
        Ok(all_tables.into_iter()
            .filter(|t| t.table_type == table_type)
            .collect())
    }
}
```

### SchemaCache Integration

```rust
pub struct SchemaCache {
    cache: Arc<DashMap<TableId, Arc<TableDefinition>>>,
    max_size: usize,
    store: Arc<dyn EntityStore<TableId, TableDefinition>>,
}

impl SchemaCache {
    pub fn get(&self, table_id: &TableId) -> Result<Arc<TableDefinition>> {
        // Try cache first
        if let Some(entry) = self.cache.get(table_id) {
            return Ok(entry.value().clone());
        }
        
        // Cache miss - load from store
        let table_def = self.store.get(table_id)?
            .ok_or_else(|| KalamDbError::TableNotFound(table_id.clone()))?;
        
        let table_def_arc = Arc::new(table_def);
        
        // Insert into cache (with LRU eviction if needed)
        self.insert_with_eviction(table_id.clone(), table_def_arc.clone())?;
        
        Ok(table_def_arc)
    }
    
    fn insert_with_eviction(&self, key: TableId, value: Arc<TableDefinition>) -> Result<()> {
        if self.cache.len() >= self.max_size {
            // TODO: Implement LRU eviction
            // For now, just allow cache to grow (will be fixed in FR-QUALITY-004)
        }
        self.cache.insert(key, value);
        Ok(())
    }
}
```

---

## Summary

The data model provides:
- **Single source of truth** for table schemas (TableDefinition)
- **Type safety** via KalamDataType enum (13 variants including vector embeddings)
- **Schema evolution** tracking via embedded SchemaVersion history
- **Deterministic column ordering** via ordinal_position
- **Efficient caching** via DashMap-based SchemaCache
- **Atomic updates** via EntityStore with secondary indexes

All entities follow Phase 14 EntityStore patterns for consistency with existing system tables (users, jobs, namespaces).

---

**Data Model Completed**: 2025-11-01  
**Next Deliverable**: quickstart.md (developer getting started guide)
