//! FlatBuffer schema contracts for persisted entities.
//!
//! Note: `.fbs` files in this directory are the source of truth for wire
//! contracts. Generated Rust code is placed under `serialization/generated`.

/// Schema identifier for entity envelope payloads.
pub const ENTITY_ENVELOPE_SCHEMA_ID: &str = "kalamdb.serialization.entity_envelope";

/// Initial schema version for the entity envelope.
pub const ENTITY_ENVELOPE_SCHEMA_VERSION: u16 = 1;

pub const ROW_SCHEMA_ID: &str = "kalamdb.serialization.row.RowPayload";
pub const USER_TABLE_ROW_SCHEMA_ID: &str = "kalamdb.serialization.row.UserTableRowPayload";
pub const SHARED_TABLE_ROW_SCHEMA_ID: &str = "kalamdb.serialization.row.SharedTableRowPayload";
pub const ROW_SCHEMA_VERSION: u16 = 1;

pub const SYSTEM_AUDIT_LOG_SCHEMA_ID: &str = "kalamdb.serialization.system.AuditLogEntryPayload";
pub const SYSTEM_JOB_NODE_SCHEMA_ID: &str = "kalamdb.serialization.system.JobNodePayload";
pub const SYSTEM_JOB_SCHEMA_ID: &str = "kalamdb.serialization.system.JobPayload";
pub const SYSTEM_LIVE_QUERY_SCHEMA_ID: &str = "kalamdb.serialization.system.LiveQueryPayload";
pub const SYSTEM_MANIFEST_CACHE_SCHEMA_ID: &str =
    "kalamdb.serialization.system.ManifestCacheEntryPayload";
pub const SYSTEM_NAMESPACE_SCHEMA_ID: &str = "kalamdb.serialization.system.NamespacePayload";
pub const SYSTEM_STORAGE_SCHEMA_ID: &str = "kalamdb.serialization.system.StoragePayload";
pub const SYSTEM_TOPIC_OFFSET_SCHEMA_ID: &str = "kalamdb.serialization.system.TopicOffsetPayload";
pub const SYSTEM_TOPIC_SCHEMA_ID: &str = "kalamdb.serialization.system.TopicPayload";
pub const SYSTEM_USER_SCHEMA_ID: &str = "kalamdb.serialization.system.UserPayload";
pub const SYSTEM_SCHEMA_VERSION: u16 = 1;
