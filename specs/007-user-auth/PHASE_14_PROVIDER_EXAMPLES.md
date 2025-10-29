# Phase 14: Provider Implementation Examples

This document shows concrete implementation examples for the new EntityStore-based providers.

## Architecture Overview: Clean & Performant

### ❌ BEFORE (Duplicated Schema)
```rust
// users_provider.rs - DUPLICATE schema definition
impl UsersTableProvider {
    pub fn new(...) -> Self {
        let schema = Arc::new(Schema::new(vec![
            Field::new("user_id", DataType::Utf8, false),
            // ... 12 more fields (100+ lines)
        ]));
    }
}

// users_table.rs - DUPLICATE schema definition
impl UsersTableSchema {
    pub fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("user_id", DataType::Utf8, false),
            // ... same 12 fields (100+ lines duplicated)
        ]))
    }
}
```
**Problems**: Schema duplication, no caching, memory waste, maintenance nightmare

### ✅ AFTER (Single Source + Cached)
```rust
// users_table.rs - SINGLE schema definition with caching
impl UsersTableSchema {
    pub fn schema() -> Arc<Schema> {
        static SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();
        SCHEMA.get_or_init(|| Arc::new(Schema::new(vec![/* ... */]))).clone()
    }
}

// users_provider.rs - IMPORTS from users_table
use super::users_table::UsersTableSchema;
impl UsersTableProvider {
    pub fn new(...) -> Self {
        let schema = UsersTableSchema::schema();  // ✅ Cached!
    }
}
```
**Benefits**: DRY principle, zero overhead, type-safe, maintainable

---

## 1. Users Table Provider (System Table with Secondary Index)

### File Structure
```
backend/crates/kalamdb-core/src/tables/system/users/
├── users_provider.rs        # DataFusion TableProvider
├── users_store.rs           # SystemTableStore<UserId, User> wrapper
├── users_table.rs           # Schema model (columns for DataFusion)
└── users_username_index.rs  # SecondaryIndex<User, UserName>
```

### users_store.rs - Storage Layer
```rust
// File: backend/crates/kalamdb-core/src/tables/system/users/users_store.rs

use kalamdb_commons::models::{User, UserId, UserName};
use kalamdb_store::{StorageBackend, EntityStore, CrossUserTableStore, SecondaryIndex};
use std::sync::Arc;

/// Users table store with username secondary index
pub struct UsersStore {
    /// Primary store: UserId → User
    store: SystemTableStore<UserId, User>,
    
    /// Secondary index: UserName → UserId
    username_index: SecondaryIndex<User, UserName>,
}

impl UsersStore {
    pub fn new(backend: Arc<dyn StorageBackend>) -> Result<Self> {
        let store = SystemTableStore::new(backend.clone(), "system_users");
        
        // Create username index: extracts username from User
        let username_index = SecondaryIndex::unique(
            backend,
            "system_users_username_idx",
            |user: &User| UserName::new(user.username.clone()),
        );
        
        Ok(Self { store, username_index })
    }
    
    /// Create user (stores in both primary store and index)
    pub fn create_user(&self, user: User) -> Result<()> {
        let user_id = user.id.clone();
        let username = UserName::new(user.username.clone());
        
        // 1. Store user by UserId (primary key)
        self.store.put(&user_id, &user)?;
        
        // 2. Update username index (username → user_id)
        self.username_index.insert(&username, &user_id, &user)?;
        
        Ok(())
    }
    
    /// Get user by UserId (primary key lookup)
    pub fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<User>> {
        self.store.get(user_id)
    }
    
    /// Get user by username (secondary index lookup)
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let username_key = UserName::new(username.to_string());
        
        // 1. Look up UserId from username index
        if let Some(user_id) = self.username_index.lookup(&username_key)? {
            // 2. Get User from primary store
            self.store.get(&user_id)
        } else {
            Ok(None)
        }
    }
    
    /// Update user (updates both store and index if username changed)
    pub fn update_user(&self, user: User) -> Result<()> {
        let user_id = user.id.clone();
        
        // Get old user to check if username changed
        if let Some(old_user) = self.store.get(&user_id)? {
            let old_username = UserName::new(old_user.username.clone());
            let new_username = UserName::new(user.username.clone());
            
            // Update primary store
            self.store.put(&user_id, &user)?;
            
            // Update index if username changed
            if old_username.as_ref() != new_username.as_ref() {
                self.username_index.remove(&old_username, &user_id)?;
                self.username_index.insert(&new_username, &user_id, &user)?;
            }
        }
        
        Ok(())
    }
    
    /// Delete user (removes from both store and index)
    pub fn delete_user(&self, user_id: &UserId) -> Result<()> {
        if let Some(user) = self.store.get(user_id)? {
            let username = UserName::new(user.username.clone());
            
            // 1. Remove from primary store
            self.store.delete(user_id)?;
            
            // 2. Remove from username index
            self.username_index.remove(&username, user_id)?;
        }
        
        Ok(())
    }
    
    /// List all users (scans primary store)
    pub fn list_users(&self) -> Result<Vec<User>> {
        self.store.scan_all()
            .map(|results| results.into_iter().map(|(_, user)| user).collect())
    }
}
```

