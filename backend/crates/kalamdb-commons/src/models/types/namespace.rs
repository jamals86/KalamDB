//! Namespace entity for system.namespaces table.

use crate::models::ids::NamespaceId;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Namespace entity for system.namespaces table.
///
/// Represents a database namespace for data isolation.
///
/// ## Fields
/// - `namespace_id`: Unique namespace identifier
/// - `name`: Namespace name (e.g., "default", "production")
/// - `created_at`: Unix timestamp in milliseconds when namespace was created
/// - `options`: Optional JSON configuration
/// - `table_count`: Number of tables in this namespace
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::types::Namespace;
/// use kalamdb_commons::NamespaceId;
///
/// let namespace = Namespace {
///     namespace_id: NamespaceId::new("default"),
///     name: "default".to_string(),
///     created_at: 1730000000000,
///     options: Some("{}".to_string()),
///     table_count: 0,
/// };
/// ```
/// Namespace struct with fields ordered for optimal memory alignment.
/// 8-byte aligned fields first (i64, String types), then smaller types.
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Namespace {
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub namespace_id: NamespaceId,
    pub name: String,
    pub options: Option<String>, // JSON configuration
    pub table_count: i32,        //TODO: Remove this field and calculate on the fly
}

impl Namespace {
    /// Create a new namespace with default values
    ///
    /// # Arguments
    /// * `name` - Namespace identifier
    ///
    /// # Example
    /// ```
    /// use kalamdb_commons::types::Namespace;
    ///
    /// let namespace = Namespace::new("app");
    /// assert_eq!(namespace.name, "app");
    /// assert_eq!(namespace.table_count, 0);
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        let name_str = name.into();
        Self {
            namespace_id: NamespaceId::new(&name_str),
            name: name_str,
            created_at: chrono::Utc::now().timestamp_millis(),
            options: Some("{}".to_string()),
            table_count: 0,
        }
    }

    /// Validate namespace name format
    ///
    /// Namespace names must:
    /// - Not be empty
    /// - Not exceed 64 characters
    /// - Only contain alphanumeric characters and underscores
    /// - Not start with an underscore or number
    /// - Not be a reserved namespace name (system, kalamdb, etc.)
    /// - Not be a reserved SQL keyword
    ///
    /// # Example
    /// ```
    /// use kalamdb_commons::types::Namespace;
    ///
    /// assert!(Namespace::validate_name("app").is_ok());
    /// assert!(Namespace::validate_name("analytics_db").is_ok());
    /// assert!(Namespace::validate_name("MyNamespace").is_ok());
    /// assert!(Namespace::validate_name("system").is_err());  // reserved
    /// assert!(Namespace::validate_name("kalamdb").is_err()); // reserved
    /// assert!(Namespace::validate_name("_private").is_err()); // starts with _
    /// ```
    pub fn validate_name(name: &str) -> Result<(), String> {
        crate::validation::validate_namespace_name(name).map_err(|e| e.to_string())
    }

    /// Check if this namespace can be deleted (has no tables)
    #[inline]
    pub fn can_delete(&self) -> bool {
        self.table_count == 0
    }

    /// Increment the table count
    #[inline]
    pub fn increment_table_count(&mut self) {
        self.table_count += 1;
    }

    /// Decrement the table count
    #[inline]
    pub fn decrement_table_count(&mut self) {
        if self.table_count > 0 {
            self.table_count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_serialization() {
        let namespace = Namespace {
            namespace_id: NamespaceId::new("default"),
            name: "default".to_string(),
            created_at: 1730000000000,
            options: Some("{}".to_string()),
            table_count: 0,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&namespace, config).unwrap();
        let (deserialized, _): (Namespace, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(namespace, deserialized);
    }
}
