//! Unified ObjectStore factory for all storage backends.
//!
//! Uses `object_store` crate uniformly for local filesystem and cloud storage.
//! No if/else branching - all backends implement the same `ObjectStore` trait.

use crate::error::{FilestoreError, Result};
use kalamdb_commons::models::{StorageLocationConfig, StorageLocationConfigError};
use kalamdb_commons::system::Storage;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectStorePath;
use object_store::prefix::PrefixStore;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::sync::Arc;

/// Build an `ObjectStore` instance from a Storage entity.
///
/// All storage types (local, S3, GCS, Azure) are unified under `Arc<dyn ObjectStore>`.
/// For local storage, uses `LocalFileSystem` which implements the same trait.
pub fn build_object_store(storage: &Storage) -> Result<Arc<dyn ObjectStore>> {
    let config = resolve_config(storage)?;

    match config {
        StorageLocationConfig::Local(_) => build_local(storage),
        StorageLocationConfig::S3(cfg) => build_s3(storage, &cfg),
        StorageLocationConfig::Gcs(cfg) => build_gcs(storage, &cfg),
        StorageLocationConfig::Azure(cfg) => build_azure(storage, &cfg),
    }
}

/// Resolve the storage location config, falling back to defaults based on storage_type.
fn resolve_config(storage: &Storage) -> Result<StorageLocationConfig> {
    match storage.location_config() {
        Ok(cfg) => Ok(cfg),
        Err(StorageLocationConfigError::MissingConfigJson) => {
            // Fall back based on storage_type field
            use kalamdb_commons::models::StorageType;
            Ok(match storage.storage_type {
                StorageType::S3 => StorageLocationConfig::S3(Default::default()),
                StorageType::Gcs => StorageLocationConfig::Gcs(Default::default()),
                StorageType::Azure => StorageLocationConfig::Azure(Default::default()),
                StorageType::Filesystem => StorageLocationConfig::Local(Default::default()),
            })
        },
        Err(e) => Err(FilestoreError::Config(e.to_string())),
    }
}

fn build_local(storage: &Storage) -> Result<Arc<dyn ObjectStore>> {
    let base = storage.base_directory.trim();
    if base.is_empty() {
        return Err(FilestoreError::Config(
            "Local storage requires non-empty base_directory".into(),
        ));
    }

    let path = PathBuf::from(base);

    // Ensure the directory exists before canonicalizing
    // LocalFileSystem::new_with_prefix requires an absolute path that exists
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| {
            FilestoreError::Config(format!(
                "Failed to create storage directory '{}': {}",
                path.display(),
                e
            ))
        })?;
    }

    // Canonicalize to get absolute path (resolves ., .., symlinks)
    let absolute_path = path.canonicalize().map_err(|e| {
        FilestoreError::Config(format!(
            "Failed to resolve absolute path for '{}': {}",
            path.display(),
            e
        ))
    })?;

    // LocalFileSystem with prefix handles path resolution automatically
    LocalFileSystem::new_with_prefix(absolute_path)
        .map(|fs| Arc::new(fs) as Arc<dyn ObjectStore>)
        .map_err(|e| FilestoreError::Config(format!("LocalFileSystem: {e}")))
}

fn build_s3(
    storage: &Storage,
    cfg: &kalamdb_commons::models::S3StorageConfig,
) -> Result<Arc<dyn ObjectStore>> {
    let (bucket, prefix) = parse_remote_url(&storage.base_directory, &["s3://"])?;

    let mut builder = AmazonS3Builder::new().with_bucket_name(&bucket);

    if let Some(ref region) = cfg.region {
        builder = builder.with_region(region);
    }
    if let Some(ref endpoint) = cfg.endpoint {
        builder = builder.with_endpoint(endpoint);
    }
    if cfg.allow_http {
        builder = builder.with_allow_http(true);
    }
    if let (Some(ref ak), Some(ref sk)) = (&cfg.access_key_id, &cfg.secret_access_key) {
        builder = builder.with_access_key_id(ak).with_secret_access_key(sk);
    }
    if let Some(ref token) = cfg.session_token {
        builder = builder.with_token(token);
    }

    let store = builder.build().map_err(|e| FilestoreError::Config(format!("S3: {e}")))?;

    wrap_with_prefix(store, &prefix)
}

