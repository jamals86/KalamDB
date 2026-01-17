//! Manifest file operations using object_store.
//!
//! Provides read/write operations for manifest.json files with atomic writes.
//! Supports both local and remote storage backends.

use crate::error::{FilestoreError, Result};
use crate::object_store_ops::{read_file_sync, write_file_sync};
use bytes::Bytes;
use kalamdb_commons::system::Storage;
use object_store::ObjectStore;
use std::sync::Arc;

/// Read manifest.json from storage.
///
/// Returns the raw JSON string.
pub fn read_manifest_json(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    manifest_path: &str,
) -> Result<String> {
    let bytes = read_file_sync(store, storage, manifest_path)?;
    String::from_utf8(bytes.to_vec())
        .map_err(|e| FilestoreError::Other(format!("Invalid UTF-8 in manifest: {}", e)))
}

/// Write manifest.json to storage atomically.
///
/// For local storage, this uses temp file + rename for atomicity.
/// For object_store backends, PUT operations are atomic by design.
pub fn write_manifest_json(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    manifest_path: &str,
    json_content: &str,
) -> Result<()> {
    let bytes = Bytes::from(json_content.to_string());
    write_file_sync(store, storage, manifest_path, bytes)
}

/// Check if manifest.json exists.
pub fn manifest_exists(
    store: Arc<dyn ObjectStore>,
    storage: &Storage,
    manifest_path: &str,
) -> Result<bool> {
    use crate::object_store_ops::head_file_sync;

    match head_file_sync(store, storage, manifest_path) {
        Ok(_) => Ok(true),
        Err(FilestoreError::ObjectStore(ref e)) if e.contains("not found") || e.contains("404") => {
            Ok(false)
        },
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_object_store;
    use kalamdb_commons::models::ids::StorageId;
    use kalamdb_commons::models::storage::StorageType;
    use kalamdb_commons::models::types::Storage;
    use std::env;
    use std::fs;

    fn create_test_storage(temp_dir: &std::path::Path) -> Storage {
        let now = chrono::Utc::now().timestamp_millis();
        Storage {
            storage_id: StorageId::from("test_manifest"),
            storage_name: "test_manifest".to_string(),
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
    fn test_write_and_read_manifest_json() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_write_read");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test_namespace/test_table/manifest.json";
        let test_json = r#"{"version":1,"segments":[]}"#;

        // Write manifest
        let write_result =
            write_manifest_json(Arc::clone(&store), &storage, manifest_path, test_json);
        assert!(write_result.is_ok(), "Failed to write manifest");

        // Read manifest back
        let read_result = read_manifest_json(Arc::clone(&store), &storage, manifest_path);
        assert!(read_result.is_ok(), "Failed to read manifest");
        assert_eq!(read_result.unwrap(), test_json);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_manifest_exists_after_write() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_exists");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test_namespace/test_table/manifest.json";

        // Check doesn't exist initially
        let exists_before = manifest_exists(Arc::clone(&store), &storage, manifest_path);
        assert!(exists_before.is_ok());
        assert!(!exists_before.unwrap(), "Manifest should not exist yet");

        // Write manifest
        let test_json = r#"{"version":1}"#;
        write_manifest_json(Arc::clone(&store), &storage, manifest_path, test_json).unwrap();

        // Check exists after write
        let exists_after = manifest_exists(Arc::clone(&store), &storage, manifest_path);
        assert!(exists_after.is_ok());
        assert!(exists_after.unwrap(), "Manifest should exist after write");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_manifest_creates_directories() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_mkdir");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        // Deep nested path
        let manifest_path = "ns1/ns2/ns3/table/manifest.json";
        let test_json = r#"{"version":1,"segments":[]}"#;

        // Should create all intermediate directories
        let result = write_manifest_json(Arc::clone(&store), &storage, manifest_path, test_json);
        assert!(result.is_ok(), "Should create nested directories");

        // Verify can read it back
        let read_result = read_manifest_json(store, &storage, manifest_path);
        assert!(read_result.is_ok());
        assert_eq!(read_result.unwrap(), test_json);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_nonexistent_manifest() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_nonexistent");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "nonexistent/manifest.json";
        let result = read_manifest_json(store, &storage, manifest_path);

        assert!(result.is_err(), "Should fail for nonexistent file");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_manifest_json_with_special_characters() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_special");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test/manifest.json";

        // JSON with unicode, escapes, etc.
        let test_json = r#"{"version":1,"name":"Test 🚀","segments":["seg-1","seg-2"],"metadata":{"key":"value with \"quotes\""}}"#;

        write_manifest_json(Arc::clone(&store), &storage, manifest_path, test_json).unwrap();
        let read_back = read_manifest_json(store, &storage, manifest_path).unwrap();

        assert_eq!(read_back, test_json);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_manifest_overwrite() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_overwrite");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test/manifest.json";

        // Write first version
        let v1_json = r#"{"version":1}"#;
        write_manifest_json(Arc::clone(&store), &storage, manifest_path, v1_json).unwrap();

        let read_v1 = read_manifest_json(Arc::clone(&store), &storage, manifest_path).unwrap();
        assert_eq!(read_v1, v1_json);

        // Overwrite with second version
        let v2_json = r#"{"version":2}"#;
        write_manifest_json(Arc::clone(&store), &storage, manifest_path, v2_json).unwrap();

        let read_v2 = read_manifest_json(store, &storage, manifest_path).unwrap();
        assert_eq!(read_v2, v2_json, "Should read updated version");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_empty_manifest_json() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_empty");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test/empty_manifest.json";
        let empty_json = "{}";

        write_manifest_json(Arc::clone(&store), &storage, manifest_path, empty_json).unwrap();
        let read_back = read_manifest_json(store, &storage, manifest_path).unwrap();

        assert_eq!(read_back, empty_json);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_large_manifest_json() {
        let temp_dir = env::temp_dir().join("kalamdb_test_manifest_large");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let storage = create_test_storage(&temp_dir);
        let store = build_object_store(&storage).expect("Failed to build store");

        let manifest_path = "test/large_manifest.json";

        // Generate large JSON with many segments
        let mut segments = Vec::new();
        for i in 0..1000 {
            segments.push(format!(r#"{{"id":"seg-{}","size":{}}}"#, i, i * 1024));
        }
        let large_json = format!(r#"{{"version":1,"segments":[{}]}}"#, segments.join(","));

        write_manifest_json(Arc::clone(&store), &storage, manifest_path, &large_json).unwrap();
        let read_back = read_manifest_json(store, &storage, manifest_path).unwrap();

        assert_eq!(read_back, large_json);
        assert!(read_back.len() > 10000, "Should be a large manifest");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
