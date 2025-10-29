// File: backend/crates/kalamdb-commons/src/models/table_id.rs
// Composite key for system.tables entries

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::namespace_id::NamespaceId;
use super::table_name::TableName;

/// Composite key for system.tables entries: {namespace_id}:{table_name}
/// 
/// This composite key provides type-safe access to table metadata,
/// ensuring namespace and table name are always paired correctly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct TableId {
    namespace_id: NamespaceId,
    table_name: TableName,
}

impl TableId {
    /// Create a new TableId from namespace ID and table name
    pub fn new(namespace_id: NamespaceId, table_name: TableName) -> Self {
        Self {
            namespace_id,
            table_name,
        }
    }

    /// Get the namespace ID component
    pub fn namespace_id(&self) -> &NamespaceId {
        &self.namespace_id
    }

    /// Get the table name component
    pub fn table_name(&self) -> &TableName {
        &self.table_name
    }

    /// Create from string components
    pub fn from_strings(namespace_id: &str, table_name: &str) -> Self {
        Self {
            namespace_id: NamespaceId::new(namespace_id),
            table_name: TableName::new(table_name),
        }
    }

    /// Format as bytes for storage: "{namespace_id}:{table_name}"
    pub fn as_storage_key(&self) -> Vec<u8> {
        let mut key = Vec::new();
        key.extend_from_slice(self.namespace_id.as_str().as_bytes());
        key.push(b':');
        key.extend_from_slice(self.table_name.as_str().as_bytes());
        key
    }

    /// Parse from storage key format: "{namespace_id}:{table_name}"
    pub fn from_storage_key(key: &[u8]) -> Option<Self> {
        let key_str = std::str::from_utf8(key).ok()?;
        let pos = key_str.find(':')?;
        let namespace_id = &key_str[..pos];
        let table_name = &key_str[pos + 1..];
        
        Some(Self {
            namespace_id: NamespaceId::new(namespace_id),
            table_name: TableName::new(table_name),
        })
    }

    /// Consume and return inner components
    pub fn into_parts(self) -> (NamespaceId, TableName) {
        (self.namespace_id, self.table_name)
    }
}

impl AsRef<[u8]> for TableId {
    fn as_ref(&self) -> &[u8] {
        // Note: This creates a temporary allocation. For zero-copy access,
        // use as_storage_key() directly.
        // This implementation is primarily for trait compatibility.
        // In performance-critical paths, prefer as_storage_key().
        self.namespace_id.as_str().as_bytes()
    }
}

impl fmt::Display for TableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace_id, self.table_name)
    }
}

// Ensure Send and Sync are implemented
unsafe impl Send for TableId {}
unsafe impl Sync for TableId {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_id_new() {
        let namespace_id = NamespaceId::new("ns1");
        let table_name = TableName::new("users");
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        
        assert_eq!(table_id.namespace_id(), &namespace_id);
        assert_eq!(table_id.table_name(), &table_name);
    }

    #[test]
    fn test_table_id_from_strings() {
        let table_id = TableId::from_strings("ns1", "users");
        assert_eq!(table_id.namespace_id().as_str(), "ns1");
        assert_eq!(table_id.table_name().as_str(), "users");
    }

    #[test]
    fn test_table_id_as_storage_key() {
        let table_id = TableId::from_strings("ns1", "users");
        let key = table_id.as_storage_key();
        assert_eq!(key, b"ns1:users");
    }

    #[test]
    fn test_table_id_from_storage_key() {
        let key = b"ns1:users";
        let table_id = TableId::from_storage_key(key).unwrap();
        
        assert_eq!(table_id.namespace_id().as_str(), "ns1");
        assert_eq!(table_id.table_name().as_str(), "users");
    }

    #[test]
    fn test_table_id_roundtrip() {
        let original = TableId::from_strings("ns1", "users");
        let key = original.as_storage_key();
        let parsed = TableId::from_storage_key(&key).unwrap();
        
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_table_id_display() {
        let table_id = TableId::from_strings("ns1", "users");
        assert_eq!(format!("{}", table_id), "ns1:users");
    }

    #[test]
    fn test_table_id_serialization() {
        let table_id = TableId::from_strings("ns1", "users");
        let json = serde_json::to_string(&table_id).unwrap();
        let deserialized: TableId = serde_json::from_str(&json).unwrap();
        assert_eq!(table_id, deserialized);
    }

    #[test]
    fn test_table_id_into_parts() {
        let table_id = TableId::from_strings("ns1", "users");
        let (namespace_id, table_name) = table_id.into_parts();
        
        assert_eq!(namespace_id.as_str(), "ns1");
        assert_eq!(table_name.as_str(), "users");
    }

    #[test]
    fn test_table_id_with_special_chars() {
        let table_id = TableId::from_strings("my-namespace", "table_name");
        let key = table_id.as_storage_key();
        let parsed = TableId::from_storage_key(&key).unwrap();
        
        assert_eq!(table_id, parsed);
    }
}
