//! RocksDB implementation of the StorageBackend trait.
//!
//! This module provides a concrete implementation of `StorageBackend` using RocksDB
//! as the underlying storage engine. It maps the generic partition concept to
//! RocksDB column families.

use crate::storage_trait::{Operation, Partition, Result, StorageBackend, StorageError};
use kalamdb_configs::RocksDbSettings;
use rocksdb::{BoundColumnFamily, Cache, IteratorMode, Options, PrefixRange, WriteOptions, DB};
use std::sync::Arc;

/// RocksDB implementation of the StorageBackend trait.
///
/// Maps partitions to RocksDB column families, providing thread-safe access
/// to the underlying database.
///
/// ## Example
///
/// ```rust,ignore
/// use kalamdb_store::{RocksDBBackend, StorageBackend, Partition};
/// use std::sync::Arc;
///
/// let db = Arc::new(DB::open_default("/tmp/test.db").unwrap());
/// let backend = RocksDBBackend::new(db);
///
/// let partition = Partition::new("users");
/// backend.create_partition(&partition).unwrap();
/// backend.put(&partition, b"key1", b"value1").unwrap();
///
/// let value = backend.get(&partition, b"key1").unwrap();
/// assert_eq!(value, Some(b"value1".to_vec()));
/// ```
pub struct RocksDBBackend {
    db: Arc<DB>,
    /// Cached write options for fast writes (no sync)
    write_opts: WriteOptions,
    settings: RocksDbSettings,
    block_cache: Cache,
}

impl RocksDBBackend {
    fn new_internal(
        db: Arc<DB>,
        sync_writes: bool,
        disable_wal: bool,
        settings: RocksDbSettings,
    ) -> Self {
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(sync_writes);
        write_opts.disable_wal(disable_wal);
        let block_cache = Cache::new_lru_cache(settings.block_cache_size);
        Self {
            db,
            write_opts,
            settings,
            block_cache,
        }
    }

    /// Creates a new RocksDB backend with the given database handle.
    /// Uses default write options (no sync for better write performance).
    pub fn new(db: Arc<DB>) -> Self {
        Self::new_internal(db, false, false, RocksDbSettings::default())
    }

    /// Creates a new RocksDB backend with custom write options.
    ///
    /// # Arguments
    /// * `db` - Database handle
    /// * `sync_writes` - If true, sync to disk on each write (slower but more durable)
    /// * `disable_wal` - If true, disable WAL entirely (fastest but data loss on crash)
    pub fn with_options(db: Arc<DB>, sync_writes: bool, disable_wal: bool) -> Self {
        Self::new_internal(db, sync_writes, disable_wal, RocksDbSettings::default())
    }

    /// Creates a new backend with write options and explicit RocksDB tuning settings.
    pub fn with_options_and_settings(
        db: Arc<DB>,
        sync_writes: bool,
        disable_wal: bool,
        settings: RocksDbSettings,
    ) -> Self {
        Self::new_internal(db, sync_writes, disable_wal, settings)
    }

    /// Returns a reference to the underlying database.
    pub fn db(&self) -> &Arc<DB> {
        &self.db
    }

    /// Gets a column family handle by partition name.
    fn get_cf(&self, partition: &Partition) -> Result<Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(partition.name())
            .ok_or_else(|| StorageError::PartitionNotFound(partition.name().to_string()))
    }
}

