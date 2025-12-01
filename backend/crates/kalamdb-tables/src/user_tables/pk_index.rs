//! Primary Key Index for User Tables
//!
//! This module provides a secondary index on the primary key field of user tables,
//! enabling efficient lookup of rows by their PK value without scanning all rows.
//!
//! ## Index Key Format
//!
//! The index key format is: `{user_id}:{pk_value_encoded}:{seq}`
//!
//! - `user_id`: variable - user ID string bytes (for user isolation - same ID can exist for different users)
//! - `:`: 1 byte separator
//! - `pk_value_encoded`: variable - PK value encoded as string bytes  
//! - `:`: 1 byte separator
//! - `seq`: 8 bytes - sequence ID in big-endian (for MVCC version ordering)
//!
//! ## Example
//!
//! For user "alice" with id=42 and seq=1000:
//! Index key: `alice:42:0000000000001000` (seq as 8-byte BE)
//!
//! ## Prefix Scanning
//!
//! To find all versions of a row with a given PK:
//! 1. Build prefix: `{user_id}:{pk_value_encoded}:`
//! 2. Scan all keys with that prefix
//! 3. Results are ordered by seq (big-endian ensures lexicographic = numeric order)

use datafusion::scalar::ScalarValue;
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_store::IndexDefinition;

use super::UserTableRow;

/// Index for querying user table rows by primary key value.
///
/// Key format: `{user_id}:{pk_value_encoded}:{seq_be_8bytes}`
///
/// This index allows efficient lookups by PK value within a user's scope,
/// returning all MVCC versions of rows with matching PK.
/// The user_id prefix ensures the same PK value can exist for different users.
pub struct UserTablePkIndex {
    /// Partition name for the index
    partition: String,
    /// Name of the primary key field (e.g., "id", "user_id", etc.)
    pk_field_name: String,
}

impl UserTablePkIndex {
    /// Create a new PK index for a user table.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `pk_field_name` - Name of the primary key column
    pub fn new(namespace_id: &str, table_name: &str, pk_field_name: &str) -> Self {
        let partition = format!("user_{}:{}_pk_idx", namespace_id, table_name);
        Self {
            partition,
            pk_field_name: pk_field_name.to_string(),
        }
    }

    /// Encode a PK value to bytes for index key
    fn encode_pk_value(value: &ScalarValue) -> Vec<u8> {
        match value {
            ScalarValue::Int64(Some(n)) => n.to_string().into_bytes(),
            ScalarValue::Int32(Some(n)) => n.to_string().into_bytes(),
            ScalarValue::Int16(Some(n)) => n.to_string().into_bytes(),
            ScalarValue::UInt64(Some(n)) => n.to_string().into_bytes(),
            ScalarValue::UInt32(Some(n)) => n.to_string().into_bytes(),
            ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => s.as_bytes().to_vec(),
            // For other types, convert to string
            _ => value.to_string().into_bytes(),
        }
    }

    /// Build a prefix for scanning all versions of a PK for a specific user.
    ///
    /// Returns: `{user_id}:{pk_value_encoded}:`
    pub fn build_prefix_for_pk(&self, user_id: &str, pk_value: &ScalarValue) -> Vec<u8> {
        let pk_bytes = Self::encode_pk_value(pk_value);
        let mut prefix = Vec::with_capacity(user_id.len() + 1 + pk_bytes.len() + 1);
        prefix.extend_from_slice(user_id.as_bytes());
        prefix.push(b':');
        prefix.extend_from_slice(&pk_bytes);
        prefix.push(b':');
        prefix
    }
}

impl IndexDefinition<UserTableRowId, UserTableRow> for UserTablePkIndex {
    fn partition(&self) -> &str {
        &self.partition
    }

    fn indexed_columns(&self) -> Vec<&str> {
        vec![&self.pk_field_name]
    }

    fn extract_key(&self, primary_key: &UserTableRowId, entity: &UserTableRow) -> Option<Vec<u8>> {
        // Get the PK field value from the row
        let pk_value = entity.fields.get(&self.pk_field_name)?;

        // Build key: {user_id}:{pk_value_encoded}:{seq_be_8bytes}
        let user_id_bytes = primary_key.user_id.as_str().as_bytes();
        let pk_bytes = Self::encode_pk_value(pk_value);
        let seq_bytes = primary_key.seq.to_bytes(); // 8 bytes big-endian

        let mut key = Vec::with_capacity(user_id_bytes.len() + 1 + pk_bytes.len() + 1 + 8);
        key.extend_from_slice(user_id_bytes);
        key.push(b':');
        key.extend_from_slice(&pk_bytes);
        key.push(b':');
        key.extend_from_slice(&seq_bytes);
        Some(key)
    }

    fn filter_to_prefix(&self, filter: &datafusion::logical_expr::Expr) -> Option<Vec<u8>> {
        use kalamdb_store::extract_i64_equality;
        use kalamdb_store::extract_string_equality;

        // Try to extract equality filter on PK column
        // Note: We can only return the pk_value part; caller must prepend user_id
        if let Some((col, val)) = extract_string_equality(filter) {
            if col == self.pk_field_name {
                let pk_value = ScalarValue::Utf8(Some(val.to_string()));
                return Some(Self::encode_pk_value(&pk_value));
            }
        }

        if let Some((col, val)) = extract_i64_equality(filter) {
            if col == self.pk_field_name {
                let pk_value = ScalarValue::Int64(Some(val));
                return Some(Self::encode_pk_value(&pk_value));
            }
        }

        None
    }
}

