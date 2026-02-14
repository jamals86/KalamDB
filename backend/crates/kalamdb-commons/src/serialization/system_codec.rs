use serde::{de::DeserializeOwned, Serialize};

use crate::serialization::generated::{
    audit_log_entry_generated, job_generated, job_node_generated, live_query_generated,
    manifest_cache_entry_generated, namespace_generated, storage_generated, topic_generated,
    topic_offset_generated, user_generated,
};
use crate::storage::StorageError;

type Result<T> = std::result::Result<T, StorageError>;

pub fn encode_flex<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    flexbuffers::to_vec(value)
        .map_err(|e| StorageError::SerializationError(format!("flexbuffers encode failed: {e}")))
}

pub fn decode_flex<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    flexbuffers::from_slice(bytes)
        .map_err(|e| StorageError::SerializationError(format!("flexbuffers decode failed: {e}")))
}

macro_rules! system_codec_fns {
    ($encode_fn:ident, $decode_fn:ident, $module:ident, $table:ident, $args:ident, $finish_fn:ident, $root_fn:ident) => {
        pub fn $encode_fn(payload: &[u8]) -> Result<Vec<u8>> {
            use $module::kalamdb::serialization::system as fb;

            let mut builder = flatbuffers::FlatBufferBuilder::new();
            let payload_vec = builder.create_vector(payload);
            let wrapped = fb::$table::create(
                &mut builder,
                &fb::$args {
                    payload: Some(payload_vec),
                },
            );
            fb::$finish_fn(&mut builder, wrapped);
            Ok(builder.finished_data().to_vec())
        }

        pub fn $decode_fn(bytes: &[u8]) -> Result<Vec<u8>> {
            use $module::kalamdb::serialization::system as fb;

            let wrapped = fb::$root_fn(bytes).map_err(|e| {
                StorageError::SerializationError(format!(
                    "{} wrapper decode failed: {e}",
                    stringify!($table)
                ))
            })?;

            let payload = wrapped.payload().ok_or_else(|| {
                StorageError::SerializationError(format!(
                    "{} wrapper decode failed: missing payload",
                    stringify!($table)
                ))
            })?;
            Ok(payload.bytes().to_vec())
        }
    };
}

system_codec_fns!(
    encode_audit_log_payload,
    decode_audit_log_payload,
    audit_log_entry_generated,
    AuditLogEntryPayload,
    AuditLogEntryPayloadArgs,
    finish_audit_log_entry_payload_buffer,
    root_as_audit_log_entry_payload
);

system_codec_fns!(
    encode_job_node_payload,
    decode_job_node_payload,
    job_node_generated,
    JobNodePayload,
    JobNodePayloadArgs,
    finish_job_node_payload_buffer,
    root_as_job_node_payload
);

system_codec_fns!(
    encode_job_payload,
    decode_job_payload,
    job_generated,
    JobPayload,
    JobPayloadArgs,
    finish_job_payload_buffer,
    root_as_job_payload
);

system_codec_fns!(
    encode_live_query_payload,
    decode_live_query_payload,
    live_query_generated,
    LiveQueryPayload,
    LiveQueryPayloadArgs,
    finish_live_query_payload_buffer,
    root_as_live_query_payload
);

system_codec_fns!(
    encode_manifest_cache_payload,
    decode_manifest_cache_payload,
    manifest_cache_entry_generated,
    ManifestCacheEntryPayload,
    ManifestCacheEntryPayloadArgs,
    finish_manifest_cache_entry_payload_buffer,
    root_as_manifest_cache_entry_payload
);

system_codec_fns!(
    encode_namespace_payload,
    decode_namespace_payload,
    namespace_generated,
    NamespacePayload,
    NamespacePayloadArgs,
    finish_namespace_payload_buffer,
    root_as_namespace_payload
);

system_codec_fns!(
    encode_storage_payload,
    decode_storage_payload,
    storage_generated,
    StoragePayload,
    StoragePayloadArgs,
    finish_storage_payload_buffer,
    root_as_storage_payload
);

system_codec_fns!(
    encode_topic_offset_payload,
    decode_topic_offset_payload,
    topic_offset_generated,
    TopicOffsetPayload,
    TopicOffsetPayloadArgs,
    finish_topic_offset_payload_buffer,
    root_as_topic_offset_payload
);

system_codec_fns!(
    encode_topic_payload,
    decode_topic_payload,
    topic_generated,
    TopicPayload,
    TopicPayloadArgs,
    finish_topic_payload_buffer,
    root_as_topic_payload
);

system_codec_fns!(
    encode_user_payload,
    decode_user_payload,
    user_generated,
    UserPayload,
    UserPayloadArgs,
    finish_user_payload_buffer,
    root_as_user_payload
);