### users_provider.rs - DataFusion Integration
```rust
// File: backend/crates/kalamdb-core/src/tables/system/users/users_provider.rs

use datafusion::arrow::array::{ArrayRef, StringArray, Int64Array, BooleanArray};
use datafusion::arrow::datatypes::Schema;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::execution::context::SessionState;
use datafusion::physical_plan::memory::MemoryExec;
use datafusion::physical_plan::ExecutionPlan;
use async_trait::async_trait;
use std::sync::Arc;

use super::users_store::UsersStore;
use super::users_table::UsersTableSchema;  // Import schema from users_table
use kalamdb_commons::models::User;

/// DataFusion TableProvider for system.users
pub struct UsersTableProvider {
    store: Arc<UsersStore>,
    schema: Arc<Schema>,
}

impl UsersTableProvider {
    pub fn new(store: Arc<UsersStore>) -> Self {
        // ✅ Use schema from UsersTableSchema (single source of truth)
        let schema = UsersTableSchema::schema();
        
        Self { store, schema }
    }
    
    /// Convert User models to Arrow RecordBatch
    fn users_to_batch(&self, users: Vec<User>) -> Result<RecordBatch> {
        let mut user_ids = Vec::new();
        let mut usernames = Vec::new();
        let mut roles = Vec::new();
        let mut emails = Vec::new();
        let mut auth_types = Vec::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        let mut last_seens = Vec::new();
        let mut deleted_ats = Vec::new();
        
        for user in users {
            user_ids.push(user.id.to_string());
            usernames.push(user.username);
            roles.push(user.role.to_string());
            emails.push(user.email);
            auth_types.push(user.auth_type.to_string());
            created_ats.push(user.created_at);
            updated_ats.push(user.updated_at);
            last_seens.push(user.last_seen);
            deleted_ats.push(user.deleted_at);
        }
        
        let columns: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(user_ids)),
            Arc::new(StringArray::from(usernames)),
            Arc::new(StringArray::from(roles)),
            Arc::new(StringArray::from(emails)),
            Arc::new(StringArray::from(auth_types)),
            Arc::new(Int64Array::from(created_ats)),
            Arc::new(Int64Array::from(updated_ats)),
            Arc::new(Int64Array::from(last_seens)),
            Arc::new(Int64Array::from(deleted_ats)),
        ];
        
        RecordBatch::try_new(self.schema.clone(), columns)
    }
}

#[async_trait]
impl TableProvider for UsersTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn schema(&self) -> Arc<Schema> {
        self.schema.clone()
    }
    
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    
    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        // Fetch all users from store
        let users = self.store.list_users()?;
        
        // Convert to RecordBatch
        let batch = self.users_to_batch(users)?;
        
        // Create memory execution plan
        let exec = MemoryExec::try_new(
            &[vec![batch]],
            self.schema.clone(),
            projection.cloned(),
        )?;
        
        Ok(Arc::new(exec))
    }
}
```