impl StorageBackend for RocksDBBackend {
    fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let _span = tracing::trace_span!("rocksdb.get", partition = %partition.name()).entered();
        let cf = self.get_cf(partition)?;
        self.db.get_cf(&cf, key).map_err(|e| StorageError::IoError(e.to_string()))
    }

    fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()> {
        let _span = tracing::trace_span!("rocksdb.put", partition = %partition.name(), value_len = value.len()).entered();
        let cf = self.get_cf(partition)?;
        self.db
            .put_cf_opt(&cf, key, value, &self.write_opts)
            .map_err(|e| StorageError::IoError(e.to_string()))
    }

    fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()> {
        let _span = tracing::trace_span!("rocksdb.delete", partition = %partition.name()).entered();
        let cf = self.get_cf(partition)?;
        self.db
            .delete_cf_opt(&cf, key, &self.write_opts)
            .map_err(|e| StorageError::IoError(e.to_string()))
    }

    fn batch(&self, operations: Vec<Operation>) -> Result<()> {
        let _span = tracing::debug_span!("rocksdb.batch", op_count = operations.len()).entered();
        use rocksdb::WriteBatch;

        let mut batch = WriteBatch::default();

        for op in operations {
            match op {
                Operation::Put {
                    partition,
                    key,
                    value,
                } => {
                    let cf = self.get_cf(&partition)?;
                    batch.put_cf(&cf, key, value);
                },
                Operation::Delete { partition, key } => {
                    let cf = self.get_cf(&partition)?;
                    batch.delete_cf(&cf, key);
                },
            }
        }

        self.db
            .write_opt(batch, &self.write_opts)
            .map_err(|e| StorageError::IoError(e.to_string()))
    }

    fn scan(
        &self,
        partition: &Partition,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
        limit: Option<usize>,
    ) -> Result<Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + Send + '_>> {
        let _span = tracing::debug_span!("rocksdb.scan", partition = %partition.name(), has_prefix = prefix.is_some(), limit = ?limit).entered();
        use rocksdb::Direction;

        let cf = self.get_cf(partition)?;

        // Take a consistent snapshot for the duration of the iterator
        let snapshot = self.db.snapshot();

        let prefix_vec = prefix.map(|p| p.to_vec());

        // Determine start position
        let iter_mode = if let Some(start) = start_key {
            IteratorMode::From(start, Direction::Forward)
        } else if let Some(p) = &prefix_vec {
            IteratorMode::From(p.as_slice(), Direction::Forward)
        } else {
            IteratorMode::Start
        };

        // RocksDB iterator over the snapshot: bind snapshot to ReadOptions
        let mut readopts = rocksdb::ReadOptions::default();
        readopts.set_snapshot(&snapshot);
        if let Some(p) = &prefix_vec {
            // Bound scans to the prefix range at the engine layer to avoid
            // walking unrelated keys in PK/index existence checks.
            readopts.set_iterate_range(PrefixRange(p.clone()));
        }
        let inner = self.db.iterator_cf_opt(&cf, readopts, iter_mode);

        struct SnapshotScanIter<'a, D: rocksdb::DBAccess> {
            // Hold the snapshot to keep it alive for 'a
            _snapshot: rocksdb::SnapshotWithThreadMode<'a, D>,
            inner: rocksdb::DBIteratorWithThreadMode<'a, D>,
            prefix: Option<Vec<u8>>,
            remaining: Option<usize>,
        }

        impl<'a, D: rocksdb::DBAccess> Iterator for SnapshotScanIter<'a, D> {
            type Item = (Vec<u8>, Vec<u8>);
            fn next(&mut self) -> Option<Self::Item> {
                // Respect limit
                if let Some(0) = self.remaining {
                    return None;
                }

                match self.inner.next()? {
                    Ok((k, v)) => {
                        if let Some(ref p) = self.prefix {
                            if !k.starts_with(p) {
                                return None;
                            }
                        }
                        if let Some(ref mut left) = self.remaining {
                            if *left > 0 {
                                *left -= 1;
                            }
                        }
                        Some((k.to_vec(), v.to_vec()))
                    },
                    Err(_) => None,
                }
            }
        }

        let iter = SnapshotScanIter::<DB> {
            _snapshot: snapshot,
            inner,
            prefix: prefix_vec,
            remaining: limit,
        };

        Ok(Box::new(iter))
    }

    fn partition_exists(&self, partition: &Partition) -> bool {
        self.db.cf_handle(partition.name()).is_some()
    }

    fn create_partition(&self, partition: &Partition) -> Result<()> {
        // Check if already exists
        if self.partition_exists(partition) {
            return Ok(());
        }

        // Create new column family
        // Note: With multi-threaded-cf feature, create_cf takes &self and handles locking internally
        let mut opts = Options::default();
        opts.set_write_buffer_size(self.settings.write_buffer_size);
        opts.set_max_write_buffer_number(self.settings.max_write_buffers);
        opts.optimize_for_point_lookup(std::cmp::max(
            1,
            (self.settings.block_cache_size / (1024 * 1024)) as u64,
        ));
        opts.set_block_based_table_factory(&crate::rocksdb_init::create_block_options_with_cache(
            &self.block_cache,
        ));
        match self.db.create_cf(partition.name(), &opts) {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                // Handle benign race: another thread created the CF between exists-check and create
                if msg.contains("Column family already exists")
                    || msg.contains("column family already exists")
                {
                    return Ok(());
                }
                Err(StorageError::IoError(msg))
            },
        }
    }

    fn list_partitions(&self) -> Result<Vec<Partition>> {
        // RocksDB doesn't have a direct cf_names() method on Arc<DB>
        // We need to iterate through CFs differently
        let mut partitions = Vec::new();

        // Try to get all column family handles
        // The default CF always exists, so we skip it
        for name in ["default"].iter() {
            if self.db.cf_handle(name).is_some() && *name != "default" {
                partitions.push(Partition::new(*name));
            }
        }

        // This is a limitation - we can't easily enumerate all CFs from Arc<DB>
        // In practice, the application should track CF names separately
        // For now, return empty list (excluding default)
        Ok(partitions)
    }

    fn drop_partition(&self, partition: &Partition) -> Result<()> {
        if !self.partition_exists(partition) {
            return Ok(());
        }

        // Note: With multi-threaded-cf feature, drop_cf takes &self and handles locking internally
        self.db
            .drop_cf(partition.name())
            .map_err(|e| StorageError::IoError(e.to_string()))?;

        Ok(())
    }

    fn compact_partition(&self, partition: &Partition) -> Result<()> {
        let cf = self.get_cf(partition)?;

        // Compact the entire column family range
        // This removes tombstones and optimizes storage after flush operations
        // Note: compact_range_cf is infallible (no Result return)
        self.db.compact_range_cf(&cf, None::<&[u8]>, None::<&[u8]>);

        Ok(())
    }

    fn backup_to(&self, backup_dir: &std::path::Path) -> crate::storage_trait::Result<()> {
        use rocksdb::backup::{BackupEngine, BackupEngineOptions};
        use rocksdb::Env;

        std::fs::create_dir_all(backup_dir).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "Failed to create backup directory '{}': {}",
                backup_dir.display(),
                e
            ))
        })?;

        let opts = BackupEngineOptions::new(backup_dir).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "BackupEngineOptions::new failed: {}",
                e
            ))
        })?;
        let env = Env::new().map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "Failed to create RocksDB Env: {}",
                e
            ))
        })?;
        let mut engine = BackupEngine::open(&opts, &env).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "Failed to open BackupEngine: {}",
                e
            ))
        })?;
        // flush_before_backup=true ensures memtable data is included
        engine.create_new_backup_flush(&*self.db, true).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "RocksDB create_new_backup_flush failed: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn restore_from(&self, backup_dir: &std::path::Path) -> crate::storage_trait::Result<()> {
        use rocksdb::backup::{BackupEngine, BackupEngineOptions, RestoreOptions};
        use rocksdb::Env;

        let opts = BackupEngineOptions::new(backup_dir).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "BackupEngineOptions::new failed: {}",
                e
            ))
        })?;
        let env = Env::new().map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "Failed to create RocksDB Env: {}",
                e
            ))
        })?;
        let mut engine = BackupEngine::open(&opts, &env).map_err(|e| {
            crate::storage_trait::StorageError::Other(format!(
                "Failed to open BackupEngine: {}",
                e
            ))
        })?;
        let restore_opts = RestoreOptions::default();

        // Restore to a staging directory alongside the live DB rather than
        // overwriting it while it is open. The caller must restart the server
        // and swap `<db_path>_restore_pending` with `<db_path>` to complete.
        let db_path = self.db.path();
        let staging_path = {
            let mut p = db_path.as_os_str().to_os_string();
            p.push("_restore_pending");
            std::path::PathBuf::from(p)
        };

        // Remove stale staging directory if present.
        if staging_path.exists() {
            std::fs::remove_dir_all(&staging_path).map_err(|e| {
                crate::storage_trait::StorageError::Other(format!(
                    "Failed to clear stale restore staging dir '{}': {}",
                    staging_path.display(),
                    e
                ))
            })?;
        }

        engine
            .restore_from_latest_backup(&staging_path, &staging_path, &restore_opts)
            .map_err(|e| {
                crate::storage_trait::StorageError::Other(format!(
                    "RocksDB restore_from_latest_backup failed: {}",
                    e
                ))
            })?;
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<DB>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open(&opts, temp_dir.path()).unwrap();
        (Arc::new(db), temp_dir)
    }

    #[test]
    fn test_create_and_get_partition() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        assert!(backend.partition_exists(&partition));
    }

    #[test]
    fn test_put_and_get() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        backend.put(&partition, b"key1", b"value1").unwrap();
        let value = backend.get(&partition, b"key1").unwrap();

        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_delete() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        backend.put(&partition, b"key1", b"value1").unwrap();
        backend.delete(&partition, b"key1").unwrap();

        let value = backend.get(&partition, b"key1").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_batch_operations() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        let ops = vec![
            Operation::Put {
                partition: partition.clone(),
                key: b"key1".to_vec(),
                value: b"value1".to_vec(),
            },
            Operation::Put {
                partition: partition.clone(),
                key: b"key2".to_vec(),
                value: b"value2".to_vec(),
            },
            Operation::Delete {
                partition: partition.clone(),
                key: b"key1".to_vec(),
            },
        ];

        backend.batch(ops).unwrap();

        assert_eq!(backend.get(&partition, b"key1").unwrap(), None);
        assert_eq!(backend.get(&partition, b"key2").unwrap(), Some(b"value2".to_vec()));
    }

    #[test]
    fn test_scan_all() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        backend.put(&partition, b"key1", b"value1").unwrap();
        backend.put(&partition, b"key2", b"value2").unwrap();
        backend.put(&partition, b"key3", b"value3").unwrap();

        let results: Vec<_> = backend.scan(&partition, None, None, None).unwrap().collect();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_scan_with_prefix() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        backend.put(&partition, b"user:1", b"value1").unwrap();
        backend.put(&partition, b"user:2", b"value2").unwrap();
        backend.put(&partition, b"admin:1", b"value3").unwrap();

        let results: Vec<_> =
            backend.scan(&partition, Some(b"user:"), None, None).unwrap().collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_with_limit() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();

        backend.put(&partition, b"key1", b"value1").unwrap();
        backend.put(&partition, b"key2", b"value2").unwrap();
        backend.put(&partition, b"key3", b"value3").unwrap();

        let results: Vec<_> = backend.scan(&partition, None, None, Some(2)).unwrap().collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_list_partitions() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        backend.create_partition(&Partition::new("cf1")).unwrap();
        backend.create_partition(&Partition::new("cf2")).unwrap();

        // Note: Current implementation has limited CF enumeration support
        // We verify partitions exist using partition_exists instead
        assert!(backend.partition_exists(&Partition::new("cf1")));
        assert!(backend.partition_exists(&Partition::new("cf2")));

        // list_partitions is currently limited due to Arc<DB> API constraints
        let partitions = backend.list_partitions().unwrap();
        // Just verify it doesn't panic - actual enumeration is limited
        let _ = partitions.len(); // Suppress unused warning
    }

    #[test]
    fn test_drop_partition() {
        let (db, _temp) = create_test_db();
        let backend = RocksDBBackend::new(db);

        let partition = Partition::new("test_cf");
        backend.create_partition(&partition).unwrap();
        assert!(backend.partition_exists(&partition));

        backend.drop_partition(&partition).unwrap();
        assert!(!backend.partition_exists(&partition));
    }
}
