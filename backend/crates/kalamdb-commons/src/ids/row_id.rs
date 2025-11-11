//! Row ID types for MVCC architecture
//!
//! This module provides composite row ID types for different table types:
//! - UserTableRowId: Composite key with user_id and _seq for user-scoped tables
//! - SharedTableRowId: Alias to SeqId for shared tables (no user scoping)

use crate::ids::SeqId;
use crate::models::UserId;
use crate::StorageKey;
use serde::{Deserialize, Serialize};

/// Composite key for user table rows: {user_id}:{_seq}
///
/// **MVCC Architecture**: Similar to TableId pattern, this is a composite struct
/// with two fields that implements StorageKey trait for RocksDB storage.
///
/// **Storage Format**: `{user_id_len:1byte}{user_id:variable}{seq:8bytes}`
///
/// # Examples
///
/// ```ignore
/// use kalamdb_commons::ids::{UserTableRowId, SeqId};
/// use kalamdb_commons::models::UserId;
///
/// let user_id = UserId::new("alice");
/// let seq = SeqId::new(12345);
/// let row_id = UserTableRowId::new(user_id, seq);
///
/// // Serialize to storage key
/// let key_bytes = row_id.storage_key();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserTableRowId {
    pub user_id: UserId,
    pub seq: SeqId,
}

impl UserTableRowId {
    /// Create a new user table row ID
    pub fn new(user_id: UserId, seq: SeqId) -> Self {
        Self { user_id, seq }
    }

    /// Get the user_id component
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Get the sequence ID component
    pub fn seq(&self) -> SeqId {
        self.seq
    }

    /// Parse from storage bytes (user_id_len:user_id:seq_bytes)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 9 {
            // Minimum: 1 byte len + 1 char user_id + 8 bytes seq
            return Err("Invalid byte length for UserTableRowId".to_string());
        }

        // First byte is user_id length
        let user_id_len = bytes[0] as usize;
        if bytes.len() < 1 + user_id_len + 8 {
            return Err("Invalid byte structure for UserTableRowId".to_string());
        }

        // Extract user_id
        let user_id_bytes = &bytes[1..1 + user_id_len];
        let user_id_str = String::from_utf8(user_id_bytes.to_vec())
            .map_err(|e| format!("Invalid UTF-8 in user_id: {}", e))?;
        let user_id = UserId::new(user_id_str);

        // Extract seq (last 8 bytes)
        let seq_bytes = &bytes[1 + user_id_len..1 + user_id_len + 8];
        let seq = SeqId::from_bytes(seq_bytes)?;

        Ok(Self::new(user_id, seq))
    }
}

impl StorageKey for UserTableRowId {
    fn storage_key(&self) -> Vec<u8> {
        // Format: user_id_len (1 byte) + user_id (variable) + seq (8 bytes big-endian)
        let user_id_bytes = self.user_id.as_str().as_bytes();
        let user_id_len = user_id_bytes.len().min(255) as u8;
        let seq_bytes = self.seq.to_bytes();

        let mut key = Vec::with_capacity(1 + user_id_len as usize + 8);
        key.push(user_id_len);
        key.extend_from_slice(&user_id_bytes[..user_id_len as usize]);
        key.extend_from_slice(&seq_bytes);
        key
    }
}

/// Type alias for shared table row ID (just SeqId, no user scoping)
///
/// **MVCC Architecture**: Shared tables use SeqId directly as the storage key
/// since they are accessible across all users.
pub type SharedTableRowId = SeqId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_table_row_id_storage_key() {
        let user_id = UserId::new("user1");
        let seq = SeqId::new(12345);
        let row_id = UserTableRowId::new(user_id.clone(), seq);

        // Verify storage key format
        let key_bytes = row_id.storage_key();
        
        // First byte should be user_id length
        assert_eq!(key_bytes[0], 5); // "user1" = 5 bytes
        
        // Parse back
        let parsed = UserTableRowId::from_bytes(&key_bytes).unwrap();
        assert_eq!(parsed.user_id(), &user_id);
        assert_eq!(parsed.seq(), seq);
    }

    #[test]
    fn test_user_table_row_id_round_trip() {
        let user_id = UserId::new("alice");
        let seq = SeqId::new(99999);
        let row_id = UserTableRowId::new(user_id.clone(), seq);

        // Serialize and deserialize
        let bytes = row_id.storage_key();
        let parsed = UserTableRowId::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.user_id, user_id);
        assert_eq!(parsed.seq, seq);
    }

    #[test]
    fn test_user_table_row_id_invalid_bytes() {
        // Too short
        let result = UserTableRowId::from_bytes(&[1, 2, 3]);
        assert!(result.is_err());

        // Length mismatch
        let result = UserTableRowId::from_bytes(&[10, b'a', b'b', 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_shared_table_row_id_alias() {
        // SharedTableRowId is just SeqId
        let seq: SharedTableRowId = SeqId::new(12345);
        assert_eq!(seq.as_i64(), 12345);
    }
}
