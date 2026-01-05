//! System group commands (metadata: namespaces, tables, storages)

use kalamdb_commons::models::NamespaceId;
use kalamdb_commons::TableId;
use serde::{Deserialize, Serialize};

/// Commands for the system metadata Raft group
///
/// Handles: namespaces, tables (schema), storages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemCommand {
    // === Namespace Operations ===
    CreateNamespace {
        namespace_id: NamespaceId,
        created_by: Option<String>,
    },
    DeleteNamespace {
        namespace_id: NamespaceId,
    },

    // === Table Operations ===
    CreateTable {
        table_id: TableId,
        table_type: String,  // "user", "shared", "stream"
        schema_json: String, // Serialized TableDefinition
    },
    AlterTable {
        table_id: TableId,
        schema_json: String,
    },
    DropTable {
        table_id: TableId,
    },

    // === Storage Operations ===
    RegisterStorage {
        storage_id: String,
        config_json: String,
    },
    UnregisterStorage {
        storage_id: String,
    },
}

/// Response from system commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum SystemResponse {
    #[default]
    Ok,
    NamespaceCreated {
        namespace_id: NamespaceId,
    },
    TableCreated {
        table_id: TableId,
    },
    Error {
        message: String,
    },
}

impl SystemResponse {
    pub fn error(msg: impl Into<String>) -> Self {
        SystemResponse::Error { message: msg.into() }
    }

    pub fn is_ok(&self) -> bool {
        !matches!(self, SystemResponse::Error { .. })
    }
}
