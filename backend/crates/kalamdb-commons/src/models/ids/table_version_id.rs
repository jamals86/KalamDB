//! Table version identifier for schema versioning
//!
//! This module provides `TableVersionId` - a composite key that combines a TableId
//! with a version number for storing and retrieving specific schema versions.
//!
//! Key format:
//! - Latest pointer: "{namespace}:{table}<lat>" -> points to current version number
//! - Versioned:      "{namespace}:{table}<ver>{version:08}" -> full TableDefinition
//!
//! The zero-padded version (8 digits) ensures lexicographic ordering for range scans.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::table_id::TableId;
use crate::StorageKey;

/// Marker for the "latest" version pointer
pub const LATEST_MARKER: &str = "<lat>";

/// Marker for versioned entries
pub const VERSION_MARKER: &str = "<ver>";

/// Composite key for versioned table schema storage
///
/// Supports two key types:
/// 1. **Latest pointer** (version = None): Points to current version number
/// 2. **Versioned entry** (version = Some(N)): Full TableDefinition at version N
///
/// # Storage Key Format
///
/// ```text
/// Latest:    "default:users<lat>"
/// Versioned: "default:users<ver>00000001"
/// ```
///
/// The 8-digit zero-padded version ensures lexicographic ordering for efficient
/// range scans when listing all versions of a table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct TableVersionId {
    /// The base table identifier
    table_id: TableId,

    /// Version number: None = latest pointer, Some(N) = specific version
    version: Option<u32>,
}

impl TableVersionId {
    /// Create a latest pointer key (for looking up current version)
    ///
    /// # Example
    /// ```rust,ignore
    /// let table_id = TableId::from_strings("default", "users");
    /// let latest = TableVersionId::latest(table_id);
    /// // Storage key: "default:users<lat>"
    /// ```
    pub fn latest(table_id: TableId) -> Self {
        Self {
            table_id,
            version: None,
        }
    }

    /// Create a versioned key (for storing/retrieving specific version)
    ///
    /// # Example
    /// ```rust,ignore
    /// let table_id = TableId::from_strings("default", "users");
    /// let v1 = TableVersionId::versioned(table_id, 1);
    /// // Storage key: "default:users<ver>00000001"
    /// ```
    pub fn versioned(table_id: TableId, version: u32) -> Self {
        Self {
            table_id,
            version: Some(version),
        }
    }

    /// Get the base table ID
    pub fn table_id(&self) -> &TableId {
        &self.table_id
    }

    /// Get the version number (None for latest pointer)
    pub fn version(&self) -> Option<u32> {
        self.version
    }

    /// Check if this is a latest pointer
    pub fn is_latest(&self) -> bool {
        self.version.is_none()
    }

    /// Check if this is a versioned entry
    pub fn is_versioned(&self) -> bool {
        self.version.is_some()
    }

    /// Consume and return inner components
    pub fn into_parts(self) -> (TableId, Option<u32>) {
        (self.table_id, self.version)
    }

    /// Create prefix for scanning all versions of a table
    ///
    /// Returns prefix that matches all versioned entries for a table.
    /// Use with range scan to list all versions.
    ///
    /// # Example
    /// ```rust,ignore
    /// let table_id = TableId::from_strings("default", "users");
    /// let prefix = TableVersionId::version_scan_prefix(&table_id);
    /// // Returns: b"default:users<ver>"
    /// ```
    pub fn version_scan_prefix(table_id: &TableId) -> Vec<u8> {
        let mut key = table_id.as_storage_key();
        key.extend_from_slice(VERSION_MARKER.as_bytes());
        key
    }

    /// Format as bytes for storage
    ///
    /// - Latest: "{namespace}:{table}<lat>"
    /// - Versioned: "{namespace}:{table}<ver>{version:08}"
    pub fn as_storage_key(&self) -> Vec<u8> {
        let mut key = self.table_id.as_storage_key();

        match self.version {
            None => {
                key.extend_from_slice(LATEST_MARKER.as_bytes());
            },
            Some(v) => {
                key.extend_from_slice(VERSION_MARKER.as_bytes());
                // Zero-pad to 8 digits for lexicographic ordering
                key.extend_from_slice(format!("{:08}", v).as_bytes());
            },
        }

        key
    }