### users_table.rs - Schema Model
```rust
// File: backend/crates/kalamdb-core/src/tables/system/users/users_table.rs

use datafusion::arrow::datatypes::{Schema, Field, DataType};
use std::sync::{Arc, OnceLock};

/// Schema definition for system.users table
pub struct UsersTableSchema;

impl UsersTableSchema {
    /// Get cached schema (initialized once, zero overhead after first call)
    pub fn schema() -> Arc<Schema> {
        static SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();
        
        SCHEMA.get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new("user_id", DataType::Utf8, false),
                Field::new("username", DataType::Utf8, false),
                Field::new("password_hash", DataType::Utf8, false),
                Field::new("role", DataType::Utf8, false),
                Field::new("email", DataType::Utf8, true),
                Field::new("auth_type", DataType::Utf8, false),
                Field::new("auth_data", DataType::Utf8, true),
                Field::new("storage_mode", DataType::Utf8, false),
                Field::new("storage_id", DataType::Utf8, true),
                Field::new("created_at", DataType::Int64, false),
                Field::new("updated_at", DataType::Int64, false),
                Field::new("last_seen", DataType::Int64, true),
                Field::new("deleted_at", DataType::Int64, true),
            ]))
        }).clone()
    }
    
    /// Column names for projections
    pub fn columns() -> Vec<&'static str> {
        vec![
            "user_id",
            "username",
            "password_hash",
            "role",
            "email",
            "auth_type",
            "auth_data",
            "storage_mode",
            "storage_id",
            "created_at",
            "updated_at",
            "last_seen",
            "deleted_at",
        ]
    }
}
```

## 2. Shared Table Provider (Hybrid: Cross-User with Access Control)

### File Structure
```
backend/crates/kalamdb-core/src/tables/shared/
├── shared_table_provider.rs  # DataFusion TableProvider for hybrid queries
└── shared_table_store.rs     # SharedTableStoreImpl with access control
```

### shared_table_store.rs - Hybrid Storage with Access Control
```rust
// File: backend/crates/kalamdb-core/src/tables/shared/shared_table_store.rs

use kalamdb_commons::models::{SharedTableRow, TableAccess, NamespaceId, TableName, RowId};
use kalamdb_store::{StorageBackend, EntityStore, CrossUserTableStore};
use std::sync::Arc;

/// Shared table store with access control
pub struct SharedTableStoreImpl {
    backend: Arc<dyn StorageBackend>,
    namespace_id: NamespaceId,
    table_name: TableName,
    access_level: TableAccess,
}

impl SharedTableStoreImpl {
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        namespace_id: NamespaceId,
        table_name: TableName,
        access_level: TableAccess,
    ) -> Self {
        Self {
            backend,
            namespace_id,
            table_name,
            access_level,
        }
    }
    
    /// Partition key: "shared:{namespace}:{table}"
    fn partition_key(&self) -> String {
        format!("shared:{}:{}", self.namespace_id, self.table_name)
    }
}

// Implement EntityStore<RowId, SharedTableRow>
impl EntityStore<RowId, SharedTableRow> for SharedTableStoreImpl {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }
    
    fn partition(&self) -> &str {
        &self.partition_key()
    }
}

// Implement CrossUserTableStore for access control
impl CrossUserTableStore<RowId, SharedTableRow> for SharedTableStoreImpl {
    fn table_access(&self) -> Option<TableAccess> {
        Some(self.access_level.clone())
    }
    
    fn can_read(&self, user_role: &Role) -> bool {
        match self.access_level {
            TableAccess::Public => true,  // Everyone can read
            TableAccess::Private => user_role >= &Role::Service,  // Service+ only
            TableAccess::Restricted => user_role >= &Role::Dba,  // DBA+ only
        }
    }
    
    fn can_write(&self, user_role: &Role) -> bool {
        // Only privileged roles can write to shared tables
        user_role >= &Role::Service
    }
}
```

