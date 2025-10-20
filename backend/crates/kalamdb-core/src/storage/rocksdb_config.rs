//! RocksDB configuration
//!
//! This module provides configuration options for RocksDB including
//! per-column-family settings for memtable, write buffer, WAL, and compaction.

use rocksdb::{BlockBasedOptions, Cache, Options, SliceTransform};

/// RocksDB configuration builder
pub struct RocksDbConfig {
    /// Base options for the database
    pub db_options: Options,
}

impl RocksDbConfig {
    /// Create default RocksDB configuration
    pub fn default() -> Self {
        let mut opts = Options::default();

        // Enable creation of missing column families
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // WAL settings
        opts.set_wal_ttl_seconds(3600); // 1 hour WAL retention
        opts.set_wal_size_limit_mb(1024); // 1GB WAL size limit

        // Compaction settings
        opts.set_max_background_jobs(4);
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_level_zero_slowdown_writes_trigger(20);
        opts.set_level_zero_stop_writes_trigger(36);

        // Write buffer settings
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB write buffer
        opts.set_max_write_buffer_number(3);
        opts.set_min_write_buffer_number_to_merge(1);

        // Block cache for read performance
        let cache = Cache::new_lru_cache(256 * 1024 * 1024); // 256MB cache
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_cache(&cache);
        block_opts.set_block_size(16 * 1024); // 16KB blocks
        block_opts.set_bloom_filter(10.0, false); // 10 bits per key bloom filter
        opts.set_block_based_table_factory(&block_opts);

        // Compression
        opts.set_compression_type(rocksdb::DBCompressionType::Snappy);

        Self { db_options: opts }
    }

    /// Create column family options for user tables
    ///
    /// User tables have moderate write throughput with user-based key prefixes
    pub fn user_table_cf_options() -> Options {
        let mut opts = Options::default();

        // Write buffer optimized for medium-sized batches
        opts.set_write_buffer_size(128 * 1024 * 1024); // 128MB
        opts.set_max_write_buffer_number(3);

        // Prefix extractor for user_id-based keys
        // Keys format: {user_id}:{row_id}
        // We'll use the user_id as the prefix
        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(36)); // UUID length

        // Bloom filter with prefix support
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        block_opts.set_whole_key_filtering(false); // Use prefix filtering
        opts.set_block_based_table_factory(&block_opts);

        opts
    }

    /// Create column family options for shared tables
    ///
    /// Shared tables have lower write throughput but need fast reads
    pub fn shared_table_cf_options() -> Options {
        let mut opts = Options::default();

        // Smaller write buffer since writes are less frequent
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(2);

        // No prefix extractor for shared tables (no user_id prefix)

        // Standard bloom filter
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        opts
    }

    /// Create column family options for stream tables
    ///
    /// Stream tables have high write throughput with timestamp-based keys
    pub fn stream_table_cf_options() -> Options {
        let mut opts = Options::default();

        // Larger write buffer for high-frequency writes
        opts.set_write_buffer_size(256 * 1024 * 1024); // 256MB
        opts.set_max_write_buffer_number(4);

        // Prefix extractor for timestamp-based keys
        // Keys format: {timestamp}:{row_id}
        opts.set_prefix_extractor(SliceTransform::create_fixed_prefix(8)); // 8-byte timestamp

        // Bloom filter with prefix support
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        block_opts.set_whole_key_filtering(false);
        opts.set_block_based_table_factory(&block_opts);

        // TTL-based compaction filter would be added here for stream tables
        // (Future enhancement)

        opts
    }

    /// Create column family options for system tables
    ///
    /// System tables have low write throughput, small data size
    pub fn system_table_cf_options() -> Options {
        let mut opts = Options::default();

        // Small write buffer for infrequent writes
        opts.set_write_buffer_size(32 * 1024 * 1024); // 32MB
        opts.set_max_write_buffer_number(2);

        // Standard bloom filter
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);

        opts
    }

    /// Get column family options based on table type
    pub fn cf_options_for_table_type(table_type: crate::catalog::TableType) -> Options {
        match table_type {
            crate::catalog::TableType::User => Self::user_table_cf_options(),
            crate::catalog::TableType::Shared => Self::shared_table_cf_options(),
            crate::catalog::TableType::Stream => Self::stream_table_cf_options(),
            crate::catalog::TableType::System => Self::system_table_cf_options(),
        }
    }
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableType;

    #[test]
    fn test_default_config() {
        let _config = RocksDbConfig::default();
        // DB options are created - config object exists
    }

    #[test]
    fn test_user_table_options() {
        let opts = RocksDbConfig::user_table_cf_options();
        // Options are configured but we can't easily test specific values
        // Just ensure it doesn't panic
        drop(opts);
    }

    #[test]
    fn test_cf_options_for_table_type() {
        let _ = RocksDbConfig::cf_options_for_table_type(TableType::User);
        let _ = RocksDbConfig::cf_options_for_table_type(TableType::Shared);
        let _ = RocksDbConfig::cf_options_for_table_type(TableType::Stream);
        let _ = RocksDbConfig::cf_options_for_table_type(TableType::System);
    }
}
