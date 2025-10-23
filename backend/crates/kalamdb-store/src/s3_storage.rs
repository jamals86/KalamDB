//! S3 storage backend for Parquet files
//!
//! Provides write and read operations for Parquet files stored in AWS S3.
//! This module implements the S3 storage backend for KalamDB's multi-storage architecture.

use anyhow::{anyhow, Context, Result};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use kalamdb_commons::models::StorageConfig;
use serde::Deserialize;
use std::path::Path;

/// S3 storage backend
///
/// Handles reading and writing Parquet files to AWS S3.
/// Uses the aws-sdk-s3 crate for S3 operations.
///
/// # Example
/// ```no_run
/// use kalamdb_store::s3_storage::S3Storage;
/// use aws_sdk_s3::Client;
///
/// # async fn example(client: Client) -> anyhow::Result<()> {
/// let storage = S3Storage::new(client, "my-bucket".to_string());
/// storage.write_parquet("path/to/file.parquet", &data).await?;
/// # Ok(())
/// # }
/// ```
pub struct S3Storage {
    client: S3Client,
    bucket: String,
    prefix: Option<String>,
    credentials: Option<S3Credentials>,
}

impl S3Storage {
    /// Create a new S3 storage backend
    ///
    /// # Arguments
    /// * `client` - AWS S3 client
    /// * `bucket` - S3 bucket name
    pub fn new(client: S3Client, bucket: String) -> Self {
        Self {
            client,
            bucket,
            prefix: None,
            credentials: None,
        }
    }

    /// Build storage backend from a storage configuration.
    pub fn from_storage_config(client: S3Client, config: &StorageConfig) -> Result<Self> {
        let base_directory = config.base_directory();
        let bucket = Self::parse_bucket_from_url(base_directory)
            .ok_or_else(|| anyhow!("Invalid S3 base_directory: {}", base_directory))?;
        let prefix = Self::parse_prefix_from_url(base_directory);
        let credentials = config
            .credentials()
            .map(S3Credentials::from_json)
            .transpose()?;

        Ok(Self {
            client,
            bucket,
            prefix,
            credentials,
        })
    }

    /// Write a Parquet file to S3 (T171b)
    ///
    /// # Arguments
    /// * `key` - S3 object key (path within bucket)
    /// * `data` - Parquet file data as bytes
    ///
    /// # Returns
    /// * `Ok(())` - File written successfully
    /// * `Err` - S3 operation failed
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_store::s3_storage::S3Storage;
    /// # use aws_sdk_s3::Client;
    /// # async fn example(storage: &S3Storage) -> anyhow::Result<()> {
    /// let data = vec![/* Parquet file bytes */];
    /// storage.write_parquet("namespace/table/2025-10-22T14-30-00.parquet", &data).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn write_parquet(&self, key: &str, data: &[u8]) -> Result<()> {
        let byte_stream = ByteStream::from(data.to_vec());

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(byte_stream)
            .content_type("application/vnd.apache.parquet")
            .send()
            .await
            .context(format!(
                "Failed to write Parquet file to S3: s3://{}/{}",
                self.bucket, key
            ))?;

