//! Indexed Entity Store - Automatic secondary index management.
//!
//! This module provides `IndexedEntityStore<K, V>` which extends `EntityStore`
//! with automatic secondary index maintenance using RocksDB's atomic WriteBatch.
//!
//! ## Features
//!
//! - **Automatic Index Management**: Indexes are updated automatically on insert/update/delete
//! - **Atomic Operations**: Entity + all indexes updated in single WriteBatch
//! - **Async Support**: spawn_blocking wrappers for async contexts
//! - **DataFusion Integration**: Filter pushdown support via `IndexDefinition` trait
//!
//! ## Architecture
//!
//! ```text
//! IndexedEntityStore<K, V>
//!     │
//!     ├── insert(key, entity)
//!     │       │
//!     │       ▼
//!     │   backend.batch([
//!     │       Put { entity },
//!     │       Put { index1 },
//!     │       Put { index2 },
//!     │   ])
//!     │
//!     ├── update(key, entity)
//!     │       │
//!     │       ▼
//!     │   1. Fetch old entity
//!     │   2. backend.batch([
//!     │       Delete { old_index1 },  // if changed
//!     │       Delete { old_index2 },  // if changed
//!     │       Put { entity },
//!     │       Put { new_index1 },
//!     │       Put { new_index2 },
//!     │   ])
//!     │
//!     └── delete(key)
//!             │
//!             ▼
//!         1. Fetch entity
//!         2. backend.batch([
//!             Delete { entity },
//!             Delete { index1 },
//!             Delete { index2 },
//!         ])
//! ```
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_store::{IndexedEntityStore, IndexDefinition};
//! use kalamdb_commons::{JobId, Job, JobStatus};
//!
//! // Define an index
//! struct JobStatusIndex;
//!
//! impl IndexDefinition<JobId, Job> for JobStatusIndex {
//!     fn partition(&self) -> &str {
//!         "system_jobs_status_idx"
//!     }
//!
//!     fn indexed_columns(&self) -> Vec<&str> {
//!         vec!["status"]
//!     }
//!
//!     fn extract_key(&self, _pk: &JobId, job: &Job) -> Option<Vec<u8>> {
//!         let mut key = Vec::new();
//!         key.push(job.status as u8);
//!         key.extend_from_slice(&job.created_at.to_be_bytes());
//!         key.extend_from_slice(job.job_id.as_bytes());
//!         Some(key)
//!     }
//! }
//!
//! // Create store with indexes
//! let store = IndexedEntityStore::new(
//!     backend,
//!     "system_jobs",
//!     vec![Arc::new(JobStatusIndex)],
//! );
//!
//! // Insert - automatically updates indexes
//! store.insert(&job.job_id, &job)?;
//!
//! // Update - automatically removes old index entries, adds new ones
//! job.status = JobStatus::Completed;
//! store.update(&job.job_id, &job)?;
//!
//! // Delete - automatically removes all index entries
//! store.delete(&job.job_id)?;
//!
//! // Query by index
//! let running_jobs = store.scan_by_index(0, Some(&[JobStatus::Running as u8]), Some(10))?;
//! ```

use crate::entity_store::{EntityStore, KSerializable};
use crate::storage_trait::{Operation, Partition, Result, StorageBackend, StorageError};
use kalamdb_commons::StorageKey;
use std::sync::Arc;

#[cfg(feature = "datafusion")]
use datafusion::logical_expr::{Expr, Operator};
#[cfg(feature = "datafusion")]
use datafusion::scalar::ScalarValue;

// ============================================================================
// IndexDefinition Trait
// ============================================================================