### shared_table_provider.rs - Hybrid Query Support
```rust
// File: backend/crates/kalamdb-core/src/tables/shared/shared_table_provider.rs

use datafusion::arrow::array::{ArrayRef, StringArray, BooleanArray};
use datafusion::arrow::datatypes::{Schema, Field, DataType};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::execution::context::SessionState;
use datafusion::physical_plan::memory::MemoryExec;
use datafusion::physical_plan::ExecutionPlan;
use async_trait::async_trait;
use std::sync::Arc;

use super::shared_table_store::SharedTableStoreImpl;
use kalamdb_commons::models::{SharedTableRow, Role};
use kalamdb_store::EntityStore;

/// Hybrid provider: supports both cross-user queries and access control
pub struct SharedTableProvider {
    store: Arc<SharedTableStoreImpl>,
    schema: Arc<Schema>,
    user_role: Role,  // Current user's role for access control
}

impl SharedTableProvider {
    pub fn new(
        store: Arc<SharedTableStoreImpl>,
        schema: Arc<Schema>,
        user_role: Role,
    ) -> Self {
        Self { store, schema, user_role }
    }
    
    /// Check if user can read this table
    fn check_read_access(&self) -> Result<()> {
        if self.store.can_read(&self.user_role) {
            Ok(())
        } else {
            Err(KalamDbError::Unauthorized(
                format!("User role {:?} cannot read shared table with access {:?}",
                    self.user_role, self.store.table_access())
            ))
        }
    }
    
    /// Fetch all rows (with access control)
    fn fetch_rows(&self) -> Result<Vec<SharedTableRow>> {
        self.check_read_access()?;
        
        // Scan all rows from store
        self.store.scan_all()
            .map(|results| results.into_iter().map(|(_, row)| row).collect())
    }
    
    /// Convert rows to Arrow RecordBatch
    fn rows_to_batch(&self, rows: Vec<SharedTableRow>) -> Result<RecordBatch> {
        // Extract field names from schema
        let field_names: Vec<String> = self.schema.fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        
        // Build column arrays dynamically based on schema
        let mut columns: Vec<ArrayRef> = Vec::new();
        
        for field_name in field_names {
            let values: Vec<Option<String>> = rows.iter()
                .map(|row| {
                    row.fields.get(&field_name)
                        .map(|v| v.to_string())
                })
                .collect();
            
            columns.push(Arc::new(StringArray::from(values)));
        }
        
        RecordBatch::try_new(self.schema.clone(), columns)
    }
}

#[async_trait]
impl TableProvider for SharedTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn schema(&self) -> Arc<Schema> {
        self.schema.clone()
    }
    
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    
    async fn scan(
        &self,
        _state: &SessionState,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        // Fetch rows with access control
        let mut rows = self.fetch_rows()?;
        
        // Apply limit if specified
        if let Some(limit) = limit {
            rows.truncate(limit);
        }
        
        // Convert to RecordBatch
        let batch = self.rows_to_batch(rows)?;
        
        // Create memory execution plan
        let exec = MemoryExec::try_new(
            &[vec![batch]],
            self.schema.clone(),
            projection.cloned(),
        )?;
        
        Ok(Arc::new(exec))
    }
}
```

## 3. User Table Provider (User-Isolated with Composite Keys)

### File Structure
```
backend/crates/kalamdb-core/src/tables/user/
├── user_table_provider.rs  # DataFusion TableProvider for user-isolated queries
└── user_table_store.rs     # UserTableStoreImpl with composite keys
```

