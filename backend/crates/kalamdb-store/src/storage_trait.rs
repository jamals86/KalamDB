//! Storage backend abstraction for pluggable storage implementations.
//!
//! This module provides a trait-based abstraction layer that allows KalamDB to support
//! multiple storage backends (RocksDB, Sled, Redis, in-memory, etc.) without changing
//! core logic.
//!
//! ## Architecture
//!
//! The abstraction uses a `StorageBackend` trait that defines common operations:
//! - get/put/delete for key-value access
//! - batch for atomic multi-operation transactions
//! - scan for range queries
//! - partition management (maps to column families in RocksDB, trees in Sled, etc.)
//!
//! ## Partition Model
//!
//! Since different backends have different concepts for data organization:
//! - **RocksDB**: Partition = Column Family
//! - **Sled**: Partition = Tree
//! - **Redis**: Partition = Key Prefix
//! - **In-Memory**: Partition = HashMap namespace
//!
//! We use a generic `Partition` abstraction that backends map to their native concepts.
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_store::storage_trait::{StorageBackend, Partition, Operation};
//!
//! fn store_user_data<S: StorageBackend>(backend: &S, user_id: &str, data: &[u8]) {
//!     let partition = Partition::new(format!("user_{}", user_id));
//!     backend.put(&partition, b"profile", data).expect("Failed to store");
//! }
//! ```
//!
//! ## Implementing a Custom Backend
//!
//! To implement a new storage backend:
//!
//! ```rust,ignore
//! use kalamdb_store::storage_trait::{StorageBackend, Partition, Operation};
//!
//! pub struct MyBackend {
//!     // Your backend's connection/state
//! }
//!
//! impl StorageBackend for MyBackend {
//!     fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
//!         // Implement key lookup in your backend
//!         todo!()
//!     }
//!     
//!     fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()> {
//!         // Implement key write in your backend
//!         todo!()
//!     }
//!     
//!     // ... implement other required methods
//! }
//! ```

use std::any::Any;
use std::fmt;

/// Result type for storage operations.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Errors that can occur during storage operations.
#[derive(Debug, Clone)]
pub enum StorageError {
    /// Partition (column family, tree, namespace) not found
    PartitionNotFound(String),

    /// Generic I/O error from underlying storage
    IoError(String),

    /// Serialization/deserialization error
    SerializationError(String),

    /// Operation not supported by this backend
    Unsupported(String),

    /// Unique constraint violation (for indexes)
    UniqueConstraintViolation(String),

    /// Lock poisoning error (internal concurrency issue)
    LockPoisoned(String),

