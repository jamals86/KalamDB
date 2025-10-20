// RocksDB storage implementation
use rocksdb::{DBCompressionType, IteratorMode, Options, WriteOptions, DB};
use std::path::Path;
use std::sync::Arc;

/// RocksDB-based storage
pub struct RocksDbStore {
    db: Arc<DB>,
}

impl RocksDbStore {
    /// Open or create a RocksDB database at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, rocksdb::Error> {
        Self::open_with_options(path, true, "none")
    }

    /// Open or create a RocksDB database with custom options
    pub fn open_with_options<P: AsRef<Path>>(
        path: P,
        enable_wal: bool,
        compression: &str,
    ) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Enable WAL for durability
        opts.set_use_fsync(!enable_wal);

        // Set compression based on config
        let compression_type = match compression.to_lowercase().as_str() {
            "snappy" => DBCompressionType::Snappy,
            "zlib" => DBCompressionType::Zlib,
            "lz4" => DBCompressionType::Lz4,
            "zstd" => DBCompressionType::Zstd,
            _ => DBCompressionType::None,
        };
        opts.set_compression_type(compression_type);

        // Optimize for SSD
        opts.set_level_compaction_dynamic_level_bytes(true);

        let db = DB::open(&opts, path)?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Put a key-value pair into the database
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<(), rocksdb::Error> {
        self.db.put(key, value)
    }

    /// Put a key-value pair with custom write options
    pub fn put_with_options(
        &self,
        key: &[u8],
        value: &[u8],
        write_opts: &WriteOptions,
    ) -> Result<(), rocksdb::Error> {
        self.db.put_opt(key, value, write_opts)
    }

    /// Get a value by key
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        self.db.get(key)
    }

    /// Delete a key-value pair
    pub fn delete(&self, key: &[u8]) -> Result<(), rocksdb::Error> {
        self.db.delete(key)
    }

    /// Check if a key exists
    pub fn exists(&self, key: &[u8]) -> Result<bool, rocksdb::Error> {
        self.db.get(key).map(|v| v.is_some())
    }

    /// Iterate over all key-value pairs
    pub fn iter(&self) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + '_ {
        self.db
            .iterator(IteratorMode::Start)
            .map(|result| result.unwrap())
    }

    /// Iterate over keys with a specific prefix
    pub fn iter_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + 'a {
        self.db
            .iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward))
            .map(|result| result.unwrap())
            .take_while(move |(key, _)| key.starts_with(prefix))
    }

    /// Get approximate number of keys
    pub fn approximate_keys(&self) -> Result<u64, rocksdb::Error> {
        self.db
            .property_int_value("rocksdb.estimate-num-keys")
            .map(|opt| opt.unwrap_or(0))
    }

    /// Flush memtable to disk
    pub fn flush(&self) -> Result<(), rocksdb::Error> {
        self.db.flush()
    }

    /// Get database statistics
    pub fn stats(&self) -> Option<String> {
        self.db.property_value("rocksdb.stats").ok().flatten()
    }

    /// Compact the database
    pub fn compact(&self) {
        self.db.compact_range::<&[u8], &[u8]>(None, None);
    }
}

impl Clone for RocksDbStore {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (RocksDbStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDbStore::open(temp_dir.path()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_put_and_get() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();
        let value = db.get(b"key1").unwrap();

        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_get_nonexistent() {
        let (db, _temp_dir) = create_test_db();

        let value = db.get(b"nonexistent").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_delete() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();
        db.delete(b"key1").unwrap();
        let value = db.get(b"key1").unwrap();

        assert_eq!(value, None);
    }

    #[test]
    fn test_exists() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();
        assert!(db.exists(b"key1").unwrap());
        assert!(!db.exists(b"key2").unwrap());
    }

    #[test]
    fn test_iter() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();
        db.put(b"key2", b"value2").unwrap();
        db.put(b"key3", b"value3").unwrap();

        let count = db.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_iter_prefix() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"msg:1", b"value1").unwrap();
        db.put(b"msg:2", b"value2").unwrap();
        db.put(b"conv:1", b"value3").unwrap();

        let msg_count = db.iter_prefix(b"msg:").count();
        assert_eq!(msg_count, 2);

        let conv_count = db.iter_prefix(b"conv:").count();
        assert_eq!(conv_count, 1);
    }

    #[test]
    fn test_clone() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();

        let db_clone = db.clone();
        let value = db_clone.get(b"key1").unwrap();

        assert_eq!(value, Some(b"value1".to_vec()));
    }

    #[test]
    fn test_flush() {
        let (db, _temp_dir) = create_test_db();

        db.put(b"key1", b"value1").unwrap();
        assert!(db.flush().is_ok());
    }
}