fn build_gcs(
    storage: &Storage,
    cfg: &kalamdb_commons::models::GcsStorageConfig,
) -> Result<Arc<dyn ObjectStore>> {
    let (bucket, prefix) = parse_remote_url(&storage.base_directory, &["gs://", "gcs://"])?;

    let mut builder = GoogleCloudStorageBuilder::new().with_bucket_name(&bucket);

    if let Some(ref sa) = cfg.service_account_json {
        builder = builder.with_service_account_key(sa);
    }

    let store = builder.build().map_err(|e| FilestoreError::Config(format!("GCS: {e}")))?;

    wrap_with_prefix(store, &prefix)
}

fn build_azure(
    storage: &Storage,
    cfg: &kalamdb_commons::models::AzureStorageConfig,
) -> Result<Arc<dyn ObjectStore>> {
    let (container, prefix) = parse_remote_url(&storage.base_directory, &["az://", "azure://"])?;

    let mut builder = MicrosoftAzureBuilder::new().with_container_name(&container);

    if let Some(ref account) = cfg.account_name {
        builder = builder.with_account(account);
    }
    if let Some(ref key) = cfg.access_key {
        builder = builder.with_access_key(key);
    }
    if let Some(ref sas) = cfg.sas_token {
        // Parse SAS token query string into key-value pairs
        let query_pairs: Vec<(String, String)> = sas
            .trim_start_matches('?')
            .split('&')
            .filter_map(|pair| pair.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect();
        builder = builder.with_sas_authorization(query_pairs);
    }

    let store = builder.build().map_err(|e| FilestoreError::Config(format!("Azure: {e}")))?;

    wrap_with_prefix(store, &prefix)
}

/// Parse a remote URL like `s3://bucket/prefix` into (bucket, prefix).
fn parse_remote_url(url: &str, schemes: &[&str]) -> Result<(String, String)> {
    let trimmed = url.trim();

    for scheme in schemes {
        if let Some(rest) = trimmed.strip_prefix(scheme) {
            let (bucket, prefix) = match rest.split_once('/') {
                Some((b, p)) => (b.to_string(), p.to_string()),
                None => (rest.to_string(), String::new()),
            };
            return Ok((bucket, prefix));
        }
    }

    Err(FilestoreError::Config(format!(
        "Expected URL with schemes {:?}, got: {}",
        schemes, url
    )))
}

/// Wrap a store with a PrefixStore if prefix is non-empty.
fn wrap_with_prefix<T: ObjectStore + 'static>(
    store: T,
    prefix: &str,
) -> Result<Arc<dyn ObjectStore>> {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        Ok(Arc::new(store) as Arc<dyn ObjectStore>)
    } else {
        let prefix_path =
            ObjectStorePath::parse(prefix).map_err(|e| FilestoreError::Path(e.to_string()))?;
        Ok(Arc::new(PrefixStore::new(store, prefix_path)) as Arc<dyn ObjectStore>)
    }
}

/// Compute the object key (path within the store) for a given full destination path.
///
/// For local storage: strips base_directory prefix.
/// For remote storage: extracts key portion from URL.
pub fn object_key_for_path(storage: &Storage, full_path: &str) -> Result<ObjectStorePath> {
    let trimmed = full_path.trim();

    // Check if it's a remote URL
    for scheme in ["s3://", "gs://", "gcs://", "az://", "azure://"] {
        if let Some(rest) = trimmed.strip_prefix(scheme) {
            // Skip bucket/container, extract key
            let key = match rest.split_once('/') {
                Some((_, k)) => k.trim_matches('/'),
                None => "",
            };
            return ObjectStorePath::parse(key).map_err(|e| FilestoreError::Path(e.to_string()));
        }
    }

    // Local path: strip base_directory if present
    let base = storage.base_directory.trim().trim_end_matches('/');
    let path = trimmed.trim_start_matches('/');

    let key = if !base.is_empty() && path.starts_with(base.trim_start_matches('/')) {
        path.strip_prefix(base.trim_start_matches('/'))
            .unwrap_or(path)
            .trim_start_matches('/')
    } else {
        path
    };

    ObjectStorePath::parse(key).map_err(|e| FilestoreError::Path(e.to_string()))
}

