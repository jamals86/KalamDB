# Storage Backend Trait Interface Contract

**Version**: 1.0.0  
**Package**: kalamdb-store  
**Language**: Rust

## Purpose

This document defines the storage backend trait interface that abstracts storage operations to support pluggable backends (RocksDB, Sled, Redis, custom implementations). The trait provides a consistent API for key-value operations with partition support.

---

## Core Trait

### StorageBackend

**Purpose**: Trait defining storage operations that can be implemented by different storage engines.

```rust
/// Storage backend abstraction for KalamDB
///
/// Provides a unified interface for key-value storage operations with partition support.
/// Partitions map to column families in RocksDB or key prefixes in simple KV stores.
///
/// All methods are async to support both local (RocksDB) and network-based (Redis) backends.
///
/// # Examples
///
/// ## Implementing a custom backend
///
/// ```rust
/// use kalamdb_store::{StorageBackend, Partition, Operation, Result};
///
/// pub struct MyCustomBackend {
///     // Implementation-specific fields
/// }
///
/// #[async_trait::async_trait]
/// impl StorageBackend for MyCustomBackend {
///     async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
///         // Implementation
///     }
///     
///     // ... other methods
/// }
/// ```
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get a value by key from a specific partition
    ///
    /// # Arguments
    /// * `partition` - Storage partition (column family in RocksDB, key prefix in others)
    /// * `key` - Key bytes to retrieve
    ///
    /// # Returns
    /// * `Some(value)` if key exists
    /// * `None` if key does not exist
    ///
    /// # Errors
    /// * Storage engine errors (I/O, corruption, etc.)
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>>;
    
    /// Put a key-value pair in a specific partition
    ///
    /// Overwrites existing value if key already exists.
    ///
    /// # Arguments
    /// * `partition` - Storage partition
    /// * `key` - Key bytes
    /// * `value` - Value bytes
    ///
    /// # Errors
    /// * Storage engine errors (I/O, disk full, etc.)
    async fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()>;
    
    /// Delete a key from a specific partition
    ///
    /// No-op if key does not exist.
    ///
    /// # Arguments
    /// * `partition` - Storage partition
    /// * `key` - Key bytes to delete
    ///
    /// # Errors
    /// * Storage engine errors
    async fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()>;
    
    /// Scan a range of keys in a partition
    ///
    /// Returns an iterator over key-value pairs in the specified range.
    /// Range is inclusive of start_key and exclusive of end_key.
    ///
    /// # Arguments
    /// * `partition` - Storage partition
    /// * `start_key` - Starting key (None = beginning)
    /// * `end_key` - Ending key (None = end)
    ///
    /// # Returns
    /// Iterator over (key, value) pairs
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Scan all keys with prefix "user:123:"
    /// let iter = backend.scan(
    ///     &partition,
    ///     Some(b"user:123:"),
    ///     Some(b"user:123;\xFF"),  // Just after "user:123:"
    /// ).await?;
    ///
    /// for result in iter {
    ///     let (key, value) = result?;
    ///     println!("Key: {:?}, Value: {:?}", key, value);
    /// }
    /// ```
    async fn scan(
        &self,
        partition: &Partition,
        start_key: Option<&[u8]>,
        end_key: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>>;
    
    /// Execute atomic batch operations
    ///
    /// All operations succeed or all fail (transactional semantics).
    /// Operations are executed in the order provided.
    ///
    /// # Arguments
    /// * `operations` - Vector of Put/Delete operations
    ///
    /// # Errors
    /// * Storage engine errors
    /// * If any operation fails, entire batch is rolled back
    ///
    /// # Examples
    ///
    /// ```rust
    /// backend.batch(vec![
    ///     Operation::Put {
    ///         partition: partition.clone(),
    ///         key: b"key1".to_vec(),
    ///         value: b"value1".to_vec(),
    ///     },
    ///     Operation::Delete {
    ///         partition: partition.clone(),
    ///         key: b"key2".to_vec(),
    ///     },
    /// ]).await?;
    /// ```
    async fn batch(&self, operations: Vec<Operation>) -> Result<()>;
    
    /// Create a partition if it doesn't exist
    ///
    /// For RocksDB: Creates column family
    /// For simple KV stores: No-op (partitions are virtual via key prefixes)
    ///
    /// # Arguments
    /// * `partition` - Partition to create
    ///
    /// # Errors
    /// * Storage engine errors
    /// * Partition already exists (backend-specific behavior)
    async fn create_partition(&self, partition: &Partition) -> Result<()>;
    
    /// Check if partition exists
    ///
    /// # Arguments
    /// * `partition` - Partition to check
    ///
    /// # Returns
    /// * `true` if partition exists
    /// * `false` if partition does not exist
    async fn partition_exists(&self, partition: &Partition) -> Result<bool>;
    
    /// List all partitions
    ///
    /// Returns names of all existing partitions.
    async fn list_partitions(&self) -> Result<Vec<Partition>>;
    
    /// Drop a partition
    ///
    /// Deletes all data in the partition and removes the partition itself.
    ///
    /// # Arguments
    /// * `partition` - Partition to drop
    ///
    /// # Errors
    /// * Storage engine errors
    /// * Partition does not exist
    async fn drop_partition(&self, partition: &Partition) -> Result<()>;
    
    /// Get approximate size of a partition in bytes
    ///
    /// Used for monitoring and capacity planning.
    /// May not be exact depending on backend capabilities.
    ///
    /// # Arguments
    /// * `partition` - Partition to measure
    ///
    /// # Returns
    /// Approximate size in bytes
    async fn partition_size(&self, partition: &Partition) -> Result<u64>;
    
    /// Compact a partition
    ///
    /// Triggers compaction to optimize storage layout and reclaim space.
    /// Blocking operation for some backends (RocksDB).
    ///
    /// # Arguments
    /// * `partition` - Partition to compact
    ///
    /// # Errors
    /// * Storage engine errors
    /// * Operation not supported by backend
    async fn compact(&self, partition: &Partition) -> Result<()>;
}
```

---

## Supporting Types

### Partition

```rust
/// Storage partition identifier
///
/// Maps to column families in RocksDB or key prefixes in simple KV stores.
///
/// # Examples
///
/// ```rust
/// let partition = Partition::new("user_messages");
/// let partition = Partition::system("metadata");
/// ```
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Partition {
    name: String,
}

