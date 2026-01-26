//! Serialization traits for KalamDB entity storage.
//!
//! This module provides the `KSerializable` trait which standardizes how
//! entities are serialized/deserialized for storage in RocksDB.

use bincode::config::standard;
use serde::{Deserialize, Serialize};

use crate::storage::StorageError;

type Result<T> = std::result::Result<T, StorageError>;

/// Trait implemented by values that can be stored in an [`EntityStore`].
///
/// Types can override `encode`/`decode` for custom storage formats (e.g.,
/// row envelopes vs. JSON). The default implementation uses bincode.
///
/// ## Example
///
/// ```rust
/// use serde::{Deserialize, Serialize};
/// use bincode::{Encode, Decode};
/// use kalamdb_commons::serialization::KSerializable;
///
/// #[derive(Serialize, Deserialize, Encode, Decode)]
/// struct MyEntity {
///     id: String,
///     value: i64,
/// }
///
/// impl KSerializable for MyEntity {}
/// ```
pub trait KSerializable: Serialize + for<'de> Deserialize<'de> + Send + Sync {
    fn encode(&self) -> Result<Vec<u8>> {
        let config = standard();
        bincode::serde::encode_to_vec(self, config)
            .map_err(|e| StorageError::SerializationError(format!("bincode encode failed: {}", e)))
    }

    fn decode(bytes: &[u8]) -> Result<Self>
    where
        Self: Sized,
    {
        let config = standard();
        bincode::serde::decode_from_slice(bytes, config)
            .map(|(entity, _)| entity)
            .map_err(|e| StorageError::SerializationError(format!("bincode decode failed: {}", e)))
    }
}

// Blanket implementation for String (common storage type)
impl KSerializable for String {}
