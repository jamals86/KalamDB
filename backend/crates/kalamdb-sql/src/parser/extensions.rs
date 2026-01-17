//! KalamDB-specific SQL extensions.
//!
//! This module contains parsers for commands that are specific to KalamDB
//! and don't fit into standard SQL syntax:
//!
//! - **CREATE STORAGE**: Define cloud storage locations
//! - **ALTER STORAGE**: Modify storage configuration
//! - **STORAGE FLUSH / STORAGE COMPACT**: Trigger storage maintenance operations
//! - **KILL JOB**: Cancel background jobs
//! - **KILL LIVE QUERY**: Terminate WebSocket subscriptions
//!
//! These parsers complement the standard SQL parser and are invoked
//! when the standard parser doesn't recognize the syntax.

/// Re-export existing KalamDB-specific parsers for convenience.
///
/// These parsers handle commands that are unique to KalamDB and not part
/// of standard SQL. They are implemented in separate modules and re-exported
/// here for a unified parser interface.
// Re-export storage commands
pub use crate::ddl::storage_commands::{
    AlterStorageStatement, CreateStorageStatement, DropStorageStatement, ShowStoragesStatement,
};

// Re-export flush/compact commands
pub use crate::ddl::compact_commands::{CompactAllTablesStatement, CompactTableStatement};
pub use crate::ddl::flush_commands::{FlushAllTablesStatement, FlushTableStatement};

// Job commands (KILL JOB)
pub use crate::ddl::job_commands::{parse_job_command, JobCommand};

// Subscribe commands (SUBSCRIBE TO)
pub use crate::ddl::subscribe_commands::SubscribeStatement;
// Re-export SubscriptionOptions from kalamdb_commons
pub use kalamdb_commons::websocket::SubscriptionOptions;

// User commands (CREATE USER, ALTER USER, DROP USER)
pub use crate::ddl::user_commands::{
    AlterUserStatement, CreateUserStatement, DropUserStatement, UserModification,
};

// Manifest cache commands (SHOW MANIFEST CACHE)
pub use crate::ddl::manifest_commands::ShowManifestStatement;

/// Extension statement types that don't fit into standard SQL.
///
/// This enum represents KalamDB-specific commands that extend SQL
/// with custom functionality.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionStatement {
    /// CREATE STORAGE command
    CreateStorage(CreateStorageStatement),
    /// ALTER STORAGE command
    AlterStorage(AlterStorageStatement),
    /// DROP STORAGE command
    DropStorage(DropStorageStatement),
    /// SHOW STORAGES command
    ShowStorages(ShowStoragesStatement),
    /// STORAGE FLUSH TABLE command
    FlushTable(FlushTableStatement),
    /// STORAGE FLUSH ALL command
    FlushAllTables(FlushAllTablesStatement),
    /// STORAGE COMPACT TABLE command
    CompactTable(CompactTableStatement),
    /// STORAGE COMPACT ALL command
    CompactAllTables(CompactAllTablesStatement),
    /// KILL JOB command
    KillJob(JobCommand),
    /// SUBSCRIBE TO command (for live query subscriptions)
    Subscribe(SubscribeStatement),
    /// CREATE USER command
    CreateUser(CreateUserStatement),
    /// ALTER USER command
    AlterUser(AlterUserStatement),
    /// DROP USER command
    DropUser(DropUserStatement),
    /// SHOW MANIFEST CACHE command
    ShowManifest(ShowManifestStatement),
    /// CLUSTER SNAPSHOT command
    ClusterSnapshot,
    /// CLUSTER PURGE command
    ClusterPurge { upto: u64 },
    /// CLUSTER TRIGGER ELECTION command
    ClusterTriggerElection,
    /// CLUSTER TRANSFER-LEADER command
    ClusterTransferLeader { node_id: u64 },
    /// CLUSTER STEPDOWN command
    ClusterStepdown,
    /// CLUSTER CLEAR command
    ClusterClear,
    /// CLUSTER LIST command
    ClusterList,
    /// CLUSTER JOIN command (not implemented yet)
    ClusterJoin(String),
    /// CLUSTER LEAVE command (not implemented yet)
    ClusterLeave,
}

