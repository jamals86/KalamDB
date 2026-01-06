//! System group commands (metadata: namespaces, tables, storages)

use kalamdb_commons::models::{NamespaceId, StorageId};
use kalamdb_commons::models::schemas::TableType;
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
        table_type: TableType,
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
        storage_id: StorageId,
        config_json: String,
    },
    UnregisterStorage {
        storage_id: StorageId,
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
    /// Create an error response with the given message
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error { message: msg.into() }
    }

    /// Returns true if this is not an error response
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }
}