/// Check if a path is a remote URL.
pub fn is_remote_url(path: &str) -> bool {
    let trimmed = path.trim();
    ["s3://", "gs://", "gcs://", "az://", "azure://"]
        .iter()
        .any(|s| trimmed.starts_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remote_url_s3() {
        let (bucket, prefix) = parse_remote_url("s3://my-bucket/some/prefix", &["s3://"]).unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "some/prefix");
    }

    #[test]
    fn test_parse_remote_url_no_prefix() {
        let (bucket, prefix) = parse_remote_url("s3://my-bucket", &["s3://"]).unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "");
    }

    #[test]
    fn test_is_remote_url() {
        assert!(is_remote_url("s3://bucket/key"));
        assert!(is_remote_url("gs://bucket/key"));
        assert!(is_remote_url("az://container/key"));
        assert!(!is_remote_url("/var/data/storage"));
        assert!(!is_remote_url("./relative/path"));
    }

    #[test]
    fn test_is_remote_url_gcs_variants() {
        assert!(is_remote_url("gcs://bucket/key"));
        assert!(is_remote_url("gs://bucket/key"));
    }

    #[test]
    fn test_is_remote_url_azure_variants() {
        assert!(is_remote_url("azure://container/key"));
        assert!(is_remote_url("az://container/key"));
    }

    #[test]
    fn test_build_object_store_filesystem() {
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;
        use std::env;

        let temp_dir = env::temp_dir().join("kalamdb_test_build_store");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_build"),
            storage_name: "test_build".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: temp_dir.to_string_lossy().to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let result = build_object_store(&storage);
        assert!(result.is_ok(), "Should build filesystem store");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_object_key_for_path_filesystem() {
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_key"),
            storage_name: "test_key".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: "/tmp/kalamdb".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let result = object_key_for_path(&storage, "namespace1/table1/file.parquet");
        assert!(result.is_ok());

        let key = result.unwrap();
        assert_eq!(key.as_ref(), "namespace1/table1/file.parquet");
    }

    #[test]
    fn test_object_key_for_path_strips_leading_slash() {
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_slash"),
            storage_name: "test_slash".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: "/tmp/kalamdb".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let result = object_key_for_path(&storage, "/namespace1/table1/file.parquet");
        assert!(result.is_ok());

        let key = result.unwrap();
        // Should strip leading slash
        assert_eq!(key.as_ref(), "namespace1/table1/file.parquet");
    }

    #[test]
    fn test_object_key_for_path_empty() {
        use kalamdb_commons::models::ids::StorageId;
        use kalamdb_commons::models::storage::StorageType;
        use kalamdb_commons::models::types::Storage;

        let now = chrono::Utc::now().timestamp_millis();
        let storage = Storage {
            storage_id: StorageId::from("test_empty"),
            storage_name: "test_empty".to_string(),
            description: None,
            storage_type: StorageType::Filesystem,
            base_directory: "/tmp/kalamdb".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{table}".to_string(),
            user_tables_template: "{namespace}/{user}/{table}".to_string(),
            created_at: now,
            updated_at: now,
        };

        let result = object_key_for_path(&storage, "");
        assert!(result.is_ok());

        let key = result.unwrap();
        assert_eq!(key.as_ref(), "");
    }

    #[test]
    fn test_parse_remote_url_with_trailing_slash() {
        let (bucket, prefix) = parse_remote_url("s3://my-bucket/prefix/", &["s3://"]).unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "prefix/");
    }

    #[test]
    fn test_parse_remote_url_deep_prefix() {
        let (bucket, prefix) = parse_remote_url("s3://my-bucket/a/b/c/d/e", &["s3://"]).unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(prefix, "a/b/c/d/e");
    }

    #[test]
    fn test_parse_remote_url_invalid_scheme() {
        let result = parse_remote_url("http://bucket/key", &["s3://", "gs://"]);
        assert!(result.is_err(), "Should error on invalid scheme");
    }

    #[test]
    fn test_is_remote_url_with_spaces() {
        // is_remote_url trims internally
        assert!(is_remote_url("  s3://bucket "), "Should handle spaces");
        assert!(is_remote_url(" s3://bucket/key "));
    }
}
