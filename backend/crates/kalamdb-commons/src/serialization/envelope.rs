use serde::{Deserialize, Serialize};

use crate::serialization::generated::entity_envelope_generated::kalamdb::serialization as fb;
use crate::storage::StorageError;

type Result<T> = std::result::Result<T, StorageError>;

/// Storage codec identifier carried in the entity envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CodecKind {
    Bincode = 0,
    FlatBuffers = 1,
    FlexBuffers = 2,
}

/// Versioned envelope around persisted values.
///
/// This envelope enables codec/schema migration without requiring a full data
/// rewrite of all persisted models at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityEnvelope {
    pub codec_kind: CodecKind,
    pub schema_id: String,
    pub schema_version: u16,
    pub payload: Vec<u8>,
}

impl EntityEnvelope {
    pub fn new(
        codec_kind: CodecKind,
        schema_id: impl Into<String>,
        schema_version: u16,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            codec_kind,
            schema_id: schema_id.into(),
            schema_version,
            payload,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let schema_id = builder.create_string(&self.schema_id);
        let payload = builder.create_vector(&self.payload);

        let args = fb::EntityEnvelopeArgs {
            codec_kind: to_fb_codec_kind(self.codec_kind),
            schema_id: Some(schema_id),
            schema_version: self.schema_version,
            payload: Some(payload),
        };

        let envelope = fb::EntityEnvelope::create(&mut builder, &args);
        fb::finish_entity_envelope_buffer(&mut builder, envelope);
        Ok(builder.finished_data().to_vec())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if !fb::entity_envelope_buffer_has_identifier(bytes) {
            return Err(StorageError::SerializationError(
                "entity envelope decode failed: invalid file identifier (expected KENV)"
                    .to_string(),
            ));
        }

        let envelope = fb::root_as_entity_envelope(bytes).map_err(|e| {
            StorageError::SerializationError(format!("entity envelope decode failed: {e}"))
        })?;

        let schema_id = envelope.schema_id().ok_or_else(|| {
            StorageError::SerializationError(
                "entity envelope decode failed: missing schema_id".to_string(),
            )
        })?;

        let payload = envelope.payload().ok_or_else(|| {
            StorageError::SerializationError(
                "entity envelope decode failed: missing payload".to_string(),
            )
        })?;

        Ok(Self {
            codec_kind: from_fb_codec_kind(envelope.codec_kind())?,
            schema_id: schema_id.to_string(),
            schema_version: envelope.schema_version(),
            payload: payload.bytes().to_vec(),
        })
    }

    pub fn validate(&self, expected_schema_id: &str, expected_schema_version: u16) -> Result<()> {
        if self.schema_id != expected_schema_id {
            return Err(StorageError::SerializationError(format!(
                "schema_id mismatch: expected '{expected_schema_id}', got '{}'",
                self.schema_id
            )));
        }

        if self.schema_version != expected_schema_version {
            return Err(StorageError::SerializationError(format!(
                "schema_version mismatch: expected {}, got {}",
                expected_schema_version, self.schema_version
            )));
        }

        Ok(())
    }
}

fn to_fb_codec_kind(value: CodecKind) -> fb::CodecKind {
    match value {
        CodecKind::Bincode => fb::CodecKind::Bincode,
        CodecKind::FlatBuffers => fb::CodecKind::FlatBuffers,
        CodecKind::FlexBuffers => fb::CodecKind::FlexBuffers,
    }
}

fn from_fb_codec_kind(value: fb::CodecKind) -> Result<CodecKind> {
    match value {
        fb::CodecKind::Bincode => Ok(CodecKind::Bincode),
        fb::CodecKind::FlatBuffers => Ok(CodecKind::FlatBuffers),
        fb::CodecKind::FlexBuffers => Ok(CodecKind::FlexBuffers),
        _ => Err(StorageError::SerializationError(format!(
            "entity envelope decode failed: unknown codec kind {}",
            value.0
        ))),
    }
}
