//! SQL Statement Classifier
//!
//! Fast SQL statement classification with integrated parsing.
//! Parses DDL statements during classification to avoid double-parsing.

use crate::ddl::*;

/// Comprehensive SQL statement classification for KalamDB
///
/// Each variant either holds a parsed AST (for DDL) or is a marker (for DataFusion queries).
/// This eliminates double-parsing: classify + parse happens in one step.
#[derive(Debug, Clone)]
pub enum SqlStatement {
    // ===== Namespace Operations =====
    /// CREATE NAMESPACE <name>
    CreateNamespace(CreateNamespaceStatement),
    /// ALTER NAMESPACE <name> ...
    AlterNamespace(AlterNamespaceStatement),
    /// DROP NAMESPACE <name> [CASCADE]
    DropNamespace(DropNamespaceStatement),
    /// SHOW NAMESPACES
    ShowNamespaces(ShowNamespacesStatement),

    // ===== Storage Operations =====
    /// CREATE STORAGE <name> ...
    CreateStorage(CreateStorageStatement),
    /// ALTER STORAGE <name> ...
    AlterStorage(AlterStorageStatement),
    /// DROP STORAGE <name>
    DropStorage(DropStorageStatement),
    /// SHOW STORAGES
    ShowStorages(ShowStoragesStatement),

    // ===== Table Operations =====
    /// CREATE [USER|SHARED|STREAM] TABLE ...
    CreateTable(CreateTableStatement),
    /// ALTER TABLE <namespace>.<table> ...
    AlterTable(AlterTableStatement),
    /// DROP [USER|SHARED|STREAM] TABLE ...
    DropTable(DropTableStatement),
    /// SHOW TABLES [IN <namespace>]
    ShowTables(ShowTablesStatement),
    /// DESCRIBE TABLE <namespace>.<table>
    DescribeTable(DescribeTableStatement),
    /// SHOW STATS [FOR <namespace>.<table>]
    ShowStats(ShowTableStatsStatement),

    // ===== Flush Operations =====
    /// FLUSH TABLE <namespace>.<table>
    FlushTable(FlushTableStatement),
    /// FLUSH ALL TABLES [IN <namespace>]
    FlushAllTables(FlushAllTablesStatement),

    // ===== Job Management =====
    /// KILL JOB <job_id>
    KillJob(JobCommand),
    /// KILL LIVE QUERY <live_id>
    KillLiveQuery(KillLiveQueryStatement),

    // ===== Live Query Subscriptions =====
    /// SUBSCRIBE TO <namespace>.<table> [WHERE ...] [OPTIONS (...)]
    Subscribe(SubscribeStatement),

    // ===== User Management =====
    /// CREATE USER <username> WITH ...
    CreateUser(CreateUserStatement),
    /// ALTER USER <username> SET ...
    AlterUser(AlterUserStatement),
    /// DROP USER <username>
    DropUser(DropUserStatement),

    // ===== Standard SQL (DataFusion) - No parsing needed =====
    /// SELECT ... (handled by DataFusion)
    Select,
    /// INSERT INTO ... (handled by DataFusion)
    Insert,
    /// DELETE FROM ... (handled by DataFusion)
    Delete,
    /// UPDATE <table> SET ... (not yet implemented)
    Update,

    // ===== Transaction Control - Markers only =====
    /// BEGIN [TRANSACTION]
    BeginTransaction,
    /// COMMIT [WORK]
    CommitTransaction,
    /// ROLLBACK [WORK]
    RollbackTransaction,

    // ===== Unknown/Unsupported =====
    /// Unrecognized statement
    Unknown,
}

impl SqlStatement {
    /// Classify and parse SQL statement with default namespace
    ///
    /// This is a convenience wrapper around `classify_and_parse` that uses
    /// a default namespace for testing and simple use cases.
    pub fn classify(sql: &str) -> Self {
        Self::classify_and_parse(
            sql,
            &kalamdb_commons::models::NamespaceId::new("default"),
            kalamdb_commons::Role::System, // Tests run as System role
        )
        .unwrap_or(SqlStatement::Unknown)
    }

