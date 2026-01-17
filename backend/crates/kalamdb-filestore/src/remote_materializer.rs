//! Remote Parquet file materialization.
//!
//! Downloads remote Parquet files to local temp directory for scanning.
//! Uses object_store async streaming APIs.

use crate::error::{FilestoreError, Result};
use crate::file_handle_diagnostics::{record_close, record_open};
use crate::object_store_factory::{build_object_store, is_remote_url};
use bytes::Bytes;
use futures_util::StreamExt;
use kalamdb_commons::system::Storage;
use object_store::path::Path as ObjectStorePath;
use object_store::ObjectStore;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Materialize remote Parquet files to a local temp directory.
///
/// This enables the existing filesystem-based Parquet scanner to work with
/// remote storage backends transparently.
///
/// Returns the local temp directory containing the downloaded files.
pub async fn materialize_remote_parquet_dir(
    storage: &Storage,
    remote_prefix: &str,
) -> Result<PathBuf> {
    if !is_remote_url(remote_prefix) {
        return Err(FilestoreError::Config(
            "materialize_remote_parquet_dir requires a remote URL".into(),
        ));
    }

    let store = build_object_store(storage)?;
    let target_dir = make_temp_dir(remote_prefix).await?;

    // Extract the key prefix from the URL
    let prefix = extract_key_prefix(remote_prefix);
    let prefix_path = if prefix.is_empty() {
        None
    } else {
        Some(ObjectStorePath::parse(&prefix).map_err(|e| FilestoreError::Path(e.to_string()))?)
    };

    // List objects with streaming
    let mut stream = store.list(prefix_path.as_ref());
    let mut downloaded = 0;

    while let Some(result) = stream.next().await {
        let meta = result.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
        let location = meta.location;

        // Only download .parquet files
        if !location.as_ref().ends_with(".parquet") {
            continue;
        }

        // Compute relative path within temp dir
        let rel_path = compute_relative_path(&prefix, location.as_ref());
        let local_path = target_dir.join(&rel_path);

        // Download file using streaming
        download_file_streaming(Arc::clone(&store), &location, &local_path).await?;
        downloaded += 1;
    }

    log::debug!(
        "Materialized {} parquet files from {} to {}",
        downloaded,
        remote_prefix,
        target_dir.display()
    );

    Ok(target_dir)
}

/// Synchronous wrapper for non-async contexts.
pub fn materialize_remote_parquet_dir_sync(
    storage: &Storage,
    remote_prefix: &str,
) -> Result<PathBuf> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(materialize_remote_parquet_dir(storage, remote_prefix)))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(materialize_remote_parquet_dir(storage, remote_prefix))
    }
}

/// Download a single file using streaming to avoid loading entire file in memory.
async fn download_file_streaming(
    store: Arc<dyn ObjectStore>,
    key: &ObjectStorePath,
    local_path: &PathBuf,
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).await.map_err(FilestoreError::Io)?;
    }

    // Get object as stream
    let result = store.get(key).await.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;

    // Stream to local file
    record_open("remote_materializer", local_path);

    let mut file = fs::File::create(local_path).await.map_err(FilestoreError::Io)?;

    let mut stream = result.into_stream();
    while let Some(chunk) = stream.next().await {
        let bytes: Bytes = chunk.map_err(|e| FilestoreError::ObjectStore(e.to_string()))?;
        file.write_all(&bytes).await.map_err(FilestoreError::Io)?;
    }

    file.flush().await.map_err(FilestoreError::Io)?;
    drop(file); // Explicitly close file
    record_close("remote_materializer", local_path);

    Ok(())
}

/// Create a temp directory for materialized files.
async fn make_temp_dir(remote_prefix: &str) -> Result<PathBuf> {
    let ts = chrono::Utc::now().timestamp_millis();
    let mut dir = std::env::temp_dir();
    dir.push("kalamdb");
    dir.push("parquet_cache");

    // Create a readable hint from the URL
    let hint: String =
        remote_prefix.replace("://", "_").replace('/', "_").chars().take(60).collect();

    dir.push(format!("{}-{}", ts, hint));

    fs::create_dir_all(&dir).await.map_err(FilestoreError::Io)?;

    Ok(dir)
}

/// Extract key prefix from a remote URL.
fn extract_key_prefix(url: &str) -> String {
    let trimmed = url.trim();

    for scheme in ["s3://", "gs://", "gcs://", "az://", "azure://"] {
        if let Some(rest) = trimmed.strip_prefix(scheme) {
            return match rest.split_once('/') {
                Some((_, prefix)) => prefix.trim_matches('/').to_string(),
                None => String::new(),
            };
        }
    }

    String::new()
}

/// Compute relative path within the temp directory.
fn compute_relative_path(prefix: &str, full_key: &str) -> String {
    let prefix = prefix.trim_matches('/');
    let full_key = full_key.trim_matches('/');

    if prefix.is_empty() {
        full_key.to_string()
    } else if let Some(rest) = full_key.strip_prefix(prefix) {
        rest.trim_start_matches('/').to_string()
    } else {
        full_key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_key_prefix() {
        assert_eq!(extract_key_prefix("s3://bucket/some/prefix"), "some/prefix");
        assert_eq!(extract_key_prefix("s3://bucket"), "");
        assert_eq!(extract_key_prefix("gs://bucket/key"), "key");
    }

    #[test]
    fn test_compute_relative_path() {
        assert_eq!(
            compute_relative_path("some/prefix", "some/prefix/file.parquet"),
            "file.parquet"
        );
        assert_eq!(compute_relative_path("", "file.parquet"), "file.parquet");
    }
}