/// Defines how to extract index keys from an entity.
///
/// Each index is defined by:
/// - A partition name where index entries are stored
/// - Column names covered by the index (for DataFusion filter pushdown)
/// - A function to extract the index key from the entity
/// - Optional: Custom index value (default is primary key for reverse lookup)
/// - Optional: Filter-to-prefix conversion for DataFusion
///
/// ## Index Key Design Guidelines
///
/// For range queries, design composite keys with most selective field first:
/// ```text
/// [status][created_at_be][job_id]
///    1B       8B          var
/// ```
///
/// - Use big-endian for numeric fields (ensures lexicographic = numeric order)
/// - Append primary key to ensure uniqueness
/// - Return `None` from `extract_key()` to skip indexing (e.g., conditional indexes)
pub trait IndexDefinition<K, V>: Send + Sync
where
    K: StorageKey,
    V: KSerializable,
{
    /// Returns the partition name for this index.
    ///
    /// This should be unique across all indexes in the system.
    /// Convention: `{main_partition}_idx_{columns}` e.g., `system_jobs_idx_status`
    fn partition(&self) -> &str;

    /// Returns the column names this index covers.
    ///
    /// Used by DataFusion to determine if a filter can use this index.
    /// Should match the order of fields in the index key.
    fn indexed_columns(&self) -> Vec<&str>;

    /// Extracts the index key from the entity.
    ///
    /// Returns `None` if this entity should not be indexed (e.g., conditional index).
    ///
    /// ## Key Design
    ///
    /// The returned bytes should be designed for efficient prefix scanning:
    /// - Put the most frequently filtered field first
    /// - Use big-endian encoding for numbers (preserves sort order)
    /// - Append primary key to ensure uniqueness
    fn extract_key(&self, primary_key: &K, entity: &V) -> Option<Vec<u8>>;

    /// Returns the value to store in the index.
    ///
    /// Default: Store the primary key bytes for reverse lookup.
    /// Override for covering indexes that include additional data.
    fn index_value(&self, primary_key: &K, _entity: &V) -> Vec<u8> {
        primary_key.storage_key()
    }

    /// Converts a DataFusion filter expression to an index scan prefix.
    ///
    /// Returns `Some(prefix)` if this index can satisfy the filter.
    /// Returns `None` if the filter cannot use this index.
    ///
    /// ## Example
    ///
    /// For a status index, convert `status = 'Running'` to prefix `[2]` (Running = 2).
    #[cfg(feature = "datafusion")]
    fn filter_to_prefix(&self, _filter: &Expr) -> Option<Vec<u8>> {
        None
    }

    /// Checks if this index can satisfy the given filter.
    ///
    /// Used by DataFusion's `supports_filters_pushdown()`.
    #[cfg(feature = "datafusion")]
    fn supports_filter(&self, filter: &Expr) -> bool {
        self.filter_to_prefix(filter).is_some()
    }
}

// ============================================================================
// IndexedEntityStore
// ============================================================================

