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
pub const ROW_SCHEMA_VERSION: u16 = 2;