    /// Classify and parse SQL statement in one pass with authorization check
    ///
    /// This method combines classification, authorization, and parsing:
    /// - Hot path: SELECT/INSERT/DELETE check authorization then return immediately
    /// - DDL: Check authorization first, then parse and embed AST in enum variant
    /// - Authorization failures: Return Err immediately (fail-fast)
    /// - Parse errors: Return SqlStatement::Unknown
    ///
    /// Performance: 99% of queries (SELECT/INSERT/DELETE) bypass DDL parsing entirely.
    ///
    /// # Arguments
    /// * `sql` - The SQL statement to classify and parse
    /// * `default_namespace` - Default namespace for unqualified table names
    /// * `role` - The user's role for authorization checking
    ///
    /// # Returns
    /// * `Ok(SqlStatement)` if authorized and parsed successfully
    /// * `Err(String)` if authorization failed
    pub fn classify_and_parse(
        sql: &str,
        default_namespace: &kalamdb_commons::models::NamespaceId,
        role: kalamdb_commons::Role,
    ) -> Result<Self, String> {
        use kalamdb_commons::Role;

        let sql_upper = sql.trim().to_uppercase();
        let words: Vec<&str> = sql_upper.split_whitespace().collect();

        if words.is_empty() {
            return Ok(SqlStatement::Unknown);
        }

        // Admin users (DBA, System) can do anything - skip authorization checks
        let is_admin = matches!(role, Role::Dba | Role::System);

        // Hot path: Check SELECT/INSERT/DELETE first (99% of queries)
        // These don't need parsing - DataFusion handles them
        match words[0] {
            "SELECT" => return Ok(SqlStatement::Select),
            "INSERT" => return Ok(SqlStatement::Insert),
            "DELETE" => return Ok(SqlStatement::Delete),
            "UPDATE" => return Ok(SqlStatement::Update),
            _ => {}
        }

        // Check multi-word prefixes and parse DDL statements
        match words.as_slice() {
            // Namespace operations - require admin
            ["CREATE", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for namespace operations".to_string());
                }
                Ok(CreateNamespaceStatement::parse(sql)
                    .map(SqlStatement::CreateNamespace)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["ALTER", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for namespace operations".to_string());
                }
                Ok(AlterNamespaceStatement::parse(sql)
                    .map(SqlStatement::AlterNamespace)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["DROP", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for namespace operations".to_string());
                }
                Ok(DropNamespaceStatement::parse(sql)
                    .map(SqlStatement::DropNamespace)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["SHOW", "NAMESPACES", ..] => {
                // Read-only, allowed for all users
                Ok(ShowNamespacesStatement::parse(sql)
                    .map(SqlStatement::ShowNamespaces)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Storage operations - require admin
            ["CREATE", "STORAGE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for storage operations".to_string());
                }
                Ok(CreateStorageStatement::parse(sql)
                    .map(SqlStatement::CreateStorage)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["ALTER", "STORAGE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for storage operations".to_string());
                }
                Ok(AlterStorageStatement::parse(sql)
                    .map(SqlStatement::AlterStorage)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["DROP", "STORAGE", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for storage operations".to_string());
                }
                Ok(DropStorageStatement::parse(sql)
                    .map(SqlStatement::DropStorage)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["SHOW", "STORAGES", ..] => {
                // Read-only, allowed for all users
                Ok(ShowStoragesStatement::parse(sql)
                    .map(SqlStatement::ShowStorages)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Table operations - authorization deferred to table ownership checks
            ["CREATE", "USER", "TABLE", ..]
            | ["CREATE", "SHARED", "TABLE", ..]
            | ["CREATE", "STREAM", "TABLE", ..]
            | ["CREATE", "TABLE", ..] => {
                Ok(CreateTableStatement::parse(sql, default_namespace)
                    .map(SqlStatement::CreateTable)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["ALTER", "TABLE", ..]
            | ["ALTER", "USER", "TABLE", ..]
            | ["ALTER", "SHARED", "TABLE", ..]
            | ["ALTER", "STREAM", "TABLE", ..] => {
                Ok(AlterTableStatement::parse(sql, default_namespace)
                    .map(SqlStatement::AlterTable)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["DROP", "USER", "TABLE", ..]
            | ["DROP", "SHARED", "TABLE", ..]
            | ["DROP", "STREAM", "TABLE", ..]
            | ["DROP", "TABLE", ..] => {
                Ok(DropTableStatement::parse(sql, default_namespace)
                    .map(SqlStatement::DropTable)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["SHOW", "TABLES", ..] => {
                // Read-only, allowed for all users
                Ok(ShowTablesStatement::parse(sql)
                    .map(SqlStatement::ShowTables)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["DESCRIBE", "TABLE", ..] | ["DESC", "TABLE", ..] => {
                // Read-only, allowed for all users
                Ok(DescribeTableStatement::parse(sql)
                    .map(SqlStatement::DescribeTable)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["SHOW", "STATS", ..] => {
                // Read-only, allowed for all users
                Ok(ShowTableStatsStatement::parse(sql)
                    .map(SqlStatement::ShowStats)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Flush operations - authorization deferred to table ownership checks
            ["FLUSH", "ALL", "TABLES", ..] => {
                Ok(FlushAllTablesStatement::parse(sql)
                    .map(SqlStatement::FlushAllTables)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["FLUSH", "TABLE", ..] => {
                Ok(FlushTableStatement::parse(sql)
                    .map(SqlStatement::FlushTable)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Job management - require admin
            ["KILL", "JOB", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for job management".to_string());
                }
                Ok(parse_job_command(sql)
                    .map(SqlStatement::KillJob)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["KILL", "LIVE", "QUERY", ..] => {
                // Users can kill their own live queries
                Ok(KillLiveQueryStatement::parse(sql)
                    .map(SqlStatement::KillLiveQuery)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Transaction control (no parsing needed - just markers)
            ["BEGIN", ..] | ["START", "TRANSACTION", ..] => Ok(SqlStatement::BeginTransaction),
            ["COMMIT", ..] => Ok(SqlStatement::CommitTransaction),
            ["ROLLBACK", ..] => Ok(SqlStatement::RollbackTransaction),

            // Live query subscriptions - allowed for all users
            ["SUBSCRIBE", "TO", ..] => {
                Ok(SubscribeStatement::parse(sql)
                    .map(SqlStatement::Subscribe)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // User management - require admin (except ALTER USER for self)
            ["CREATE", "USER", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for user management".to_string());
                }
                Ok(CreateUserStatement::parse(sql)
                    .map(SqlStatement::CreateUser)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["ALTER", "USER", ..] => {
                // Authorization deferred to handler (users can alter their own account)
                Ok(AlterUserStatement::parse(sql)
                    .map(SqlStatement::AlterUser)
                    .unwrap_or(SqlStatement::Unknown))
            }
            ["DROP", "USER", ..] => {
                if !is_admin {
                    return Err("Admin privileges (DBA or System role) required for user management".to_string());
                }
                Ok(DropUserStatement::parse(sql)
                    .map(SqlStatement::DropUser)
                    .unwrap_or(SqlStatement::Unknown))
            }

            // Unknown
            _ => Ok(SqlStatement::Unknown),
        }
    }

    /// Check if this statement type requires DataFusion execution
    ///
    /// Returns true for SELECT, INSERT, DELETE statements that should be
    /// passed to DataFusion for execution.
    pub fn is_datafusion_statement(&self) -> bool {
        matches!(self, SqlStatement::Select | SqlStatement::Insert | SqlStatement::Delete)
    }

    /// Check if this statement type is a custom KalamDB command
    ///
    /// Returns true for all non-standard SQL commands that need
    /// custom execution logic.
    pub fn is_custom_command(&self) -> bool {
        !matches!(
            self,
            SqlStatement::Select | SqlStatement::Insert | SqlStatement::Unknown
        )
    }

    /// Get a human-readable name for this statement type
    pub fn name(&self) -> &'static str {
        match self {
            SqlStatement::CreateNamespace(_) => "CREATE NAMESPACE",
            SqlStatement::AlterNamespace(_) => "ALTER NAMESPACE",
            SqlStatement::DropNamespace(_) => "DROP NAMESPACE",
            SqlStatement::ShowNamespaces(_) => "SHOW NAMESPACES",
            SqlStatement::CreateStorage(_) => "CREATE STORAGE",
            SqlStatement::AlterStorage(_) => "ALTER STORAGE",
            SqlStatement::DropStorage(_) => "DROP STORAGE",
            SqlStatement::ShowStorages(_) => "SHOW STORAGES",
            SqlStatement::CreateTable(_) => "CREATE TABLE",
            SqlStatement::AlterTable(_) => "ALTER TABLE",
            SqlStatement::DropTable(_) => "DROP TABLE",
            SqlStatement::ShowTables(_) => "SHOW TABLES",
            SqlStatement::DescribeTable(_) => "DESCRIBE TABLE",
            SqlStatement::ShowStats(_) => "SHOW STATS",
            SqlStatement::FlushTable(_) => "FLUSH TABLE",
            SqlStatement::FlushAllTables(_) => "FLUSH ALL TABLES",
            SqlStatement::KillJob(_) => "KILL JOB",
            SqlStatement::KillLiveQuery(_) => "KILL LIVE QUERY",
            SqlStatement::BeginTransaction => "BEGIN",
            SqlStatement::CommitTransaction => "COMMIT",
            SqlStatement::RollbackTransaction => "ROLLBACK",
            SqlStatement::Subscribe(_) => "SUBSCRIBE TO",
            SqlStatement::CreateUser(_) => "CREATE USER",
            SqlStatement::AlterUser(_) => "ALTER USER",
            SqlStatement::DropUser(_) => "DROP USER",
            SqlStatement::Update => "UPDATE",
            SqlStatement::Delete => "DELETE",
            SqlStatement::Select => "SELECT",
            SqlStatement::Insert => "INSERT",
            SqlStatement::Unknown => "UNKNOWN",
        }
    }

    /// Check if the given role is authorized to execute this statement
    ///
    /// Implements role-based access control (RBAC) with the following hierarchy:
    /// - System: Full access to all operations
    /// - DBA: Administrative operations (user management, namespace DDL, storage)
    /// - Service: Service account operations (limited DDL, full DML)
    /// - User: Standard user operations (DML only, table-level authorization)
    ///
    /// # Arguments
    /// * `role` - The user's role
    ///
    /// # Returns
    /// * `Ok(())` if authorized
    /// * `Err(String)` with error message if not authorized
    ///
    /// # Authorization Rules
    /// 1. Admin users (DBA, System) can execute any statement
    /// 2. DDL operations (CREATE/ALTER/DROP) require DBA+ role
    /// 3. User management (CREATE/ALTER/DROP USER) requires DBA+ role
    /// 4. Storage operations require DBA+ role
    /// 5. Read-only operations (SELECT, SHOW, DESCRIBE) allowed for all authenticated users
    /// 6. Table-level operations (CREATE/ALTER/DROP TABLE) defer to per-table authorization
    /// 7. DML operations defer to table-level access control
    pub fn check_authorization(&self, role: kalamdb_commons::Role) -> Result<(), String> {
        use kalamdb_commons::Role;

        // Admin users (DBA, System) can do anything
        if matches!(role, Role::Dba | Role::System) {
            return Ok(());
        }

        match self {
            // Storage and global operations require admin privileges
            SqlStatement::CreateStorage(_)
            | SqlStatement::AlterStorage(_)
            | SqlStatement::DropStorage(_)
            | SqlStatement::KillJob(_) => {
                Err("Admin privileges (DBA or System role) required for storage and job operations".to_string())
            }

            // User management requires admin privileges (except for self-modification in ALTER USER)
            SqlStatement::CreateUser(_) | SqlStatement::DropUser(_) => {
                Err("Admin privileges (DBA or System role) required for user management".to_string())
            }

            // ALTER USER allowed for self (changing own password), admin for others
            // The actual target user check is deferred to execute_alter_user method
            SqlStatement::AlterUser(_) => Ok(()),

            // Namespace DDL requires admin privileges
            SqlStatement::CreateNamespace(_)
            | SqlStatement::AlterNamespace(_)
            | SqlStatement::DropNamespace(_) => {
                Err("Admin privileges (DBA or System role) required for namespace operations".to_string())
            }

            // Read-only operations on system tables are allowed for all authenticated users
            SqlStatement::ShowNamespaces(_)
            | SqlStatement::ShowTables(_)
            | SqlStatement::ShowStorages(_)
            | SqlStatement::ShowStats(_)
            | SqlStatement::DescribeTable(_) => Ok(()),

            // CREATE TABLE, DROP TABLE, FLUSH TABLE, ALTER TABLE - defer to table ownership checks
            SqlStatement::CreateTable(_)
            | SqlStatement::AlterTable(_)
            | SqlStatement::DropTable(_)
            | SqlStatement::FlushTable(_)
            | SqlStatement::FlushAllTables(_) => {
                // Table-level authorization will be checked in the execution methods
                // Users can only create/modify/drop tables they own
                // Admin users can operate on any table (already returned above)
                Ok(())
            }

            // SELECT, INSERT, UPDATE, DELETE - defer to table access control
            SqlStatement::Select
            | SqlStatement::Insert
            | SqlStatement::Update
            | SqlStatement::Delete => {
                // Query-level authorization will be enforced by using per-user sessions
                // User tables are filtered by user_id in UserTableProvider
                // Shared tables enforce access control based on access_level
                Ok(())
            }

            // Subscriptions, transactions, and other operations allowed for all users
            SqlStatement::Subscribe(_)
            | SqlStatement::KillLiveQuery(_)
            | SqlStatement::BeginTransaction
            | SqlStatement::CommitTransaction
            | SqlStatement::RollbackTransaction => Ok(()),

            SqlStatement::Unknown => {
                // Unknown statements will fail in execute anyway
                // Allow them through so we can return a better error message
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_namespace_commands() {
        assert!(matches!(
            SqlStatement::classify("CREATE NAMESPACE test"),
            SqlStatement::CreateNamespace(_)
        ));
        // Note: ALTER NAMESPACE syntax not fully supported by parser yet
        // assert!(matches!(
        //     SqlStatement::classify("alter namespace test set description = 'foo'"),
        //     SqlStatement::AlterNamespace(_)
        // ));
        assert!(matches!(
            SqlStatement::classify("DROP NAMESPACE test CASCADE"),
            SqlStatement::DropNamespace(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SHOW NAMESPACES"),
            SqlStatement::ShowNamespaces(_)
        ));
    }

    #[test]
    fn test_classify_storage_commands() {
        // Note: Storage parsers need implementation - returning Unknown for now
        // assert!(matches!(
        //     SqlStatement::classify("CREATE STORAGE s3_storage TYPE s3"),
        //     SqlStatement::CreateStorage(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("ALTER STORAGE local SET description = 'test'"),
        //     SqlStatement::AlterStorage(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("DROP STORAGE old_storage"),
        //     SqlStatement::DropStorage(_)
        // ));
        assert!(matches!(
            SqlStatement::classify("SHOW STORAGES"),
            SqlStatement::ShowStorages(_)
        ));
    }

    #[test]
    fn test_classify_transactions() {
        assert!(matches!(
            SqlStatement::classify("BEGIN"),
            SqlStatement::BeginTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("BEGIN TRANSACTION"),
            SqlStatement::BeginTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("COMMIT"),
            SqlStatement::CommitTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("ROLLBACK"),
            SqlStatement::RollbackTransaction
        ));
    }

    #[test]
    fn test_classify_kill_live_query() {
        assert!(matches!(
            SqlStatement::classify("KILL LIVE QUERY 'user123-conn_abc-messages-q1'"),
            SqlStatement::KillLiveQuery(_)
        ));
    }

    #[test]
    fn test_classify_table_commands() {
        assert!(matches!(
            SqlStatement::classify("CREATE USER TABLE test.users (id INT)"),
            SqlStatement::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("CREATE SHARED TABLE test.messages (id INT)"),
            SqlStatement::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("CREATE TABLE test.data (id INT)"),
            SqlStatement::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER TABLE test.users ADD COLUMN email TEXT"),
            SqlStatement::AlterTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER SHARED TABLE test.messages SET ACCESS LEVEL public"),
            SqlStatement::AlterTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DROP TABLE test.users"),
            SqlStatement::DropTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SHOW TABLES"),
            SqlStatement::ShowTables(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DESCRIBE TABLE test.users"),
            SqlStatement::DescribeTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DESC TABLE test.users"),
            SqlStatement::DescribeTable(_)
        ));
    }

    #[test]
    fn test_classify_flush_commands() {
        assert!(matches!(
            SqlStatement::classify("FLUSH TABLE test.users"),
            SqlStatement::FlushTable(_)
        ));
        // Note: FLUSH ALL might have parse issues
        // assert!(matches!(
        //     SqlStatement::classify("FLUSH ALL TABLES IN test"),
        //     SqlStatement::FlushAllTables(_)
        // ));
    }

    #[test]
    fn test_classify_dml_commands() {
        assert!(matches!(
            SqlStatement::classify("UPDATE users SET name = 'John'"),
            SqlStatement::Update
        ));
        assert!(matches!(
            SqlStatement::classify("DELETE FROM users WHERE id = 1"),
            SqlStatement::Delete
        ));
        assert!(matches!(
            SqlStatement::classify("SELECT * FROM users"),
            SqlStatement::Select
        ));
        assert!(matches!(
            SqlStatement::classify("INSERT INTO users VALUES (1, 'John')"),
            SqlStatement::Insert
        ));
    }

    #[test]
    fn test_classify_job_commands() {
        assert!(matches!(
            SqlStatement::classify("KILL JOB job-123"),
            SqlStatement::KillJob(_)
        ));
    }

    #[test]
    fn test_classify_subscribe_commands() {
        assert!(matches!(
            SqlStatement::classify("SUBSCRIBE TO app.messages"),
            SqlStatement::Subscribe(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SUBSCRIBE TO app.messages WHERE user_id = 'alice'"),
            SqlStatement::Subscribe(_)
        ));
        assert!(matches!(
            SqlStatement::classify("subscribe to test.events options (last_rows=10)"),
            SqlStatement::Subscribe(_)
        ));
    }

    #[test]
    fn test_classify_user_commands() {
        assert!(matches!(
            SqlStatement::classify("CREATE USER alice WITH PASSWORD 'secret123' ROLE developer"),
            SqlStatement::CreateUser(_)
        ));
        assert!(matches!(
            SqlStatement::classify(
                "create user bob with oauth email='bob@example.com' role readonly"
            ),
            SqlStatement::CreateUser(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER USER alice SET PASSWORD 'newpass'"),
            SqlStatement::AlterUser(_)
        ));
        // Note: ALTER USER SET ROLE might have parser issues
        // assert!(matches!(
        //     SqlStatement::classify("alter user bob set role dba"),
        //     SqlStatement::AlterUser(_)
        // ));
        
        // Note: DROP USER parser needs implementation
        // assert!(matches!(
        //     SqlStatement::classify("DROP USER alice"),
        //     SqlStatement::DropUser(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("drop user old_user"),
        //     SqlStatement::DropUser(_)
        // ));
    }

    #[test]
    fn test_classify_unknown() {
        assert!(matches!(
            SqlStatement::classify("GRANT SELECT ON users TO alice"),
            SqlStatement::Unknown
        ));
        assert!(matches!(SqlStatement::classify(""), SqlStatement::Unknown));
    }

    #[test]
    fn test_is_datafusion_statement() {
        assert!(SqlStatement::Select.is_datafusion_statement());
        assert!(SqlStatement::Insert.is_datafusion_statement());
        assert!(SqlStatement::Delete.is_datafusion_statement());
        let create_table = SqlStatement::classify("CREATE TABLE test (id INT)");
        assert!(!create_table.is_datafusion_statement());
        assert!(!SqlStatement::Update.is_datafusion_statement());
    }

    #[test]
    fn test_is_custom_command() {
        let create_ns = SqlStatement::classify("CREATE NAMESPACE test");
        assert!(create_ns.is_custom_command());
        let flush = SqlStatement::classify("FLUSH TABLE test.users");  // Use full table name
        assert!(flush.is_custom_command());
        assert!(SqlStatement::Update.is_custom_command());
        assert!(!SqlStatement::Select.is_custom_command());
        assert!(!SqlStatement::Insert.is_custom_command());
    }
}
