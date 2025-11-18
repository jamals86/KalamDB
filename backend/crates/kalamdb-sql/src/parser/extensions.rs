//! KalamDB-specific SQL extensions.
//!
//! This module contains parsers for commands that are specific to KalamDB
//! and don't fit into standard SQL syntax:
//!
//! - **CREATE STORAGE**: Define cloud storage locations
//! - **ALTER STORAGE**: Modify storage configuration
//! - **FLUSH TABLE / FLUSH ALL TABLES**: Trigger manual data persistence
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

// Re-export flush commands
pub use crate::ddl::flush_commands::{FlushAllTablesStatement, FlushTableStatement};

// Job commands (KILL JOB)
pub use crate::ddl::job_commands::{parse_job_command, JobCommand};

// Subscribe commands (SUBSCRIBE TO)
pub use crate::ddl::subscribe_commands::{SubscribeOptions, SubscribeStatement};

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
    /// FLUSH TABLE command
    FlushTable(FlushTableStatement),
    /// FLUSH ALL TABLES command
    FlushAllTables(FlushAllTablesStatement),
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

        // Try FLUSH TABLE
        if sql_upper.starts_with("FLUSH TABLE") {
            return FlushTableStatement::parse(sql)
                .map(ExtensionStatement::FlushTable)
                .map_err(|e| format!("FLUSH TABLE parsing failed: {}", e));
        }

        // Try FLUSH ALL TABLES
        if sql_upper.starts_with("FLUSH ALL TABLES") {
            return FlushAllTablesStatement::parse(sql)
                .map(ExtensionStatement::FlushAllTables)
                .map_err(|e| format!("FLUSH ALL TABLES parsing failed: {}", e));
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

        Err("Unknown KalamDB extension command. Supported commands: CREATE/ALTER/DROP/SHOW STORAGE, FLUSH TABLE, FLUSH ALL TABLES, KILL JOB, SUBSCRIBE TO, CREATE/ALTER/DROP USER, SHOW MANIFEST".to_string())
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
        assert!(matches!(
            result.unwrap(),
            ExtensionStatement::CreateStorage(_)
        ));
    }

    #[test]
    fn test_parse_flush_table() {
        let sql = "FLUSH TABLE prod.events";
        let result = ExtensionStatement::parse(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ExtensionStatement::FlushTable(_)));
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
        assert!(result
            .unwrap_err()
            .contains("Unknown KalamDB extension command"));
    }
}
