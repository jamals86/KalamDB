//! System table initialization and schema versioning.
//!
//! This module handles:
//! - System schema version tracking in RocksDB
//! - Version comparison and upgrade logic
//! - Future migration path for new system tables
//!
//! ## Version History
//!
//! - **v1 (2025-01-15)**: Initial schema with 7 system tables:
//!   - system.users, system.namespaces, system.tables, system.storages
//!   - system.live_queries, system.jobs, system.audit_logs
//!
//! ## Usage
//!
//! Called from `lifecycle::bootstrap()` after AppContext creation:
//!
//! ```rust,ignore
//! let app_context = AppContext::init(...);
//! initialize_system_tables(app_context.storage_backend()).await?;
//! ```

use kalamdb_commons::constants::{SYSTEM_SCHEMA_VERSION, SYSTEM_SCHEMA_VERSION_KEY};
use kalamdb_store::{Partition, StorageBackend};
use crate::error::SystemError;
use std::sync::Arc;

/// Initialize system tables and verify schema version.
///
/// This function:
/// 1. Reads stored schema version from RocksDB (`system:schema_version` key)
/// 2. Compares with current `SYSTEM_SCHEMA_VERSION` constant
/// 3. If stored < current: logs upgrade message (future: create missing tables)
/// 4. Stores current version on first init or after upgrade
///
/// ## Version Tracking
///
/// Schema version is stored in RocksDB default column family with key
/// `system:schema_version` as a u32 in big-endian bytes.
///
/// ## Future Migrations
///
/// When adding new system tables:
/// 1. Increment `SYSTEM_SCHEMA_VERSION` in constants.rs
/// 2. Add migration logic in `match stored_version` block below
/// 3. Document version in constants.rs comment
///
/// ## Error Handling
///
/// Returns `SystemError` if:
/// - RocksDB read/write fails
/// - Version deserialization fails (data corruption)
///
/// ## Example
///
/// ```rust,ignore
/// // In lifecycle::bootstrap()
/// initialize_system_tables(storage_backend).await?;
/// ```
pub async fn initialize_system_tables(
    storage_backend: Arc<dyn StorageBackend>,
) -> Result<(), SystemError> {
    let current_version = SYSTEM_SCHEMA_VERSION;
    let version_key = SYSTEM_SCHEMA_VERSION_KEY.as_bytes();
    let default_partition = Partition::new("default");

    // Read stored version from RocksDB
    let stored_version_opt = match storage_backend.get(&default_partition, version_key)? {
        Some(bytes) => {
            if bytes.len() != 4 {
                return Err(SystemError::Other(format!(
                    "Invalid schema version data: expected 4 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut version_bytes = [0u8; 4];
            version_bytes.copy_from_slice(&bytes);
            Some(u32::from_be_bytes(version_bytes))
        }
        None => None, // First initialization - no version stored yet
    };

    // Version comparison and upgrade/initialization logic
    match stored_version_opt {
        None => {
            // First-time initialization
            log::info!(
                "Initializing system schema at v{} (first startup)",
                current_version
            );
            
            // All 7 system tables created on first access by EntityStore providers
            // No explicit migration needed
        }
        Some(stored_version) if stored_version < current_version => {
            // Upgrade from older version
            log::info!(
                "System schema upgrade detected: v{} -> v{}",
                stored_version,
                current_version
            );

            // Migration logic based on stored version
            match stored_version {
                1 => {
                    // Future: v1 -> v2 migration
                    // Example: add new system table
                    log::info!("Migrating v1 -> v2: adding new system tables");
                }
                _ => {
                    log::warn!(
                        "Unknown stored schema version {} - upgrading to v{}",
                        stored_version,
                        current_version
                    );
                }
            }
        }
        Some(stored_version) if stored_version == current_version => {
            // Already at current version - no action needed
            log::debug!(
                "System schema version v{} is current",
                current_version
            );
        }
        Some(stored_version) => {
            // Stored version is NEWER than current (downgrade not supported)
            return Err(SystemError::Other(format!(
                "Schema downgrade not supported: stored v{} > current v{}. \
                 Please upgrade to a newer server version.",
                stored_version,
                current_version
            )));
        }
    }

    // Store/update current version in RocksDB
    let version_bytes = current_version.to_be_bytes();
    storage_backend.put(&default_partition, version_key, &version_bytes)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::{RocksDBBackend, RocksDbInit};
    use tempfile::TempDir;

    /// Helper to create temporary RocksDB backend for testing
    fn create_test_backend() -> (Arc<dyn StorageBackend>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let init = RocksDbInit::with_defaults(temp_dir.path().to_str().unwrap());
        let db = init.open().expect("Failed to open RocksDB");
        let backend = RocksDBBackend::new(db);
        (Arc::new(backend), temp_dir)
    }

    #[tokio::test]
    async fn test_first_initialization() {
        let (backend, _temp) = create_test_backend();
        let default_partition = Partition::new("default");
        
        // First init should succeed with no stored version
        let result = initialize_system_tables(backend.clone()).await;
        assert!(result.is_ok(), "First initialization failed");

        // Verify version stored
        let version_key = SYSTEM_SCHEMA_VERSION_KEY.as_bytes();
        let stored = backend.get(&default_partition, version_key).expect("Failed to read version");
        assert!(stored.is_some(), "Version not stored after initialization");
        
        let version_bytes = stored.unwrap();
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&version_bytes);
        let stored_version = u32::from_be_bytes(bytes);
        assert_eq!(stored_version, SYSTEM_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_version_unchanged() {
        let (backend, _temp) = create_test_backend();
        
        // Initialize once
        initialize_system_tables(backend.clone()).await.expect("First init failed");
        
        // Initialize again - should be idempotent
        let result = initialize_system_tables(backend.clone()).await;
        assert!(result.is_ok(), "Repeated initialization failed");
    }

    #[tokio::test]
    async fn test_version_upgrade() {
        let (backend, _temp) = create_test_backend();
        let default_partition = Partition::new("default");
        
        // Manually store old version (v0)
        let version_key = SYSTEM_SCHEMA_VERSION_KEY.as_bytes();
        let old_version = 0u32;
        backend.put(&default_partition, version_key, &old_version.to_be_bytes()).expect("Failed to store v0");
        
        // Initialize should upgrade to current version
        let result = initialize_system_tables(backend.clone()).await;
        assert!(result.is_ok(), "Upgrade initialization failed");
        
        // Verify version updated
        let stored = backend.get(&default_partition, version_key).expect("Failed to read version");
        let version_bytes = stored.unwrap();
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&version_bytes);
        let stored_version = u32::from_be_bytes(bytes);
        assert_eq!(stored_version, SYSTEM_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_version_downgrade_rejected() {
        let (backend, _temp) = create_test_backend();
        let default_partition = Partition::new("default");
        
        // Manually store future version (v999)
        let version_key = SYSTEM_SCHEMA_VERSION_KEY.as_bytes();
        let future_version = 999u32;
        backend.put(&default_partition, version_key, &future_version.to_be_bytes()).expect("Failed to store v999");
        
        // Initialize should reject downgrade
        let result = initialize_system_tables(backend.clone()).await;
        assert!(result.is_err(), "Downgrade should be rejected");
        
        let err = result.unwrap_err();
        assert!(
            matches!(err, SystemError::Other(_)),
            "Expected Other error for downgrade"
        );
    }

    #[tokio::test]
    async fn test_invalid_version_data() {
        let (backend, _temp) = create_test_backend();
        let default_partition = Partition::new("default");
        
        // Store invalid version data (wrong length)
        let version_key = SYSTEM_SCHEMA_VERSION_KEY.as_bytes();
        backend.put(&default_partition, version_key, &[1, 2, 3]).expect("Failed to store invalid data");
        
        // Initialize should reject invalid data
        let result = initialize_system_tables(backend.clone()).await;
        assert!(result.is_err(), "Invalid version data should be rejected");
        
        let err = result.unwrap_err();
        assert!(
            matches!(err, SystemError::Other(_)),
            "Expected Other error for invalid data"
        );
    }
}
