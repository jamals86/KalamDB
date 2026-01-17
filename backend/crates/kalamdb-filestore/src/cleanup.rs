//! Table storage cleanup operations using object_store.
//!
//! Provides functions to delete Parquet storage trees when tables are dropped.

use crate::error::{FilestoreError, Result};
use crate::object_store_ops::{delete_prefix_sync, list_files_sync};
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::sync::Arc;

/// Delete Parquet storage for a table based on a partially-resolved template.
///
/// The `relative_template` should already have static placeholders substituted
/// (e.g., `{namespace}`, `{tableName}`) and may still contain dynamic ones
/// (e.g., `{userId}`, `{shard}`).
///
/// - User tables: Deletes all user subdirectories under the `{userId}` prefix.
/// - Shared tables: Deletes the table directory; if `{shard}` is present, deletes
///   the directory prefix before `{shard}`.
/// - Stream tables: No-op (in-memory only).
///
/// Returns the total number of bytes freed (best-effort, computed before deletion).
pub fn delete_parquet_tree_for_table(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    relative_template: &str,
    table_type: TableType,
) -> Result<u64> {
    let rel = relative_template.trim_start_matches('/');

    let bytes_freed = match table_type {
        TableType::User => {
            // Split at {userId} and remove each subdirectory under the prefix
            if let Some((prefix, _)) = rel.split_once("{userId}") {
                // Clean trailing slash from prefix
                let prefix = prefix.trim_end_matches('/');
                
                // Check if prefix exists
                let files = list_files_sync(Arc::clone(&store), storage, prefix)?;
                if files.is_empty() {
                    0
                } else {
                    // Delete all files under the prefix
                    delete_prefix_sync(Arc::clone(&store), storage, prefix)?
                }
            } else {
                // No {userId}; treat like shared
                delete_prefix_sync(Arc::clone(&store), storage, rel)?
            }
        }
        TableType::Shared => {
            let target = if let Some((prefix, _)) = rel.split_once("{shard}") {
                prefix.trim_end_matches('/')
            } else {
                rel
            };
            delete_prefix_sync(Arc::clone(&store), storage, target)?
        }
        TableType::Stream => 0,
        TableType::System => {
            return Err(FilestoreError::Other(
                "Attempted to delete Parquet for system table".to_string(),
            ));
        }
    };

    Ok(bytes_freed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_object_store;
    use crate::object_store_ops::write_file_sync;
    use bytes::Bytes;
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use kalamdb_commons::models::types::Storage;
    use std::env;
    use std::fs;

    fn create_test_storage(temp_dir: &std::path::Path) -> Storage {
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_cleanup"),
            storage_name: "test_cleanup".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_delete_shared_table_basic() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_shared");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create some files under a shared table path
        let files = vec![
            "namespace1/shared_table/seg1.parquet",
            "namespace1/shared_table/seg2.parquet",
            "namespace1/shared_table/manifest.json",
        ];

        for file in &files {
            write_file_sync(
                Arc::clone(&store),
                &storage,
                file,
                Bytes::from("test data"),
            )
            .unwrap();
        }

        // Delete the shared table
        let result = delete_parquet_tree_for_table(
            Arc::clone(&store),
            &storage,
            "namespace1/shared_table",
            TableType::Shared,
        );
        assert!(result.is_ok());

        // Verify files are deleted
        let remaining = list_files_sync(store, &storage, "namespace1/shared_table").unwrap();
        assert_eq!(remaining.len(), 0, "All files should be deleted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_shared_table_with_shard() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_shard");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create files in a sharded structure
        let files = vec![
            "namespace1/table/shard_0/seg1.parquet",
            "namespace1/table/shard_1/seg2.parquet",
        ];

        for file in &files {
            write_file_sync(
                Arc::clone(&store),
                &storage,
                file,
                Bytes::from("test data"),
            )
            .unwrap();
        }

        // Delete using template with {shard}
        let result = delete_parquet_tree_for_table(
            Arc::clone(&store),
            &storage,
            "namespace1/table/{shard}",
            TableType::Shared,
        );
        assert!(result.is_ok());

        // Verify files are deleted
        let remaining = list_files_sync(store, &storage, "namespace1/table").unwrap();
        assert_eq!(remaining.len(), 0, "All sharded files should be deleted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_user_table_with_placeholder() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_user");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create files for multiple users
        let files = vec![
            "namespace1/user1/table/seg1.parquet",
            "namespace1/user2/table/seg2.parquet",
            "namespace1/user3/table/seg3.parquet",
        ];

        for file in &files {
            write_file_sync(
                Arc::clone(&store),
                &storage,
                file,
                Bytes::from("test data"),
            )
            .unwrap();
        }

        // Delete all user data using {userId} placeholder
        let result = delete_parquet_tree_for_table(
            Arc::clone(&store),
            &storage,
            "namespace1/{userId}/table",
            TableType::User,
        );
        assert!(result.is_ok());

        // Verify all user files are deleted
        let remaining = list_files_sync(store, &storage, "namespace1").unwrap();
        assert_eq!(remaining.len(), 0, "All user files should be deleted");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_stream_table_is_noop() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_stream");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create a file (shouldn't be deleted for stream tables)
        write_file_sync(
            Arc::clone(&store),
            &storage,
            "namespace1/stream_table/seg1.parquet",
            Bytes::from("test data"),
        )
        .unwrap();

        // Try to delete stream table (should be no-op)
        let result = delete_parquet_tree_for_table(
            Arc::clone(&store),
            &storage,
            "namespace1/stream_table",
            TableType::Stream,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "Stream tables should return 0 bytes");

        // Verify file still exists
        let remaining = list_files_sync(store, &storage, "namespace1/stream_table").unwrap();
        assert_eq!(
            remaining.len(),
            1,
            "Stream table files should not be deleted"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_system_table_errors() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_system");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Attempt to delete system table should error
        let result = delete_parquet_tree_for_table(
            store,
            &storage,
            "system/tables",
            TableType::System,
        );
        assert!(result.is_err(), "Should error for system tables");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_nonexistent_table() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_nonexistent");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Delete non-existent table should succeed (idempotent)
        let result = delete_parquet_tree_for_table(
            store,
            &storage,
            "namespace1/nonexistent_table",
            TableType::Shared,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "Should return 0 bytes for empty prefix");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_with_leading_slash() {
        let temp_dir = env::temp_dir().join("kalamdb_test_cleanup_slash");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Create file
        write_file_sync(
            Arc::clone(&store),
            &storage,
            "namespace1/table/seg.parquet",
            Bytes::from("test"),
        )
        .unwrap();

        // Delete with leading slash (should be trimmed)
        let result = delete_parquet_tree_for_table(
            Arc::clone(&store),
            &storage,
            "/namespace1/table",
            TableType::Shared,
        );
        assert!(result.is_ok());

        // Verify deleted
        let remaining = list_files_sync(store, &storage, "namespace1/table").unwrap();
        assert_eq!(remaining.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
