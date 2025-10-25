//! SQL Statement Classifier
//!
//! Fast SQL statement classification without full parsing.
//! Used by the executor to dispatch SQL commands to appropriate handlers.

/// Comprehensive SQL statement classification for KalamDB
///
/// All custom SQL commands (DDL, DML, utilities) are classified here.
/// Standard SQL (SELECT, INSERT from DataFusion) can be detected but
/// are typically handled by DataFusion directly.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlStatement {
    // ===== Namespace Operations =====
    /// CREATE NAMESPACE <name>
    CreateNamespace,
    /// ALTER NAMESPACE <name> ...
    AlterNamespace,
    /// DROP NAMESPACE <name> [CASCADE]
    DropNamespace,
    /// SHOW NAMESPACES
    ShowNamespaces,

    // ===== Storage Operations =====
    /// CREATE STORAGE <name> ...
    CreateStorage,
    /// ALTER STORAGE <name> ...
    AlterStorage,
    /// DROP STORAGE <name>
    DropStorage,
    /// SHOW STORAGES
    ShowStorages,

    // ===== Table Operations =====
    /// CREATE [USER|SHARED|STREAM] TABLE ...
    CreateTable,
    /// DROP [USER|SHARED|STREAM] TABLE ...
    DropTable,
    /// SHOW TABLES [IN <namespace>]
    ShowTables,
    /// DESCRIBE TABLE <namespace>.<table>
    DescribeTable,
    /// SHOW STATS [FOR <namespace>.<table>]
    ShowStats,

    // ===== Flush Operations =====
    /// FLUSH TABLE <namespace>.<table>
    FlushTable,
    /// FLUSH ALL TABLES [IN <namespace>]
    FlushAllTables,

    // ===== Job Management =====
    /// KILL JOB <job_id>
    KillJob,
    /// KILL LIVE QUERY <live_id>
    KillLiveQuery,

    // ===== Transaction Control =====
    /// BEGIN [TRANSACTION]
    BeginTransaction,
    /// COMMIT [WORK]
    CommitTransaction,
    /// ROLLBACK [WORK]
    RollbackTransaction,

    // ===== Live Query Subscriptions =====
    /// SUBSCRIBE TO <namespace>.<table> [WHERE ...] [OPTIONS (...)]
    Subscribe,

    // ===== DML Operations =====
    /// UPDATE <table> SET ... WHERE ...
    Update,
    /// DELETE FROM <table> WHERE ...
    Delete,

    // ===== Standard SQL (DataFusion) =====
    /// SELECT ... (handled by DataFusion)
    Select,
    /// INSERT INTO ... (handled by DataFusion)
    Insert,

    // ===== Unknown/Unsupported =====
    /// Unrecognized statement
    Unknown,
}

impl SqlStatement {
    /// Classify a SQL statement based on its prefix
    ///
    /// This is a fast classification that doesn't fully parse the SQL,
    /// just identifies the statement type for routing purposes.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL string to classify
    ///
    /// # Returns
    ///
    /// The classified statement type
    ///
    /// # Example
    ///
    /// ```
    /// use kalamdb_sql::statement_classifier::SqlStatement;
    ///
    /// let stmt = SqlStatement::classify("CREATE NAMESPACE test");
    /// assert_eq!(stmt, SqlStatement::CreateNamespace);
    ///
    /// let stmt = SqlStatement::classify("SELECT * FROM users");
    /// assert_eq!(stmt, SqlStatement::Select);
    /// ```
    pub fn classify(sql: &str) -> Self {
        let sql_upper = sql.trim().to_uppercase();
        let words: Vec<&str> = sql_upper.split_whitespace().collect();

        if words.is_empty() {
            return SqlStatement::Unknown;
        }

        // Check multi-word prefixes first (most specific to least specific)
        match words.as_slice() {
            // Namespace operations
            ["CREATE", "NAMESPACE", ..] => SqlStatement::CreateNamespace,
            ["ALTER", "NAMESPACE", ..] => SqlStatement::AlterNamespace,
            ["DROP", "NAMESPACE", ..] => SqlStatement::DropNamespace,
            ["SHOW", "NAMESPACES", ..] => SqlStatement::ShowNamespaces,

            // Storage operations
            ["CREATE", "STORAGE", ..] => SqlStatement::CreateStorage,
            ["ALTER", "STORAGE", ..] => SqlStatement::AlterStorage,
            ["DROP", "STORAGE", ..] => SqlStatement::DropStorage,
            ["SHOW", "STORAGES", ..] => SqlStatement::ShowStorages,

            // Table operations
            ["CREATE", "USER", "TABLE", ..] => SqlStatement::CreateTable,
            ["CREATE", "SHARED", "TABLE", ..] => SqlStatement::CreateTable,
            ["CREATE", "STREAM", "TABLE", ..] => SqlStatement::CreateTable,
            ["CREATE", "TABLE", ..] => SqlStatement::CreateTable,
            ["DROP", "USER", "TABLE", ..] => SqlStatement::DropTable,
            ["DROP", "SHARED", "TABLE", ..] => SqlStatement::DropTable,
            ["DROP", "STREAM", "TABLE", ..] => SqlStatement::DropTable,
            ["DROP", "TABLE", ..] => SqlStatement::DropTable,
            ["SHOW", "TABLES", ..] => SqlStatement::ShowTables,
            ["DESCRIBE", "TABLE", ..] => SqlStatement::DescribeTable,
            ["DESC", "TABLE", ..] => SqlStatement::DescribeTable,
            ["SHOW", "STATS", ..] => SqlStatement::ShowStats,

            // Flush operations
            ["FLUSH", "ALL", "TABLES", ..] => SqlStatement::FlushAllTables,
            ["FLUSH", "TABLE", ..] => SqlStatement::FlushTable,

            // Job management
            ["KILL", "JOB", ..] => SqlStatement::KillJob,
            ["KILL", "LIVE", "QUERY", ..] => SqlStatement::KillLiveQuery,

            // Transaction control
            ["BEGIN", ..] | ["START", "TRANSACTION", ..] => SqlStatement::BeginTransaction,
            ["COMMIT", ..] => SqlStatement::CommitTransaction,
            ["ROLLBACK", ..] => SqlStatement::RollbackTransaction,

            // Live query subscriptions
            ["SUBSCRIBE", "TO", ..] => SqlStatement::Subscribe,

            // DML operations (single word start)
            ["UPDATE", ..] => SqlStatement::Update,
            ["DELETE", ..] => SqlStatement::Delete,
            ["SELECT", ..] => SqlStatement::Select,
            ["INSERT", ..] => SqlStatement::Insert,

            // Unknown
            _ => SqlStatement::Unknown,
        }
    }