impl Partition {
    /// Create a new partition
    ///
    /// # Arguments
    /// * `name` - Partition name (alphanumeric + underscore only)
    ///
    /// # Errors
    /// * Invalid partition name format
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        Self::validate(&name)?;
        Ok(Partition { name })
    }
    
    /// Create a system partition
    ///
    /// System partitions use "sys_" prefix.
    pub fn system(name: impl Into<String>) -> Result<Self> {
        let name = format!("sys_{}", name.into());
        Self::new(name)
    }
    
    /// Get partition name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Validate partition name
    fn validate(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::InvalidPartitionName("empty".to_string()));
        }
        if name.len() > 64 {
            return Err(Error::InvalidPartitionName("too long".to_string()));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::InvalidPartitionName("invalid characters".to_string()));
        }
        Ok(())
    }
}

impl fmt::Display for Partition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}
```

---

### Operation

```rust
/// Batch operation type
#[derive(Debug, Clone)]
pub enum Operation {
    /// Put key-value pair
    Put {
        partition: Partition,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    
    /// Delete key
    Delete {
        partition: Partition,
        key: Vec<u8>,
    },
}

impl Operation {
    /// Create Put operation
    pub fn put(partition: Partition, key: Vec<u8>, value: Vec<u8>) -> Self {
        Operation::Put { partition, key, value }
    }
    
    /// Create Delete operation
    pub fn delete(partition: Partition, key: Vec<u8>) -> Self {
        Operation::Delete { partition, key }
    }
}
```

---

### Error

```rust
/// Storage backend errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Partition not found: {0}")]
    PartitionNotFound(String),
    
    #[error("Partition already exists: {0}")]
    PartitionAlreadyExists(String),
    
    #[error("Invalid partition name: {0}")]
    InvalidPartitionName(String),
    
    #[error("Key not found")]
    KeyNotFound,
    
    #[error("Batch operation failed: {0}")]
    BatchFailed(String),
    
    #[error("Operation not supported: {0}")]
    NotSupported(String),
    