        Ok(())
    }

    /// Read a Parquet file from S3 (T171c)
    ///
    /// # Arguments
    /// * `key` - S3 object key (path within bucket)
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Parquet file data
    /// * `Err` - S3 operation failed or file not found
    ///
    /// # Example
    /// ```no_run
    /// # use kalamdb_store::s3_storage::S3Storage;
    /// # use aws_sdk_s3::Client;
    /// # async fn example(storage: &S3Storage) -> anyhow::Result<()> {
    /// let data = storage.read_parquet("namespace/table/2025-10-22T14-30-00.parquet").await?;
    /// println!("Read {} bytes from S3", data.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_parquet(&self, key: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .context(format!(
                "Failed to read Parquet file from S3: s3://{}/{}",
                self.bucket, key
            ))?;

        let data = response
            .body
            .collect()
            .await
            .context("Failed to read S3 object body")?
            .into_bytes()
            .to_vec();

        Ok(data)
    }

    /// Return parsed credentials if available.
    pub fn credentials(&self) -> Option<&S3Credentials> {
        self.credentials.as_ref()
    }

    /// Extract bucket name from s3:// URL
    ///
    /// # Arguments
    /// * `base_directory` - S3 URL (e.g., "s3://my-bucket/path/")
    ///
    /// # Returns
    /// * `Some(bucket_name)` - Extracted bucket name
    /// * `None` - Invalid S3 URL format
    pub fn parse_bucket_from_url(base_directory: &str) -> Option<String> {
        if let Some(stripped) = base_directory.strip_prefix("s3://") {
            // Extract bucket name (everything before first '/')
            let bucket = stripped.split('/').next()?;
            Some(bucket.to_string())
        } else {
            None
        }
    }

    /// Extract S3 key prefix from s3:// URL
    ///
    /// # Arguments
    /// * `base_directory` - S3 URL (e.g., "s3://my-bucket/path/")
    ///
    /// # Returns
    /// * `Some(prefix)` - Extracted key prefix (e.g., "path/")
    /// * `None` - No prefix or invalid URL
    pub fn parse_prefix_from_url(base_directory: &str) -> Option<String> {
        if let Some(stripped) = base_directory.strip_prefix("s3://") {
            // Extract everything after bucket name
            let parts: Vec<&str> = stripped.splitn(2, '/').collect();
            if parts.len() > 1 {
                Some(parts[1].to_string())
            } else {
                Some(String::new())
            }
        } else {
            None
        }
    }
}

/// Parsed credential payload for S3 operations
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct S3Credentials {
    #[serde(alias = "accessKey", alias = "access_key")]
    pub access_key: String,
    #[serde(alias = "secretKey", alias = "secret_key")]
    pub secret_key: String,
    #[serde(default, alias = "sessionToken", alias = "session_token")]
    pub session_token: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

impl S3Credentials {
    pub fn from_json(raw: &str) -> Result<Self> {
        let creds: S3Credentials = serde_json::from_str(raw)
            .map_err(|e| anyhow!("Invalid S3 credentials JSON: {}", e))?;
        Ok(creds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credentials_from_json() {
        let json = r#"{"access_key":"AKIA","secret_key":"SECRET","session_token":"token","region":"us-west-2"}"#;
        let creds = S3Credentials::from_json(json).unwrap();
        assert_eq!(creds.access_key, "AKIA");
        assert_eq!(creds.secret_key, "SECRET");
        assert_eq!(creds.session_token, Some("token".to_string()));
        assert_eq!(creds.region, Some("us-west-2".to_string()));
    }

    #[test]
    fn test_parse_credentials_from_json_missing_optional() {
        let json = r#"{"access_key":"AKIA","secret_key":"SECRET"}"#;
        let creds = S3Credentials::from_json(json).unwrap();
        assert!(creds.session_token.is_none());
        assert!(creds.region.is_none());
    }

    #[test]
    fn test_parse_bucket_from_url() {
        assert_eq!(
            S3Storage::parse_bucket_from_url("s3://my-bucket/path/to/data"),
            Some("my-bucket".to_string())
        );
        assert_eq!(
            S3Storage::parse_bucket_from_url("s3://my-bucket"),
            Some("my-bucket".to_string())
        );
        assert_eq!(S3Storage::parse_bucket_from_url("/local/path"), None);
    }

    #[test]
    fn test_parse_prefix_from_url() {
        assert_eq!(
            S3Storage::parse_prefix_from_url("s3://my-bucket/path/to/data"),
            Some("path/to/data".to_string())
        );
        assert_eq!(
            S3Storage::parse_prefix_from_url("s3://my-bucket/"),
            Some(String::new())
        );
        assert_eq!(
            S3Storage::parse_prefix_from_url("s3://my-bucket"),
            Some(String::new())
        );
        assert_eq!(S3Storage::parse_prefix_from_url("/local/path"), None);
    }
}