impl ExtensionStatement {
    /// Parse a KalamDB-specific extension statement.
    ///
    /// Attempts to parse the SQL as one of the supported extension commands.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SQL string to parse
    ///
    /// # Returns
    ///
    /// The parsed extension statement, or an error if the syntax is invalid
    pub fn parse(sql: &str) -> Result<Self, String> {
        let sql_upper = sql.trim().to_uppercase();

        // Try CREATE STORAGE
        if sql_upper.starts_with("CREATE STORAGE") {
            return CreateStorageStatement::parse(sql)
                .map(ExtensionStatement::CreateStorage)
                .map_err(|e| format!("CREATE STORAGE parsing failed: {}", e));
        }

        // Try ALTER STORAGE
        if sql_upper.starts_with("ALTER STORAGE") {
            return AlterStorageStatement::parse(sql)
                .map(ExtensionStatement::AlterStorage)
                .map_err(|e| format!("ALTER STORAGE parsing failed: {}", e));
        }

        // Try DROP STORAGE
        if sql_upper.starts_with("DROP STORAGE") {
            return DropStorageStatement::parse(sql)
                .map(ExtensionStatement::DropStorage)
                .map_err(|e| format!("DROP STORAGE parsing failed: {}", e));
        }

        // Try SHOW STORAGES
        if sql_upper.starts_with("SHOW STORAGES") || sql_upper.starts_with("SHOW STORAGE") {
            return ShowStoragesStatement::parse(sql)
                .map(ExtensionStatement::ShowStorages)
                .map_err(|e| format!("SHOW STORAGES parsing failed: {}", e));
        }

        // Try STORAGE FLUSH TABLE
        if sql_upper.starts_with("STORAGE FLUSH TABLE") {
            return FlushTableStatement::parse(sql)
                .map(ExtensionStatement::FlushTable)
                .map_err(|e| format!("STORAGE FLUSH TABLE parsing failed: {}", e));
        }

        // Try STORAGE FLUSH ALL
        if sql_upper.starts_with("STORAGE FLUSH ALL") {
            return FlushAllTablesStatement::parse(sql)
                .map(ExtensionStatement::FlushAllTables)
                .map_err(|e| format!("STORAGE FLUSH ALL parsing failed: {}", e));
        }

        // Try STORAGE COMPACT TABLE
        if sql_upper.starts_with("STORAGE COMPACT TABLE") {
            return CompactTableStatement::parse(sql)
                .map(ExtensionStatement::CompactTable)
                .map_err(|e| format!("STORAGE COMPACT TABLE parsing failed: {}", e));
        }

        // Try STORAGE COMPACT ALL
        if sql_upper.starts_with("STORAGE COMPACT ALL") {
            return CompactAllTablesStatement::parse(sql)
                .map(ExtensionStatement::CompactAllTables)
                .map_err(|e| format!("STORAGE COMPACT ALL parsing failed: {}", e));
        }

        // Try KILL JOB
        if sql_upper.starts_with("KILL JOB") {
            return parse_job_command(sql)
                .map(ExtensionStatement::KillJob)
                .map_err(|e| format!("KILL JOB parsing failed: {}", e));
        }

        // Try SUBSCRIBE TO
        if sql_upper.starts_with("SUBSCRIBE TO") {
            return SubscribeStatement::parse(sql)
                .map(ExtensionStatement::Subscribe)
                .map_err(|e| format!("SUBSCRIBE TO parsing failed: {}", e));
        }

        // Try CREATE USER
        if sql_upper.starts_with("CREATE USER") {
            return CreateUserStatement::parse(sql)
                .map(ExtensionStatement::CreateUser)
                .map_err(|e| format!("CREATE USER parsing failed: {}", e));
        }

        // Try ALTER USER
        if sql_upper.starts_with("ALTER USER") {
            return AlterUserStatement::parse(sql)
                .map(ExtensionStatement::AlterUser)
                .map_err(|e| format!("ALTER USER parsing failed: {}", e));
        }

        // Try DROP USER
        if sql_upper.starts_with("DROP USER") {
            return DropUserStatement::parse(sql)
                .map(ExtensionStatement::DropUser)
                .map_err(|e| format!("DROP USER parsing failed: {}", e));
        }

        // Try SHOW MANIFEST
        if sql_upper.starts_with("SHOW MANIFEST") {
            return ShowManifestStatement::parse(sql)
                .map(ExtensionStatement::ShowManifest)
                .map_err(|e| format!("SHOW MANIFEST parsing failed: {}", e));
        }

        // Try CLUSTER commands
        if sql_upper.starts_with("CLUSTER") {
            let parts: Vec<&str> = sql_upper.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[1] {
                    "SNAPSHOT" => return Ok(ExtensionStatement::ClusterSnapshot),
                    "PURGE" => {
                        let original_parts: Vec<&str> = sql.trim().split_whitespace().collect();
                        let upto = original_parts
                            .iter()
                            .skip(2)
                            .find(|part| part.trim_start_matches('-').eq_ignore_ascii_case("upto"))
                            .and_then(|_| {
                                original_parts
                                    .iter()
                                    .skip(3)
                                    .find(|p| p.parse::<u64>().is_ok())
                                    .and_then(|p| p.parse::<u64>().ok())
                            })
                            .or_else(|| {
                                original_parts
                                    .get(2)
                                    .and_then(|p| p.parse::<u64>().ok())
                            });

                        if let Some(upto) = upto {
                            return Ok(ExtensionStatement::ClusterPurge { upto });
                        }
                        return Err("CLUSTER PURGE requires --upto <index> or a numeric index".to_string());
                    }
                    "TRIGGER" => {
                        if parts.get(2) == Some(&"ELECTION") {
                            return Ok(ExtensionStatement::ClusterTriggerElection);
                        }
                    }
                    "TRIGGER-ELECTION" => return Ok(ExtensionStatement::ClusterTriggerElection),
                    "TRANSFER" => {
                        if parts.get(2) == Some(&"LEADER") {
                            let node_id = parts.get(3).and_then(|id| id.parse::<u64>().ok());
                            if let Some(node_id) = node_id {
                                return Ok(ExtensionStatement::ClusterTransferLeader { node_id });
                            }
                            return Err("CLUSTER TRANSFER LEADER requires a node id".to_string());
                        }
                    }
                    "TRANSFER-LEADER" => {
                        let node_id = parts.get(2).and_then(|id| id.parse::<u64>().ok());
                        if let Some(node_id) = node_id {
                            return Ok(ExtensionStatement::ClusterTransferLeader { node_id });
                        }
                        return Err("CLUSTER TRANSFER-LEADER requires a node id".to_string());
                    }
                    "STEPDOWN" | "STEP-DOWN" => return Ok(ExtensionStatement::ClusterStepdown),
                    "CLEAR" => return Ok(ExtensionStatement::ClusterClear),
                    "LIST" | "LS" | "STATUS" => return Ok(ExtensionStatement::ClusterList),
                    "JOIN" => {
                        // Get the address from the original SQL to preserve case
                        let original_parts: Vec<&str> = sql.trim().split_whitespace().collect();
                        if original_parts.len() >= 3 {
                            return Ok(ExtensionStatement::ClusterJoin(original_parts[2].to_string()));
                        } else {
                            return Err("CLUSTER JOIN requires a node address".to_string());
                        }
                    }
                    "LEAVE" => return Ok(ExtensionStatement::ClusterLeave),
                    _ => return Err("Unknown CLUSTER subcommand. Supported: SNAPSHOT, PURGE, TRIGGER ELECTION, TRANSFER-LEADER, STEPDOWN, CLEAR, LIST, JOIN, LEAVE".to_string()),
                }
            }
        }