    /// Other errors
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::PartitionNotFound(p) => write!(f, "Partition not found: {}", p),
            StorageError::IoError(msg) => write!(f, "I/O error: {}", msg),
            StorageError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            StorageError::Unsupported(msg) => write!(f, "Unsupported operation: {}", msg),
            StorageError::UniqueConstraintViolation(msg) => {
                write!(f, "Unique constraint violation: {}", msg)
            }
            StorageError::LockPoisoned(msg) => write!(f, "Lock poisoned: {}", msg),
            StorageError::Other(msg) => write!(f, "Storage error: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

/// Represents a logical partition of data within a storage backend.
///
/// Partitions provide a way to organize data into separate namespaces.
/// Different backends map partitions to their native concepts:
/// - RocksDB: Column Family
/// - Sled: Tree
/// - Redis: Key prefix
/// - In-memory: HashMap namespace
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Partition {
    name: String,
}

impl Partition {
    /// Creates a new partition with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Returns the partition name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl From<String> for Partition {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl From<&str> for Partition {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<kalamdb_commons::storage::Partition> for Partition {
    fn from(p: kalamdb_commons::storage::Partition) -> Self {
        Self::new(p.name().to_owned())
    }
}

impl From<&kalamdb_commons::storage::Partition> for Partition {
    fn from(p: &kalamdb_commons::storage::Partition) -> Self {
        Self::new(p.name())
    }
}

/// Represents a single operation in a batch transaction.
///
/// Used with `StorageBackend::batch()` for atomic multi-operation transactions.
#[derive(Debug, Clone)]
pub enum Operation {
    /// Insert or update a key-value pair
    Put {
        partition: Partition,
        key: Vec<u8>,
        value: Vec<u8>,
    },

    /// Delete a key
    Delete { partition: Partition, key: Vec<u8> },
}

/// Trait for pluggable storage backend implementations.
///
/// Implementations must be thread-safe (Send + Sync) to allow concurrent access.
///
/// ## Performance Considerations
///
/// - `get` operations should be fast (typically <1ms)
/// - `put` operations may be buffered (check backend documentation)
/// - `batch` operations should be atomic (all-or-nothing)
/// - `scan` operations should return an iterator for memory efficiency
///
/// ## Error Handling
///
/// Implementations should:
/// - Return `PartitionNotFound` if partition doesn't exist
/// - Return `IoError` for underlying storage failures
/// - Return `Unsupported` for operations not supported by the backend
pub trait StorageBackend: Send + Sync {
    /// Retrieves a value by key from the specified partition.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Stores a key-value pair in the specified partition.
    ///
    /// If the key already exists, its value is updated.
    fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()>;

    /// Deletes a key from the specified partition.
    ///
    /// Returns `Ok(())` even if the key doesn't exist (idempotent).
    fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()>;

    /// Executes multiple operations atomically in a batch.
    ///
    /// Either all operations succeed or none are applied.
    fn batch(&self, operations: Vec<Operation>) -> Result<()>;

    /// Scans keys in a partition, optionally filtered by prefix and limit.
    ///
    /// Returns an iterator of (key, value) pairs. The iterator should be
    /// memory-efficient (not loading all data at once).
    ///
    /// ## Parameters
    /// - `prefix`: If Some, only return keys starting with this prefix
    /// - `start_key`: If Some, start scanning from this key (inclusive). Must be >= prefix if both are set.
    /// - `limit`: If Some, return at most this many entries
    fn scan(
        &self,
        partition: &Partition,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<kalamdb_commons::storage::KvIterator<'_>>;

    /// Checks if a partition exists.
    fn partition_exists(&self, partition: &Partition) -> bool;

    /// Creates a new partition.
    ///
    /// Returns `Ok(())` if the partition already exists (idempotent).
    fn create_partition(&self, partition: &Partition) -> Result<()>;

    /// Lists all partitions in the storage backend.
    fn list_partitions(&self) -> Result<Vec<Partition>>;

    /// Deletes a partition and all its data.
    ///
    /// **Warning**: This is a destructive operation and cannot be undone.
    fn drop_partition(&self, partition: &Partition) -> Result<()>;

    /// Downcast support to enable integration paths that need concrete backends.
    ///
    /// This should be used sparingly; prefer the trait methods above. It exists
    /// to help legacy components that still require a concrete backend handle.
    fn as_any(&self) -> &dyn Any;
}

/// Extension trait providing async versions of StorageBackend methods.
///
/// These methods internally use `tokio::task::spawn_blocking` to offload
/// synchronous storage operations to a blocking thread pool, preventing
/// the async runtime from being blocked.
///
/// ## Usage
///
/// ```rust,ignore
/// use kalamdb_store::storage_trait::{StorageBackend, StorageBackendAsync, Partition};
/// use std::sync::Arc;
///
/// async fn store_data(backend: Arc<dyn StorageBackend>, partition: &Partition) {
///     // Use async variants in async contexts:
///     let value = backend.get_async(partition, b"key").await.unwrap();
///     backend.put_async(partition, b"key", b"value").await.unwrap();
/// }
/// ```
#[async_trait::async_trait]
pub trait StorageBackendAsync: Send + Sync {
    /// Async version of `get()` - retrieves a value by key.
    async fn get_async(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Async version of `put()` - stores a key-value pair.
    async fn put_async(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()>;

    /// Async version of `delete()` - removes a key.
    async fn delete_async(&self, partition: &Partition, key: &[u8]) -> Result<()>;

    /// Async version of `batch()` - executes multiple operations atomically.
    async fn batch_async(&self, operations: Vec<Operation>) -> Result<()>;

    /// Async version of `scan()` - scans keys in a partition.
    /// Returns collected results since iterators can't cross spawn_blocking boundary.
    async fn scan_async(
        &self,
        partition: &Partition,
        prefix: Option<Vec<u8>>,
        start_key: Option<Vec<u8>>,
        limit: Option<usize>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

// Blanket implementation for Arc<dyn StorageBackend>
#[async_trait::async_trait]
impl StorageBackendAsync for std::sync::Arc<dyn StorageBackend> {
    async fn get_async(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let backend = self.clone();
        let partition = partition.clone();
        let key = key.to_vec();
        tokio::task::spawn_blocking(move || backend.get(&partition, &key))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn put_async(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()> {
        let backend = self.clone();
        let partition = partition.clone();
        let key = key.to_vec();
        let value = value.to_vec();
        tokio::task::spawn_blocking(move || backend.put(&partition, &key, &value))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn delete_async(&self, partition: &Partition, key: &[u8]) -> Result<()> {
        let backend = self.clone();
        let partition = partition.clone();
        let key = key.to_vec();
        tokio::task::spawn_blocking(move || backend.delete(&partition, &key))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn batch_async(&self, operations: Vec<Operation>) -> Result<()> {
        let backend = self.clone();
        tokio::task::spawn_blocking(move || backend.batch(operations))
            .await
            .map_err(|e| StorageError::Other(format!("spawn_blocking join error: {}", e)))?
    }

    async fn scan_async(
        &self,
        partition: &Partition,
        prefix: Option<Vec<u8>>,
        start_key: Option<Vec<u8>>,
        limit: Option<usize>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let backend = self.clone();
        let partition = partition.clone();
        tokio::task::spawn_blocking(move || {
            let iter = backend.scan(
                &partition,
                prefix.as_deref(),
                start_key.as_deref(),
                limit,
            )?;
            Ok(iter.collect())
        })
        .await
        .map_err(|e| StorageError::Other(format!("spawn_blocking join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_creation() {
        let p1 = Partition::new("users");
        assert_eq!(p1.name(), "users");

        let p2 = Partition::from("tables");
        assert_eq!(p2.name(), "tables");
    }

    #[test]
    fn test_operation_construction() {
        let op = Operation::Put {
            partition: Partition::new("test"),
            key: b"key1".to_vec(),
            value: b"value1".to_vec(),
        };

        match op {
            Operation::Put {
                partition,
                key,
                value,
            } => {
                assert_eq!(partition.name(), "test");
                assert_eq!(key, b"key1");
                assert_eq!(value, b"value1");
            }
            _ => panic!("Wrong operation type"),
        }
    }

    #[test]
    fn test_error_display() {
        let err = StorageError::PartitionNotFound("users".to_string());
        assert_eq!(err.to_string(), "Partition not found: users");

        let err = StorageError::IoError("disk full".to_string());
        assert_eq!(err.to_string(), "I/O error: disk full");
    }
}