    #[error("Backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

---

## Backend Implementations

### RocksDB Backend

```rust
/// RocksDB storage backend
///
/// Uses column families for partitions.
pub struct RocksDbBackend {
    db: Arc<rocksdb::DB>,
    options: RocksDbOptions,
}

impl RocksDbBackend {
    /// Create new RocksDB backend
    ///
    /// # Arguments
    /// * `path` - Database directory path
    /// * `options` - RocksDB configuration options
    ///
    /// # Errors
    /// * Failed to open database
    pub fn new(path: impl AsRef<Path>, options: RocksDbOptions) -> Result<Self> {
        let mut db_opts = rocksdb::Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        
        let db = rocksdb::DB::open(&db_opts, path)?;
        
        Ok(RocksDbBackend {
            db: Arc::new(db),
            options,
        })
    }
}

#[async_trait::async_trait]
impl StorageBackend for RocksDbBackend {
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        Ok(self.db.get_cf(cf, key)?)
    }
    
    async fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        self.db.put_cf(cf, key, value)?;
        Ok(())
    }
    
    async fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        self.db.delete_cf(cf, key)?;
        Ok(())
    }
    
    async fn scan(
        &self,
        partition: &Partition,
        start_key: Option<&[u8]>,
        end_key: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        let mut iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        
        if let Some(start) = start_key {
            iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::From(start, rocksdb::Direction::Forward));
        }
        
        let end_key = end_key.map(|k| k.to_vec());
        
        Ok(Box::new(iter.map(move |result| {
            result
                .map(|(k, v)| (k.to_vec(), v.to_vec()))
                .map_err(Error::from)
        }).take_while(move |result| {
            if let Ok((key, _)) = result {
                if let Some(ref end) = end_key {
                    &key[..] < &end[..]
                } else {
                    true
                }
            } else {
                true
            }
        })))
    }
    
    async fn batch(&self, operations: Vec<Operation>) -> Result<()> {
        let mut batch = rocksdb::WriteBatch::default();
        
        for op in operations {
            match op {
                Operation::Put { partition, key, value } => {
                    let cf = self.db
                        .cf_handle(partition.name())
                        .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
                    batch.put_cf(cf, key, value);
                }
                Operation::Delete { partition, key } => {
                    let cf = self.db
                        .cf_handle(partition.name())
                        .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
                    batch.delete_cf(cf, key);
                }
            }
        }
        
        self.db.write(batch)?;
        Ok(())
    }
    
    async fn create_partition(&self, partition: &Partition) -> Result<()> {
        let mut opts = rocksdb::Options::default();
        self.db.create_cf(partition.name(), &opts)?;
        Ok(())
    }
    
    async fn partition_exists(&self, partition: &Partition) -> Result<bool> {
        Ok(self.db.cf_handle(partition.name()).is_some())
    }
    
    async fn list_partitions(&self) -> Result<Vec<Partition>> {
        let names = self.db.cf_names();
        names.into_iter()
            .filter(|name| name != "default")  // Skip default CF
            .map(|name| Partition::new(name))
            .collect()
    }
    
    async fn drop_partition(&self, partition: &Partition) -> Result<()> {
        self.db.drop_cf(partition.name())?;
        Ok(())
    }
    
    async fn partition_size(&self, partition: &Partition) -> Result<u64> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        // Use RocksDB property to get approximate size
        let prop = self.db.property_int_value_cf(cf, "rocksdb.estimate-live-data-size")?;
        Ok(prop.unwrap_or(0))
    }
    
    async fn compact(&self, partition: &Partition) -> Result<()> {
        let cf = self.db
            .cf_handle(partition.name())
            .ok_or_else(|| Error::PartitionNotFound(partition.name().to_string()))?;
        
        self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        Ok(())
    }
}
```

---

### Sled Backend (Example)

```rust
/// Sled storage backend (simple KV store)
///
/// Uses key prefixes for partitions since Sled doesn't have column families.
pub struct SledBackend {
    db: sled::Db,
}

impl SledBackend {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(SledBackend { db })
    }
    
    /// Create prefixed key (partition:key)
    fn make_key(&self, partition: &Partition, key: &[u8]) -> Vec<u8> {
        let prefix = partition.name().as_bytes();
        let mut result = Vec::with_capacity(prefix.len() + 1 + key.len());
        result.extend_from_slice(prefix);
        result.push(b':');
        result.extend_from_slice(key);
        result
    }
}

#[async_trait::async_trait]
impl StorageBackend for SledBackend {
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let prefixed_key = self.make_key(partition, key);
        Ok(self.db.get(prefixed_key)?.map(|v| v.to_vec()))
    }
    
    async fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()> {
        let prefixed_key = self.make_key(partition, key);
        self.db.insert(prefixed_key, value)?;
        Ok(())
    }
    
    async fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()> {
        let prefixed_key = self.make_key(partition, key);
        self.db.remove(prefixed_key)?;
        Ok(())
    }
    
    // ... other methods with key prefixing
}
```

---

## Usage Examples

### Basic Operations

```rust
use kalamdb_store::{RocksDbBackend, StorageBackend, Partition, Operation};

#[tokio::main]
async fn main() -> Result<()> {
    let backend = RocksDbBackend::new("./data/storage", Default::default())?;
    
    let partition = Partition::new("messages")?;
    backend.create_partition(&partition).await?;
    
    // Put
    backend.put(&partition, b"msg:123", b"Hello, world!").await?;
    
    // Get
    let value = backend.get(&partition, b"msg:123").await?;
    assert_eq!(value, Some(b"Hello, world!".to_vec()));
    
    // Delete
    backend.delete(&partition, b"msg:123").await?;
    
    Ok(())
}
```

### Batch Operations

```rust
backend.batch(vec![
    Operation::put(partition.clone(), b"key1".to_vec(), b"value1".to_vec()),
    Operation::put(partition.clone(), b"key2".to_vec(), b"value2".to_vec()),
    Operation::delete(partition.clone(), b"key3".to_vec()),
]).await?;
```

### Scanning

```rust
let iter = backend.scan(
    &partition,
    Some(b"user:jamal:"),
    Some(b"user:jamal;\xFF"),
).await?;

for result in iter {
    let (key, value) = result?;
    println!("Key: {:?}, Value: {:?}", key, value);
}
```

---

## Migration Strategy

### Step 1: Implement trait for RocksDB
- Create `RocksDbBackend` struct
- Implement `StorageBackend` trait
- Keep existing RocksDB code for compatibility

### Step 2: Update kalamdb-store public API
- Export `StorageBackend` trait
- Export `Partition`, `Operation`, `Error` types
- Provide `RocksDbBackend` as default implementation

### Step 3: Migrate kalamdb-sql
- Replace direct RocksDB calls with `StorageBackend` trait methods
- Use dependency injection to pass backend instance
- Remove RocksDB dependency from kalamdb-sql

### Step 4: Update kalamdb-core
- Use `StorageBackend` trait in system table providers
- Remove direct RocksDB dependencies

### Step 5: Test alternative backends
- Implement `SledBackend` as proof of concept
- Run full test suite against both RocksDB and Sled
- Document backend-specific behavior differences

---

## Testing Strategy

### Unit Tests
- Partition validation
- Key prefixing (for Sled backend)
- Operation construction

### Integration Tests
- Basic CRUD operations (get, put, delete)
- Scan with various range combinations
- Batch operations (atomic semantics)
- Partition management (create, drop, exists)
- Error handling (partition not found, key not found)
- Concurrent access (multiple threads)

### Backend Compatibility Tests
- Run same test suite against multiple backends
- Verify consistent behavior across backends
- Document known differences (e.g., RocksDB compaction vs Sled)

---

## Performance Considerations

### RocksDB Backend
- Column families enable efficient isolation
- Native iterator support for scans
- Bloom filters for fast key lookups
- Compaction reduces storage size

### Sled Backend
- Key prefixing adds overhead (~50 bytes per key)
- Single tree means no true partition isolation
- Suitable for embedded use cases

### Redis Backend (Future)
- Network latency for all operations
- Key prefixing required
- Scan may be expensive for large keyspaces

---

## Backward Compatibility

**Migration Path**: Existing RocksDB code continues to work. New code uses `StorageBackend` trait. Gradual migration over multiple releases.

**Configuration**: No configuration changes required. Backend selection via compile-time feature flags or runtime configuration.

**Data Migration**: Not required. Existing RocksDB data files work with `RocksDbBackend`.
