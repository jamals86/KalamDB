use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Enum representing the type of storage backend in KalamDB.
///
/// - Filesystem: Local or network filesystem storage
/// - S3: Amazon S3 or S3-compatible object storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StorageType {
    /// Local or network filesystem storage
    Filesystem,
    /// Amazon S3 or S3-compatible object storage
    S3,
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageType::Filesystem => write!(f, "filesystem"),
            StorageType::S3 => write!(f, "s3"),
        }
    }
}

impl From<&str> for StorageType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "s3" => StorageType::S3,
            _ => StorageType::Filesystem,
        }
    }
}

impl From<String> for StorageType {
    fn from(s: String) -> Self {
        StorageType::from(s.as_str())
    }
}