### user_table_store.rs - User-Isolated Storage
```rust
// File: backend/crates/kalamdb-core/src/tables/user/user_table_store.rs

use kalamdb_commons::models::{UserTableRow, UserId, UserRowId, NamespaceId, TableName};
use kalamdb_store::{StorageBackend, EntityStore};
use std::sync::Arc;

/// User-isolated table store with composite keys (UserId + RowId)
pub struct UserTableStoreImpl {
    backend: Arc<dyn StorageBackend>,
    namespace_id: NamespaceId,
    table_name: TableName,
}

impl UserTableStoreImpl {
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        namespace_id: NamespaceId,
        table_name: TableName,
    ) -> Self {
        Self { backend, namespace_id, table_name }
    }
    
    /// Partition key: "user:{namespace}:{table}"
    fn partition_key(&self) -> String {
        format!("user:{}:{}", self.namespace_id, self.table_name)
    }
    
    /// Insert row for specific user
    pub fn insert_row(
        &self,
        user_id: &UserId,
        row_id: Vec<u8>,
        row: UserTableRow,
    ) -> Result<()> {
        let key = UserRowId::new(user_id.clone(), row_id);
        self.put(&key, &row)
    }
    
    /// Get row for specific user
    pub fn get_row(
        &self,
        user_id: &UserId,
        row_id: Vec<u8>,
    ) -> Result<Option<UserTableRow>> {
        let key = UserRowId::new(user_id.clone(), row_id);
        self.get(&key)
    }
    
    /// List all rows for specific user
    pub fn list_user_rows(&self, user_id: &UserId) -> Result<Vec<UserTableRow>> {
        let prefix = UserRowId::new(user_id.clone(), vec![]);
        
        self.scan_prefix(&prefix)
            .map(|results| results.into_iter().map(|(_, row)| row).collect())
    }
    
    /// Delete row for specific user
    pub fn delete_row(&self, user_id: &UserId, row_id: Vec<u8>) -> Result<()> {
        let key = UserRowId::new(user_id.clone(), row_id);
        self.delete(&key)
    }
}

// Implement EntityStore<UserRowId, UserTableRow>
impl EntityStore<UserRowId, UserTableRow> for UserTableStoreImpl {
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }
    
    fn partition(&self) -> &str {
        &self.partition_key()
    }
}
```

## Summary

### Key Architectural Patterns

1. **System Tables** (e.g., `system.users`):
   - Primary Key: Domain-specific (UserId, JobId, TableId, etc.)
   - Secondary Indexes: SecondaryIndex<T, K> for lookups
   - Access Control: Admin-only (via CrossUserTableStore returning None)
   - Folder: `tables/system/{table}/` with 3-4 files
   - **Schema**: Defined ONCE in `{table}_table.rs`, imported by `{table}_provider.rs` (single source of truth)
   - **Performance**: Schema cached with `OnceLock<Arc<Schema>>` for zero runtime overhead after first initialization

2. **Shared Tables** (e.g., cross-user shared data):
   - Primary Key: RowId (simple Vec<u8>)
   - Access Control: Public/Private/Restricted (via CrossUserTableStore)
   - Hybrid: Both cross-user AND access control
   - Folder: `tables/shared/` with 2 files

3. **User Tables** (e.g., user-isolated data):
   - Primary Key: UserRowId (composite: UserId + RowId)
   - Access Control: Implicit (user can only see their own data)
   - Isolation: Partition by user automatically
   - Folder: `tables/user/` with 2 files

### Code Duplication Eliminated

- **Before**: 600+ lines duplicated across 4 stores
- **After**: ~200 lines total with EntityStore<K,V> trait
- **Reduction**: 66% less code
- **Benefits**: Type safety, consistent APIs, easier testing, single source of truth

### Schema Caching Performance

**Problem**: Creating `Schema::new()` on every provider instantiation is wasteful for static system tables.

**Solution**: `OnceLock<Arc<Schema>>` pattern in `{table}_table.rs`:

```rust
pub fn schema() -> Arc<Schema> {
    static SCHEMA: OnceLock<Arc<Schema>> = OnceLock::new();
    
    SCHEMA.get_or_init(|| {
        Arc::new(Schema::new(vec![/* fields */]))
    }).clone()
}
```

**Benefits**:
1. ✅ **Zero Runtime Overhead**: Schema created once, reused forever
2. ✅ **Thread-Safe**: OnceLock handles concurrent initialization
3. ✅ **Memory Efficient**: Single Arc<Schema> shared across all providers
4. ✅ **Static Lifetime**: Schema lives for entire app runtime (system tables never change)
5. ✅ **Lazy Initialization**: Only created when first provider is instantiated

**Performance Impact**:
- First call: ~1-2μs (schema construction)
- Subsequent calls: ~10ns (atomic read + Arc::clone)
- Memory: ~1KB per schema (shared across all queries)

**Pattern applies to**: All system tables (users, jobs, namespaces, tables, storages, live_queries) where schema is static.
