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
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct Namespace {
    pub namespace_id: NamespaceId,
    pub name: String,
    pub created_at: i64,         // Unix timestamp in milliseconds
    pub options: Option<String>, // JSON configuration
    pub table_count: i32,
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
    /// Name must match regex: ^[a-z][a-z0-9_]*$ (lowercase, start with letter)
    /// Name cannot be "system" (reserved)
    ///
    /// # Example
    /// ```
    /// use kalamdb_commons::types::Namespace;
    ///
    /// assert!(Namespace::validate_name("app").is_ok());
    /// assert!(Namespace::validate_name("analytics_db").is_ok());
    /// assert!(Namespace::validate_name("system").is_err());
    /// assert!(Namespace::validate_name("Invalid").is_err());
    /// ```
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name == "system" {
            return Err("Namespace name 'system' is reserved".to_string());
        }

        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err("Namespace name must start with a lowercase letter".to_string());
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Namespace name can only contain lowercase letters, digits, and underscores"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Check if this namespace can be deleted (has no tables)
    pub fn can_delete(&self) -> bool {
        self.table_count == 0
    }

    /// Increment the table count
    pub fn increment_table_count(&mut self) {
        self.table_count += 1;
    }

    /// Decrement the table count
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