    /// Check if this statement type requires DataFusion execution
    ///
    /// Returns true for SELECT and INSERT statements that should be
    /// passed to DataFusion for execution.
    pub fn is_datafusion_statement(&self) -> bool {
        matches!(self, SqlStatement::Select | SqlStatement::Insert)
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
            SqlStatement::CreateNamespace => "CREATE NAMESPACE",
            SqlStatement::AlterNamespace => "ALTER NAMESPACE",
            SqlStatement::DropNamespace => "DROP NAMESPACE",
            SqlStatement::ShowNamespaces => "SHOW NAMESPACES",
            SqlStatement::CreateStorage => "CREATE STORAGE",
            SqlStatement::AlterStorage => "ALTER STORAGE",
            SqlStatement::DropStorage => "DROP STORAGE",
            SqlStatement::ShowStorages => "SHOW STORAGES",
            SqlStatement::CreateTable => "CREATE TABLE",
            SqlStatement::DropTable => "DROP TABLE",
            SqlStatement::ShowTables => "SHOW TABLES",
            SqlStatement::DescribeTable => "DESCRIBE TABLE",
            SqlStatement::ShowStats => "SHOW STATS",
            SqlStatement::FlushTable => "FLUSH TABLE",
            SqlStatement::FlushAllTables => "FLUSH ALL TABLES",
            SqlStatement::KillJob => "KILL JOB",
            SqlStatement::KillLiveQuery => "KILL LIVE QUERY",
            SqlStatement::BeginTransaction => "BEGIN",
            SqlStatement::CommitTransaction => "COMMIT",
            SqlStatement::RollbackTransaction => "ROLLBACK",
            SqlStatement::Subscribe => "SUBSCRIBE TO",
            SqlStatement::Update => "UPDATE",
            SqlStatement::Delete => "DELETE",
            SqlStatement::Select => "SELECT",
            SqlStatement::Insert => "INSERT",
            SqlStatement::Unknown => "UNKNOWN",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_namespace_commands() {
        assert_eq!(
            SqlStatement::classify("CREATE NAMESPACE test"),
            SqlStatement::CreateNamespace
        );
        assert_eq!(
            SqlStatement::classify("alter namespace test set description = 'foo'"),
            SqlStatement::AlterNamespace
        );
        assert_eq!(
            SqlStatement::classify("DROP NAMESPACE test CASCADE"),
            SqlStatement::DropNamespace
        );
        assert_eq!(
            SqlStatement::classify("SHOW NAMESPACES"),
            SqlStatement::ShowNamespaces
        );
    }

    #[test]
    fn test_classify_storage_commands() {
        assert_eq!(
            SqlStatement::classify("CREATE STORAGE s3_storage TYPE s3"),
            SqlStatement::CreateStorage
        );
        assert_eq!(
            SqlStatement::classify("ALTER STORAGE local SET description = 'test'"),
            SqlStatement::AlterStorage
        );
        assert_eq!(
            SqlStatement::classify("DROP STORAGE old_storage"),
            SqlStatement::DropStorage
        );
        assert_eq!(
            SqlStatement::classify("SHOW STORAGES"),
            SqlStatement::ShowStorages
        );
    }

    #[test]
    fn test_classify_transactions() {
        assert_eq!(
            SqlStatement::classify("BEGIN"),
            SqlStatement::BeginTransaction
        );
        assert_eq!(
            SqlStatement::classify("BEGIN TRANSACTION"),
            SqlStatement::BeginTransaction
        );
        assert_eq!(
            SqlStatement::classify("COMMIT"),
            SqlStatement::CommitTransaction
        );
        assert_eq!(
            SqlStatement::classify("ROLLBACK"),
            SqlStatement::RollbackTransaction
        );
    }

    #[test]
    fn test_classify_kill_live_query() {
        assert_eq!(
            SqlStatement::classify("KILL LIVE QUERY 'user123-conn_abc-messages-q1'"),
            SqlStatement::KillLiveQuery
        );
    }

    #[test]
    fn test_classify_table_commands() {
        assert_eq!(
            SqlStatement::classify("CREATE USER TABLE test.users (id INT)"),
            SqlStatement::CreateTable
        );
        assert_eq!(
            SqlStatement::classify("CREATE SHARED TABLE test.messages (id INT)"),
            SqlStatement::CreateTable
        );
        assert_eq!(
            SqlStatement::classify("CREATE TABLE test.data (id INT)"),
            SqlStatement::CreateTable
        );
        assert_eq!(
            SqlStatement::classify("DROP TABLE test.users"),
            SqlStatement::DropTable
        );
        assert_eq!(
            SqlStatement::classify("SHOW TABLES"),
            SqlStatement::ShowTables
        );
        assert_eq!(
            SqlStatement::classify("DESCRIBE TABLE test.users"),
            SqlStatement::DescribeTable
        );
        assert_eq!(
            SqlStatement::classify("DESC TABLE test.users"),
            SqlStatement::DescribeTable
        );
    }

    #[test]
    fn test_classify_flush_commands() {
        assert_eq!(
            SqlStatement::classify("FLUSH TABLE test.users"),
            SqlStatement::FlushTable
        );
        assert_eq!(
            SqlStatement::classify("FLUSH ALL TABLES IN test"),
            SqlStatement::FlushAllTables
        );
    }

    #[test]
    fn test_classify_dml_commands() {
        assert_eq!(
            SqlStatement::classify("UPDATE users SET name = 'John'"),
            SqlStatement::Update
        );
        assert_eq!(
            SqlStatement::classify("DELETE FROM users WHERE id = 1"),
            SqlStatement::Delete
        );
        assert_eq!(
            SqlStatement::classify("SELECT * FROM users"),
            SqlStatement::Select
        );
        assert_eq!(
            SqlStatement::classify("INSERT INTO users VALUES (1, 'John')"),
            SqlStatement::Insert
        );
    }

    #[test]
    fn test_classify_job_commands() {
        assert_eq!(
            SqlStatement::classify("KILL JOB job-123"),
            SqlStatement::KillJob
        );
    }

    #[test]
    fn test_classify_subscribe_commands() {
        assert_eq!(
            SqlStatement::classify("SUBSCRIBE TO app.messages"),
            SqlStatement::Subscribe
        );
        assert_eq!(
            SqlStatement::classify("SUBSCRIBE TO app.messages WHERE user_id = 'alice'"),
            SqlStatement::Subscribe
        );
        assert_eq!(
            SqlStatement::classify("subscribe to test.events options (last_rows=10)"),
            SqlStatement::Subscribe
        );
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(
            SqlStatement::classify("GRANT SELECT ON users TO alice"),
            SqlStatement::Unknown
        );
        assert_eq!(SqlStatement::classify(""), SqlStatement::Unknown);
    }

    #[test]
    fn test_is_datafusion_statement() {
        assert!(SqlStatement::Select.is_datafusion_statement());
        assert!(SqlStatement::Insert.is_datafusion_statement());
        assert!(!SqlStatement::CreateTable.is_datafusion_statement());
        assert!(!SqlStatement::Update.is_datafusion_statement());
    }

    #[test]
    fn test_is_custom_command() {
        assert!(SqlStatement::CreateNamespace.is_custom_command());
        assert!(SqlStatement::FlushTable.is_custom_command());
        assert!(SqlStatement::Update.is_custom_command());
        assert!(!SqlStatement::Select.is_custom_command());
        assert!(!SqlStatement::Insert.is_custom_command());
    }
}