/// Create a PK index for a user table.
///
/// # Arguments
/// * `namespace_id` - Namespace identifier
/// * `table_name` - Table name  
/// * `pk_field_name` - Name of the primary key column
pub fn create_user_table_pk_index(
    namespace_id: &str,
    table_name: &str,
    pk_field_name: &str,
) -> std::sync::Arc<dyn IndexDefinition<UserTableRowId, UserTableRow>> {
    std::sync::Arc::new(UserTablePkIndex::new(namespace_id, table_name, pk_field_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::scalar::ScalarValue;
    use kalamdb_commons::ids::SeqId;
    use kalamdb_commons::models::row::Row;
    use kalamdb_commons::models::UserId;
    use std::collections::BTreeMap;

    fn create_test_row(user_id: &str, seq: i64, id_value: i64) -> (UserTableRowId, UserTableRow) {
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), ScalarValue::Int64(Some(id_value)));
        values.insert(
            "name".to_string(),
            ScalarValue::Utf8(Some("Test".to_string())),
        );

        let key = UserTableRowId::new(UserId::new(user_id), SeqId::new(seq));
        let row = UserTableRow {
            user_id: UserId::new(user_id),
            _seq: SeqId::new(seq),
            _deleted: false,
            fields: Row::new(values),
        };
        (key, row)
    }

    #[test]
    fn test_pk_index_extract_key() {
        let index = UserTablePkIndex::new("default", "users", "id");
        let (key, row) = create_test_row("user1", 100, 42);

        let index_key = index.extract_key(&key, &row);
        assert!(index_key.is_some());

        let index_key = index_key.unwrap();
        // Format: "user1:42:" + 8 bytes seq
        // user1 = 5 bytes, : = 1, 42 = 2 bytes, : = 1, seq = 8 bytes
        // Total: 5 + 1 + 2 + 1 + 8 = 17
        assert_eq!(index_key.len(), 17);

        // Verify format: starts with "user1:42:"
        let prefix_str = std::str::from_utf8(&index_key[..9]).unwrap();
        assert_eq!(prefix_str, "user1:42:");
    }

    #[test]
    fn test_pk_index_same_pk_different_versions() {
        let index = UserTablePkIndex::new("default", "users", "id");

        // Two versions of the same row (same PK, different seq)
        let (key1, row1) = create_test_row("user1", 100, 42);
        let (key2, row2) = create_test_row("user1", 200, 42);

        let index_key1 = index.extract_key(&key1, &row1).unwrap();
        let index_key2 = index.extract_key(&key2, &row2).unwrap();

        // Same user_id and pk_value, different seq
        // Prefix "user1:42:" = 9 bytes should be the same
        assert_eq!(&index_key1[..9], &index_key2[..9]);

        // Last 8 bytes (seq) should be different
        assert_ne!(&index_key1[9..], &index_key2[9..]);
    }

    #[test]
    fn test_pk_index_same_pk_different_users() {
        let index = UserTablePkIndex::new("default", "users", "id");

        // Same PK value for different users
        let (key1, row1) = create_test_row("alice", 100, 42);
        let (key2, row2) = create_test_row("bob", 100, 42);

        let index_key1 = index.extract_key(&key1, &row1).unwrap();
        let index_key2 = index.extract_key(&key2, &row2).unwrap();

        // Different user_id prefix - keys should be completely different
        assert_ne!(index_key1, index_key2);

        // alice:42: vs bob:42:
        let prefix1 = std::str::from_utf8(&index_key1[..9]).unwrap();
        let prefix2 = std::str::from_utf8(&index_key2[..7]).unwrap();
        assert_eq!(prefix1, "alice:42:");
        assert_eq!(prefix2, "bob:42:");
    }

    #[test]
    fn test_pk_index_different_pk_values() {
        let index = UserTablePkIndex::new("default", "users", "id");

        let (key1, row1) = create_test_row("user1", 100, 42);
        let (key2, row2) = create_test_row("user1", 100, 99);

        let index_key1 = index.extract_key(&key1, &row1).unwrap();
        let index_key2 = index.extract_key(&key2, &row2).unwrap();

        // Same user_id but different pk values
        // user1: is same (6 bytes), then pk values differ
        assert_eq!(&index_key1[..6], &index_key2[..6]);
        assert_ne!(index_key1, index_key2);
    }

    #[test]
    fn test_build_prefix_for_pk() {
        let index = UserTablePkIndex::new("default", "users", "id");
        let pk_value = ScalarValue::Int64(Some(42));

        let prefix = index.build_prefix_for_pk("user1", &pk_value);

        // Should be: "user1:42:" = 9 bytes
        assert_eq!(prefix.len(), 9);
        let prefix_str = std::str::from_utf8(&prefix).unwrap();
        assert_eq!(prefix_str, "user1:42:");
    }

    #[test]
    fn test_partition_name() {
        let index = UserTablePkIndex::new("my_namespace", "my_table", "id");
        assert_eq!(index.partition(), "user_my_namespace:my_table_pk_idx");
    }
}