    /// Parse from storage key bytes
    ///
    /// Handles both latest pointer and versioned formats.
    pub fn from_storage_key(key: &[u8]) -> Option<Self> {
        let key_str = std::str::from_utf8(key).ok()?;

        // Check for latest marker
        if let Some(base) = key_str.strip_suffix(LATEST_MARKER) {
            let table_id = TableId::from_storage_key(base.as_bytes())?;
            return Some(Self::latest(table_id));
        }

        // Check for version marker
        if let Some(pos) = key_str.find(VERSION_MARKER) {
            let base = &key_str[..pos];
            let version_str = &key_str[pos + VERSION_MARKER.len()..];

            let table_id = TableId::from_storage_key(base.as_bytes())?;
            let version = version_str.parse::<u32>().ok()?;

            return Some(Self::versioned(table_id, version));
        }

        None
    }
}

impl fmt::Display for TableVersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.version {
            None => write!(f, "{}<lat>", self.table_id),
            Some(v) => write!(f, "{}<ver>{:08}", self.table_id, v),
        }
    }
}

impl StorageKey for TableVersionId {
    fn storage_key(&self) -> Vec<u8> {
        self.as_storage_key()
    }

    fn from_storage_key(bytes: &[u8]) -> Result<Self, String> {
        Self::from_storage_key(bytes).ok_or_else(|| "Invalid TableVersionId format".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ids::NamespaceId;
    use crate::models::table_name::TableName;

    fn test_table_id() -> TableId {
        TableId::new(NamespaceId::new("default"), TableName::new("users"))
    }

    #[test]
    fn test_latest_key() {
        let latest = TableVersionId::latest(test_table_id());
        assert!(latest.is_latest());
        assert!(!latest.is_versioned());
        assert_eq!(latest.version(), None);

        let key = latest.as_storage_key();
        assert_eq!(key, b"default:users<lat>");
    }

    #[test]
    fn test_versioned_key() {
        let v1 = TableVersionId::versioned(test_table_id(), 1);
        assert!(!v1.is_latest());
        assert!(v1.is_versioned());
        assert_eq!(v1.version(), Some(1));

        let key = v1.as_storage_key();
        assert_eq!(key, b"default:users<ver>00000001");
    }

    #[test]
    fn test_version_ordering() {
        // Test that lexicographic ordering matches numeric ordering
        let v1 = TableVersionId::versioned(test_table_id(), 1);
        let v2 = TableVersionId::versioned(test_table_id(), 2);
        let v10 = TableVersionId::versioned(test_table_id(), 10);
        let v100 = TableVersionId::versioned(test_table_id(), 100);

        let key1 = v1.as_storage_key();
        let key2 = v2.as_storage_key();
        let key10 = v10.as_storage_key();
        let key100 = v100.as_storage_key();

        assert!(key1 < key2);
        assert!(key2 < key10);
        assert!(key10 < key100);
    }

    #[test]
    fn test_from_storage_key_latest() {
        let original = TableVersionId::latest(test_table_id());
        let key = original.as_storage_key();
        let parsed = TableVersionId::from_storage_key(&key).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_from_storage_key_versioned() {
        let original = TableVersionId::versioned(test_table_id(), 42);
        let key = original.as_storage_key();
        let parsed = TableVersionId::from_storage_key(&key).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_version_scan_prefix() {
        let table_id = test_table_id();
        let prefix = TableVersionId::version_scan_prefix(&table_id);
        assert_eq!(prefix, b"default:users<ver>");

        // Test that prefix matches versioned keys
        let v1 = TableVersionId::versioned(table_id.clone(), 1);
        let v1_key = v1.as_storage_key();
        assert!(v1_key.starts_with(&prefix));
    }

    #[test]
    fn test_display() {
        let latest = TableVersionId::latest(test_table_id());
        assert_eq!(format!("{}", latest), "default:users<lat>");

        let v42 = TableVersionId::versioned(test_table_id(), 42);
        assert_eq!(format!("{}", v42), "default:users<ver>00000042");
    }

    #[test]
    fn test_into_parts() {
        let v5 = TableVersionId::versioned(test_table_id(), 5);
        let (table_id, version) = v5.into_parts();
        assert_eq!(table_id, test_table_id());
        assert_eq!(version, Some(5));
    }

    #[test]
    fn test_invalid_key_parsing() {
        // Invalid format should return None
        assert!(TableVersionId::from_storage_key(b"invalid").is_none());
        assert!(TableVersionId::from_storage_key(b"default:users").is_none());
        assert!(TableVersionId::from_storage_key(b"default:users<bad>").is_none());
    }
}
