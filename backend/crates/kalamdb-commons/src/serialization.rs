//! Serialization traits for KalamDB entity storage.
//!
//! This module provides the `KSerializable` trait which standardizes how
//! entities are serialized/deserialized for storage in RocksDB.

use bincode::config::standard;
use serde::{Deserialize, Serialize};

use crate::storage::StorageError;

pub mod envelope;
pub mod generated;
pub mod row_codec;
pub mod schema;
pub mod system_codec;

type Result<T> = std::result::Result<T, StorageError>;

pub use envelope::{CodecKind, EntityEnvelope};

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

/// Encode a payload into a versioned entity envelope.
///
/// This helper establishes the envelope contract used during the migration away
/// from raw bincode payloads. Callers are expected to provide a stable
/// `schema_id` and increment `schema_version` on wire changes.
pub fn encode_enveloped(
    codec_kind: CodecKind,
    schema_id: impl Into<String>,
    schema_version: u16,
    payload: Vec<u8>,
) -> Result<Vec<u8>> {
    let envelope = EntityEnvelope::new(codec_kind, schema_id, schema_version, payload);
    envelope.encode()
}

/// Decode and validate an entity envelope.
///
/// `expected_schema_id` and `expected_schema_version` provide strict validation
/// to prevent cross-schema decode mistakes.
pub fn decode_enveloped(
    bytes: &[u8],
    expected_schema_id: &str,
    expected_schema_version: u16,
) -> Result<EntityEnvelope> {
    let envelope = EntityEnvelope::decode(bytes)?;
    envelope.validate(expected_schema_id, expected_schema_version)?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip() {
        let bytes =
            encode_enveloped(CodecKind::FlatBuffers, "kalamdb.test.envelope", 1, vec![1, 2, 3])
                .expect("encode envelope");

        let decoded =
            decode_enveloped(&bytes, "kalamdb.test.envelope", 1).expect("decode envelope");
        assert_eq!(decoded.codec_kind, CodecKind::FlatBuffers);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
    }

    #[test]
    fn envelope_schema_mismatch_rejected() {
        let bytes = encode_enveloped(CodecKind::FlatBuffers, "schema.a", 1, vec![9])
            .expect("encode envelope");

        let err = decode_enveloped(&bytes, "schema.b", 1).expect_err("schema mismatch should fail");
        assert!(err.to_string().contains("schema_id mismatch"));
    }
}