/// An EntityStore that automatically manages secondary indexes.
///
/// All write operations (insert/update/delete) atomically update the entity
/// and all defined indexes using RocksDB's WriteBatch.
///
/// ## Type Parameters
///
/// - `K`: Primary key type (implements `StorageKey`)
/// - `V`: Entity value type (implements `KSerializable`)
///
/// ## Thread Safety
///
/// This struct is `Send + Sync` and can be safely shared across threads.
/// The underlying `StorageBackend` handles concurrent access.
pub struct IndexedEntityStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    backend: Arc<dyn StorageBackend>,
    partition: String,
    indexes: Vec<Arc<dyn IndexDefinition<K, V>>>,
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V> IndexedEntityStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    /// Creates a new IndexedEntityStore.
    ///
    /// # Arguments
    ///
    /// * `backend` - Storage backend (RocksDB or mock)
    /// * `partition` - Partition name for the main entity table
    /// * `indexes` - List of index definitions
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let store = IndexedEntityStore::new(
    ///     backend,
    ///     "system_jobs",
    ///     vec![Arc::new(JobStatusIndex)],
    /// );
    /// ```
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        partition: impl Into<String>,
        indexes: Vec<Arc<dyn IndexDefinition<K, V>>>,
    ) -> Self {
        let partition_str = partition.into();

        // Ensure main partition exists
        let main_partition = Partition::new(&partition_str);
        let _ = backend.create_partition(&main_partition); // Ignore error if already exists

        // Ensure all index partitions exist
        for index in &indexes {
            let index_partition = Partition::new(index.partition());
            let _ = backend.create_partition(&index_partition); // Ignore error if already exists
        }

        Self {
            backend,
            partition: partition_str,
            indexes,
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the index definitions.
    ///
    /// Useful for DataFusion integration to check which filters can be pushed down.
    pub fn indexes(&self) -> &[Arc<dyn IndexDefinition<K, V>>] {
        &self.indexes
    }

    /// Returns an index definition by index.
    pub fn get_index(&self, idx: usize) -> Option<&Arc<dyn IndexDefinition<K, V>>> {
        self.indexes.get(idx)
    }

    /// Finds the best index for a given filter.
    ///
    /// Returns `Some((index_idx, prefix))` if an index can satisfy the filter.
    #[cfg(feature = "datafusion")]
    pub fn find_index_for_filter(&self, filter: &Expr) -> Option<(usize, Vec<u8>)> {
        for (idx, index) in self.indexes.iter().enumerate() {
            if let Some(prefix) = index.filter_to_prefix(filter) {
                return Some((idx, prefix));
            }
        }
        None
    }

    // ========================================================================
    // Sync Write Operations (Atomic with Indexes)
    // ========================================================================

    /// Inserts a new entity with all indexes atomically.
    ///
    /// Uses RocksDB WriteBatch - all operations succeed or none are applied.
    ///
    /// # Arguments
    ///
    /// * `key` - Primary key for the entity
    /// * `entity` - Entity to insert
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Serialization fails
    /// - Storage backend fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// store.insert(&job.job_id, &job)?;
    /// // Entity and all indexes are now written atomically
    /// ```
    pub fn insert(&self, key: &K, entity: &V) -> Result<()> {
        let mut operations = Vec::with_capacity(1 + self.indexes.len());

        // 1. Main entity write
        let partition = Partition::new(&self.partition);
        let value = entity.encode()?;
        operations.push(Operation::Put {
            partition,
            key: key.storage_key(),
            value,
        });

        // 2. Index writes
        for index in &self.indexes {
            if let Some(index_key) = index.extract_key(key, entity) {
                let index_partition = Partition::new(index.partition());
                let index_value = index.index_value(key, entity);
                operations.push(Operation::Put {
                    partition: index_partition,
                    key: index_key,
                    value: index_value,
                });
            }
        }

        // Atomic batch write
        self.backend.batch(operations)
    }

    /// Inserts multiple entities with all indexes atomically in a single WriteBatch.
    ///
    /// This is significantly more efficient than calling `insert()` N times,
    /// as it performs a single disk write instead of N writes.
    ///
    /// # Arguments
    ///
    /// * `entries` - Vector of (key, entity) pairs to insert
    ///
    /// # Returns
    ///
    /// `Ok(())` if all entities were inserted successfully.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Serialization fails for any entity
    /// - Storage backend fails
    /// - The entire batch is rolled back on any failure (atomic)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let entries = vec![
    ///     (row_id1, entity1),
    ///     (row_id2, entity2),
    ///     (row_id3, entity3),
    /// ];
    /// store.insert_batch(&entries)?;  // Single atomic write for all
    /// ```
    pub fn insert_batch(&self, entries: &[(K, V)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        // Pre-allocate: each entry = 1 entity write + N index writes
        let ops_per_entry = 1 + self.indexes.len();
        let mut operations = Vec::with_capacity(entries.len() * ops_per_entry);

        let partition = Partition::new(&self.partition);

        for (key, entity) in entries {
            // 1. Main entity write
            let value = entity.encode()?;
            operations.push(Operation::Put {
                partition: partition.clone(),
                key: key.storage_key(),
                value,
            });

            // 2. Index writes for this entity
            for index in &self.indexes {
                if let Some(index_key) = index.extract_key(key, entity) {
                    let index_partition = Partition::new(index.partition());
                    let index_value = index.index_value(key, entity);
                    operations.push(Operation::Put {
                        partition: index_partition,
                        key: index_key,
                        value: index_value,
                    });
                }
            }
        }

        // Single atomic batch write for ALL entities
        self.backend.batch(operations)
    }

    /// Updates an entity and its indexes atomically.
    ///
    /// 1. Fetches old entity to determine stale index entries
    /// 2. Deletes old index entries (if keys changed)
    /// 3. Writes new entity
    /// 4. Writes new index entries
    ///
    /// All in a single atomic WriteBatch.
    ///
    /// # Arguments
    ///
    /// * `key` - Primary key for the entity
    /// * `new_entity` - Updated entity
    ///
    /// # Note
    ///
    /// This method fetches the old entity first to determine which index
    /// entries need to be deleted. If you already have the old entity,
    /// use `update_with_old()` for better performance.
    pub fn update(&self, key: &K, new_entity: &V) -> Result<()> {
        // Fetch old entity to determine which index entries to remove
        let old_entity = self.get(key)?;
        self.update_internal(key, old_entity.as_ref(), new_entity)
    }

    /// Updates an entity when you already have the old entity.
    ///
    /// More efficient than `update()` when you've already fetched the entity.
    pub fn update_with_old(&self, key: &K, old_entity: Option<&V>, new_entity: &V) -> Result<()> {
        self.update_internal(key, old_entity, new_entity)
    }

    fn update_internal(&self, key: &K, old_entity: Option<&V>, new_entity: &V) -> Result<()> {
        let mut operations = Vec::with_capacity(1 + self.indexes.len() * 2);

        // 1. Delete stale index entries (if entity existed and index key changed)
        if let Some(old) = old_entity {
            for index in &self.indexes {
                let old_index_key = index.extract_key(key, old);
                let new_index_key = index.extract_key(key, new_entity);

                // Only delete if index key changed
                if old_index_key != new_index_key {
                    if let Some(old_key) = old_index_key {
                        let index_partition = Partition::new(index.partition());
                        operations.push(Operation::Delete {
                            partition: index_partition,
                            key: old_key,
                        });
                    }
                }
            }
        }

        // 2. Write new entity
        let partition = Partition::new(&self.partition);
        let value = new_entity.encode()?;
        operations.push(Operation::Put {
            partition,
            key: key.storage_key(),
            value,
        });

        // 3. Write new index entries (only if changed or new entity)
        for index in &self.indexes {
            let new_index_key = index.extract_key(key, new_entity);
            let old_index_key = old_entity.and_then(|old| index.extract_key(key, old));

            // Only write if index key changed or entity is new
            if new_index_key != old_index_key {
                if let Some(idx_key) = new_index_key {
                    let index_partition = Partition::new(index.partition());
                    let index_value = index.index_value(key, new_entity);
                    operations.push(Operation::Put {
                        partition: index_partition,
                        key: idx_key,
                        value: index_value,
                    });
                }
            }
        }

        // Atomic batch write
        self.backend.batch(operations)
    }

    /// Deletes an entity and all its index entries atomically.
    ///
    /// # Note
    ///
    /// This method fetches the entity first to determine which index
    /// entries need to be deleted. If you already have the entity,
    /// use `delete_with_entity()` for better performance.
    pub fn delete(&self, key: &K) -> Result<()> {
        // Fetch entity to determine which index entries to remove
        let entity = match self.get(key)? {
            Some(e) => e,
            None => return Ok(()), // Already deleted
        };

        self.delete_with_entity(key, &entity)
    }

    /// Deletes an entity when you already have it.
    ///
    /// More efficient than `delete()` when you've already fetched the entity.
    pub fn delete_with_entity(&self, key: &K, entity: &V) -> Result<()> {
        let mut operations = Vec::with_capacity(1 + self.indexes.len());

        // 1. Delete main entity
        let partition = Partition::new(&self.partition);
        operations.push(Operation::Delete {
            partition,
            key: key.storage_key(),
        });

        // 2. Delete all index entries
        for index in &self.indexes {
            if let Some(index_key) = index.extract_key(key, entity) {
                let index_partition = Partition::new(index.partition());
                operations.push(Operation::Delete {
                    partition: index_partition,
                    key: index_key,
                });
            }
        }

        // Atomic batch write
        self.backend.batch(operations)
    }

    // ========================================================================
    // Sync Read/Scan Operations
    // ========================================================================

    /// Scans an index by prefix and returns matching entities.
    ///
    /// # Arguments
    ///
    /// * `index_idx` - Index number (0-based)
    /// * `prefix` - Optional prefix to filter by
    /// * `limit` - Optional limit on number of results
    ///
    /// # Returns
    ///
    /// Vector of (primary_key, entity) tuples.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get all Running jobs (assuming status=Running is encoded as byte 2)
    /// let running_jobs = store.scan_by_index(0, Some(&[2]), Some(100))?;
    /// ```
    pub fn scan_by_index(
        &self,
        index_idx: usize,
        prefix: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Vec<(K, V)>> {
        let index = self
            .indexes
            .get(index_idx)
            .ok_or_else(|| StorageError::Other(format!("Index {} not found", index_idx)))?;

        let index_partition = Partition::new(index.partition());
        let iter = self.backend.scan(&index_partition, prefix, None, limit)?;

        let mut results = Vec::new();
        for (_index_key, primary_key_bytes) in iter {
            // Deserialize primary key from index value
            let primary_key = K::from_storage_key(&primary_key_bytes)
                .map_err(StorageError::SerializationError)?;

            // Fetch actual entity
            if let Some(entity) = self.get(&primary_key)? {
                results.push((primary_key, entity));
            }
        }

        Ok(results)
    }

    /// Scans an index and returns only the primary keys (no entity fetch).
    ///
    /// More efficient when you only need the keys.
    pub fn scan_index_keys(
        &self,
        index_idx: usize,
        prefix: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Vec<K>> {
        let index = self
            .indexes
            .get(index_idx)
            .ok_or_else(|| StorageError::Other(format!("Index {} not found", index_idx)))?;

        let index_partition = Partition::new(index.partition());
        let iter = self.backend.scan(&index_partition, prefix, None, limit)?;

        let mut results = Vec::new();
        for (_index_key, primary_key_bytes) in iter {
            let primary_key = K::from_storage_key(&primary_key_bytes)
                .map_err(StorageError::SerializationError)?;
            results.push(primary_key);
        }

        Ok(results)
    }

    /// Checks if any entry exists in an index with the given prefix.
    ///
    /// This is the most efficient check - only scans index, no entity fetch,
    /// stops at first match. Use this for PK uniqueness validation.
    ///
    /// # Arguments
    ///
    /// * `index_idx` - Index number (0-based, typically 0 for PK index)
    /// * `prefix` - Prefix to search for
    ///
    /// # Returns
    ///
    /// `true` if at least one entry exists with this prefix
    pub fn exists_by_index(&self, index_idx: usize, prefix: &[u8]) -> Result<bool> {
        let index = self
            .indexes
            .get(index_idx)
            .ok_or_else(|| StorageError::Other(format!("Index {} not found", index_idx)))?;

        let index_partition = Partition::new(index.partition());
        // Only fetch 1 result - we just need to know if anything exists
        let iter = self.backend.scan(&index_partition, Some(prefix), None, Some(1))?;

        // If we got any result, the prefix exists
        Ok(iter.into_iter().next().is_some())
    }

    /// Scans an index returning raw (index_key, primary_key) pairs.
    ///
    /// Useful when you need access to the index key itself.
    pub fn scan_index_raw(
        &self,
        index_idx: usize,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let index = self
            .indexes
            .get(index_idx)
            .ok_or_else(|| StorageError::Other(format!("Index {} not found", index_idx)))?;

        let index_partition = Partition::new(index.partition());
        let iter = self.backend.scan(&index_partition, prefix, start_key, limit)?;

        Ok(iter.collect())
    }
}

// ============================================================================
// EntityStore Implementation
// ============================================================================

impl<K, V> EntityStore<K, V> for IndexedEntityStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    fn partition(&self) -> &str {
        &self.partition
    }
}

// ============================================================================
// Clone Implementation
// ============================================================================

impl<K, V> Clone for IndexedEntityStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            partition: self.partition.clone(),
            indexes: self.indexes.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

// ============================================================================
// Async Support
// ============================================================================

impl<K, V> IndexedEntityStore<K, V>
where
    K: StorageKey + Clone + Send + Sync + 'static,
    V: KSerializable + Clone + Send + Sync + 'static,
{
    /// Async version of `insert()`.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn insert_async(&self, key: K, entity: V) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.insert(&key, &entity))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }

    /// Async version of `update()`.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn update_async(&self, key: K, entity: V) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.update(&key, &entity))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }

    /// Async version of `delete()`.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn delete_async(&self, key: K) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.delete(&key))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }

    /// Async version of `scan_by_index()`.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn scan_by_index_async(
        &self,
        index_idx: usize,
        prefix: Option<Vec<u8>>,
        limit: Option<usize>,
    ) -> Result<Vec<(K, V)>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.scan_by_index(index_idx, prefix.as_deref(), limit))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }

    /// Async version of `get()` from EntityStore.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn get_async(&self, key: K) -> Result<Option<V>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.get(&key))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }

    /// Async version of `scan_all()` from EntityStore.
    ///
    /// Uses `spawn_blocking` to avoid blocking the async runtime.
    pub async fn scan_all_async(
        &self,
        limit: Option<usize>,
        prefix: Option<K>,
        start_key: Option<K>,
    ) -> Result<Vec<(Vec<u8>, V)>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            store.scan_all(limit, prefix.as_ref(), start_key.as_ref())
        })
        .await
        .map_err(|e| StorageError::Other(format!("spawn_blocking error: {}", e)))?
    }
}