        Err("Unknown KalamDB extension command. Supported commands: CREATE/ALTER/DROP/SHOW STORAGE, STORAGE FLUSH, STORAGE COMPACT, KILL JOB, SUBSCRIBE TO, CREATE/ALTER/DROP USER, SHOW MANIFEST".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_storage() {
        let sql = "CREATE STORAGE my_storage \
                   TYPE filesystem \
                   NAME 'My Storage' \
                   BASE_DIRECTORY '/data' \
                   SHARED_TABLES_TEMPLATE '{namespace}/{table}/' \
                   USER_TABLES_TEMPLATE '{namespace}/{table}/{userId}/'";
        let result = ExtensionStatement::parse(sql);
        if let Err(ref e) = result {
            eprintln!("Parse error: {}", e);
        }
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::CreateStorage(_)));
    }

    #[test]
    fn test_parse_flush_table() {
        let sql = "STORAGE FLUSH TABLE prod.events";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::FlushTable(_)));
    }

    #[test]
    fn test_parse_compact_table() {
        let sql = "STORAGE COMPACT TABLE prod.events";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::CompactTable(_)));
    }

    #[test]
    fn test_parse_kill_job() {
        let sql = "KILL JOB 'job-123'";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::KillJob(_)));
    }

    #[test]
    fn test_parse_subscribe_to() {
        let sql = "SUBSCRIBE TO app.messages";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::Subscribe(_)));
    }

    #[test]
    fn test_parse_subscribe_with_where() {
        let sql = "SUBSCRIBE TO app.messages WHERE user_id = CURRENT_USER()";
        let result = ExtensionStatement::parse(sql);

        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::Subscribe(_)));
    }

    #[test]
    fn test_parse_unknown_extension() {
        let sql = "CREATE FOOBAR something";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown KalamDB extension command"));
    }
}
