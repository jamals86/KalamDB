//! Storage key generation for MVCC DML operations
//!
//! This module implements generate_storage_key(), which creates the appropriate
//! storage key format for user and shared tables.

use kalamdb_commons::ids::{SeqId, SharedTableRowId, UserTableRowId};
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::UserId;
use kalamdb_commons::StorageKey;


/// Storage key type enum
///
/// Used to return different key types from generate_storage_key()
pub enum StorageKeyType {
    User(UserTableRowId),
    Shared(SharedTableRowId),
}

impl StorageKeyType {
    /// Get the underlying storage key bytes
    pub fn storage_key_bytes(&self) -> Vec<u8> {
        match self {
            StorageKeyType::User(key) => key.storage_key(),
            StorageKeyType::Shared(key) => key.storage_key(),
        }
    }
}

/// Generate storage key for MVCC DML operations
///
/// **MVCC Architecture**: Different table types use different key formats:
/// - User tables: UserTableRowId with {user_id}:{_seq} format
/// - Shared tables: SeqId directly (no wrapper)
///
/// # Arguments
/// * `table_type` - User or Shared table type
/// * `user_id` - User ID (required for user tables, ignored for shared tables)
/// * `seq` - Sequence ID for this version
///
/// # Returns
/// * `StorageKeyType` - Appropriate key type for the table
pub fn generate_storage_key(
    table_type: TableType,
    user_id: Option<UserId>,
    seq: SeqId,
) -> StorageKeyType {
    match table_type {
        TableType::User => {
            let user_id = user_id.expect("user_id required for user tables");
            StorageKeyType::User(UserTableRowId::new(user_id, seq))
        }
        TableType::Shared => StorageKeyType::Shared(seq),
        _ => panic!("Unsupported table type for generate_storage_key"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_storage_key_user_table() {
        let user_id = UserId::new("user1");
        let seq = SeqId::new(12345);

        let key = generate_storage_key(TableType::User, Some(user_id.clone()), seq);

        match key {
            StorageKeyType::User(user_key) => {
                assert_eq!(user_key.user_id(), &user_id);
                assert_eq!(user_key.seq(), seq);

                // Verify storage key format
                let bytes = user_key.storage_key();
                assert_eq!(bytes[0], 5); // "user1" = 5 bytes
            }
            _ => panic!("Expected User key"),
        }
    }

    #[test]
    fn test_generate_storage_key_shared_table() {
        let seq = SeqId::new(67890);

        let key = generate_storage_key(TableType::Shared, None, seq);

        match key {
            StorageKeyType::Shared(shared_key) => {
                assert_eq!(shared_key, seq);

                // Verify storage key is just SeqId bytes
                let bytes = shared_key.storage_key();
                assert_eq!(bytes.len(), 8); // i64 = 8 bytes
            }
            _ => panic!("Expected Shared key"),
        }
    }

    #[test]
    fn test_storage_key_bytes() {
        let user_id = UserId::new("user1");
        let seq = SeqId::new(100);

        let key = generate_storage_key(TableType::User, Some(user_id), seq);
        let bytes = key.storage_key_bytes();

        assert!(!bytes.is_empty());
        assert_eq!(bytes[0], 5); // user_id length
    }

    #[test]
    #[should_panic(expected = "user_id required")]
    fn test_generate_storage_key_user_table_missing_user_id() {
        let seq = SeqId::new(100);
        generate_storage_key(TableType::User, None, seq);
    }
}