// ============================================================================
// Helper: Extract equality filters for DataFusion integration
// ============================================================================

/// Helper function to extract column equality from a DataFusion Expr.
///
/// Returns `Some((column_name, string_value))` for expressions like `col = 'value'`.
#[cfg(feature = "datafusion")]
pub fn extract_string_equality(filter: &Expr) -> Option<(&str, &str)> {
    match filter {
        Expr::BinaryExpr(binary) => {
            if binary.op != Operator::Eq {
                return None;
            }

            match (binary.left.as_ref(), binary.right.as_ref()) {
                (Expr::Column(col), Expr::Literal(ScalarValue::Utf8(Some(val)), _)) => {
                    return Some((col.name.as_str(), val.as_str()));
                }
                (Expr::Literal(ScalarValue::Utf8(Some(val)), _), Expr::Column(col)) => {
                    return Some((col.name.as_str(), val.as_str()));
                }
                _ => {}
            }
            None
        }
        _ => None,
    }
}

/// Helper function to extract column equality with i64 from a DataFusion Expr.
#[cfg(feature = "datafusion")]
pub fn extract_i64_equality(filter: &Expr) -> Option<(&str, i64)> {
    match filter {
        Expr::BinaryExpr(binary) => {
            if binary.op != Operator::Eq {
                return None;
            }

            match (binary.left.as_ref(), binary.right.as_ref()) {
                (Expr::Column(col), Expr::Literal(ScalarValue::Int64(Some(val)), _)) => {
                    return Some((col.name.as_str(), *val));
                }
                (Expr::Literal(ScalarValue::Int64(Some(val)), _), Expr::Column(col)) => {
                    return Some((col.name.as_str(), *val));
                }
                _ => {}
            }
            None
        }
        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::InMemoryBackend;
    use kalamdb_commons::system::Job;
    use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId, NodeId};

    // Test index: Jobs by status
    struct TestStatusIndex;

    impl IndexDefinition<JobId, Job> for TestStatusIndex {
        fn partition(&self) -> &str {
            "test_jobs_status_idx"
        }

        fn indexed_columns(&self) -> Vec<&str> {
            vec!["status"]
        }

        fn extract_key(&self, _pk: &JobId, job: &Job) -> Option<Vec<u8>> {
            let status_byte = match job.status {
                JobStatus::New => 0u8,
                JobStatus::Queued => 1,
                JobStatus::Running => 2,
                JobStatus::Retrying => 3,
                JobStatus::Completed => 4,
                JobStatus::Failed => 5,
                JobStatus::Cancelled => 6,
            };
            let mut key = Vec::with_capacity(1 + 8 + job.job_id.as_bytes().len());
            key.push(status_byte);
            key.extend_from_slice(&job.created_at.to_be_bytes());
            key.extend_from_slice(job.job_id.as_bytes());
            Some(key)
        }
    }

    fn create_test_job(id: &str, status: JobStatus) -> Job {
        let now = chrono::Utc::now().timestamp_millis();
        Job {
            job_id: JobId::new(id),
            job_type: JobType::Flush,
            status,
            parameters: None,
            message: None,
            exception_trace: None,
            idempotency_key: None,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now,
            updated_at: now,
            started_at: if status != JobStatus::New && status != JobStatus::Queued {
                Some(now)
            } else {
                None
            },
            finished_at: if status == JobStatus::Completed
                || status == JobStatus::Failed
                || status == JobStatus::Cancelled
            {
                Some(now)
            } else {
                None
            },
            node_id: NodeId::from("test-node"),
            queue: None,
            priority: None,
        }
    }

    #[test]
    fn test_insert_creates_entity_and_index() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = IndexedEntityStore::new(
            backend.clone(),
            "test_jobs",
            vec![Arc::new(TestStatusIndex)],
        );

        let job = create_test_job("job1", JobStatus::Running);
        store.insert(&job.job_id, &job).unwrap();

        // Verify entity exists
        let retrieved = store.get(&job.job_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().job_id, job.job_id);

        // Verify index entry exists (status=Running=2)
        let running_jobs = store.scan_by_index(0, Some(&[2]), None).unwrap();
        assert_eq!(running_jobs.len(), 1);
        assert_eq!(running_jobs[0].0, job.job_id);
    }

    #[test]
    fn test_update_changes_index_entry() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = IndexedEntityStore::new(
            backend.clone(),
            "test_jobs",
            vec![Arc::new(TestStatusIndex)],
        );

        // Insert with Running status
        let mut job = create_test_job("job1", JobStatus::Running);
        store.insert(&job.job_id, &job).unwrap();

        // Update to Completed
        job.status = JobStatus::Completed;
        job.finished_at = Some(job.created_at + 1000);
        job.message = Some("Done".to_string());
        store.update(&job.job_id, &job).unwrap();

        // Verify old index entry removed (Running=2)
        let running_jobs = store.scan_by_index(0, Some(&[2]), None).unwrap();
        assert_eq!(running_jobs.len(), 0);

        // Verify new index entry exists (Completed=4)
        let completed_jobs = store.scan_by_index(0, Some(&[4]), None).unwrap();
        assert_eq!(completed_jobs.len(), 1);
        assert_eq!(completed_jobs[0].0, job.job_id);
    }

    #[test]
    fn test_delete_removes_entity_and_index() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = IndexedEntityStore::new(
            backend.clone(),
            "test_jobs",
            vec![Arc::new(TestStatusIndex)],
        );

        let job = create_test_job("job1", JobStatus::Running);
        store.insert(&job.job_id, &job).unwrap();

        // Delete
        store.delete(&job.job_id).unwrap();

        // Verify entity gone
        let retrieved = store.get(&job.job_id).unwrap();
        assert!(retrieved.is_none());

        // Verify index entry gone
        let running_jobs = store.scan_by_index(0, Some(&[2]), None).unwrap();
        assert_eq!(running_jobs.len(), 0);
    }

    #[test]
    fn test_scan_by_index_with_multiple_statuses() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = IndexedEntityStore::new(
            backend.clone(),
            "test_jobs",
            vec![Arc::new(TestStatusIndex)],
        );

        // Insert jobs with different statuses
        let job1 = create_test_job("job1", JobStatus::Running);
        let job2 = create_test_job("job2", JobStatus::Running);
        let job3 = create_test_job("job3", JobStatus::Completed);

        store.insert(&job1.job_id, &job1).unwrap();
        store.insert(&job2.job_id, &job2).unwrap();
        store.insert(&job3.job_id, &job3).unwrap();

        // Query Running jobs
        let running = store.scan_by_index(0, Some(&[2]), None).unwrap();
        assert_eq!(running.len(), 2);

        // Query Completed jobs
        let completed = store.scan_by_index(0, Some(&[4]), None).unwrap();
        assert_eq!(completed.len(), 1);

        // Query New jobs (none)
        let new_jobs = store.scan_by_index(0, Some(&[0]), None).unwrap();
        assert_eq!(new_jobs.len(), 0);
    }

    #[tokio::test]
    async fn test_async_operations() {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        let store = IndexedEntityStore::new(
            backend.clone(),
            "test_jobs",
            vec![Arc::new(TestStatusIndex)],
        );

        let job = create_test_job("job1", JobStatus::Running);
        let job_id = job.job_id.clone();

        // Async insert
        store.insert_async(job_id.clone(), job.clone()).await.unwrap();

        // Async get
        let retrieved = store.get_async(job_id.clone()).await.unwrap();
        assert!(retrieved.is_some());

        // Async scan
        let running = store.scan_by_index_async(0, Some(vec![2]), None).await.unwrap();
        assert_eq!(running.len(), 1);

        // Async delete
        store.delete_async(job_id.clone()).await.unwrap();

        let retrieved = store.get_async(job_id).await.unwrap();
        assert!(retrieved.is_none());
    }
}
