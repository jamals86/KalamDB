//! User table counter entity for flush tracking.

use crate::models::{ids::UserId, TableName};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// User table counter for flush tracking.
///
/// Tracks row counts per user table for flush decisions.
///
/// ## Fields
/// - `key`: Composite key (format: "{user_id}:{table_name}")
/// - `user_id`: User who owns the table
/// - `table_name`: Table name
/// - `row_count`: Current row count in RocksDB (unflushed)
/// - `last_flushed_at`: Optional Unix timestamp in milliseconds of last flush
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::types::UserTableCounter;
/// use kalamdb_commons::{UserId, TableName};
///
/// let counter = UserTableCounter {
///     key: "u_123:events".to_string(),
///     user_id: UserId::new("u_123"),
///     table_name: TableName::new("events"),
///     row_count: 1500,
///     last_flushed_at: Some(1730000000000),
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct UserTableCounter {
    pub key: String, // "{user_id}:{table_name}"
    pub user_id: UserId,
    pub table_name: TableName,
    pub row_count: u64,
    pub last_flushed_at: Option<i64>, // Unix timestamp in milliseconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_table_counter_serialization() {
        let counter = UserTableCounter {
            key: "u_123:events".to_string(),
            user_id: UserId::new("u_123"),
            table_name: TableName::new("events"),
            row_count: 1500,
            last_flushed_at: Some(1730000000000),
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&counter, config).unwrap();
        let (deserialized, _): (UserTableCounter, _) =
            bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(counter, deserialized);
    }
}
