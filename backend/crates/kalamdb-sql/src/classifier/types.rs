use crate::ddl::*;
use kalamdb_commons::models::UserId;

/// Comprehensive SQL statement classification for KalamDB
///
/// Each variant either holds a parsed AST (for DDL) or is a marker (for DataFusion queries).
/// This eliminates double-parsing: classify + parse happens in one step.
///
/// Every SqlStatement instance carries the original SQL text for debugging, logging,
/// and DML handler parsing (INSERT/UPDATE/DELETE need sql_text for sqlparser).
#[derive(Debug, Clone)]
pub struct SqlStatement {
    /// Original SQL text
    pub(crate) sql_text: String,
    /// Parsed statement variant
    pub(crate) kind: SqlStatementKind,
    /// Optional AS USER impersonation (Phase 7)
    /// Extracted from "AS USER 'user_id'" clause in DML statements
    pub(crate) as_user_id: Option<UserId>,
}

/// Statement type variants (internal to SqlStatement)
#[derive(Debug, Clone)]
pub enum SqlStatementKind {
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
    /// SHOW MANIFEST
    ShowManifest(ShowManifestStatement),

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

    // ===== Standard SQL (DataFusion/Native) - Typed markers =====
    /// SELECT ... (handled by DataFusion)
    Select,
    /// INSERT INTO ... (native handler with sqlparser)
    Insert(crate::ddl::InsertStatement),
    /// DELETE FROM ... (native handler with sqlparser)
    Delete(crate::ddl::DeleteStatement),
    /// UPDATE <table> SET ... (native handler with sqlparser)
    Update(crate::ddl::UpdateStatement),

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
    /// Create a SqlStatement with SQL text and kind
    pub fn new(sql_text: String, kind: SqlStatementKind) -> Self {
        Self {
            sql_text,
            kind,
            as_user_id: None,
        }
    }

    /// Create a SqlStatement with AS USER impersonation
    pub fn with_as_user(
        sql_text: String,
        kind: SqlStatementKind,
        as_user_id: UserId,
    ) -> Self {
        Self {
            sql_text,
            kind,
            as_user_id: Some(as_user_id),
        }
    }

    /// Get the original SQL text
    pub fn as_str(&self) -> &str {
        &self.sql_text
    }

    /// Get the statement kind (for pattern matching)
    pub fn kind(&self) -> &SqlStatementKind {
        &self.kind
    }

    /// Get the AS USER impersonation user_id if present
    pub fn as_user_id(&self) -> Option<&UserId> {
        self.as_user_id.as_ref()
    }

    /// Check if this is a specific statement kind (helper for tests and matching)
    pub fn is_kind<F>(&self, checker: F) -> bool
    where
        F: FnOnce(&SqlStatementKind) -> bool,
    {
        checker(&self.kind)
    }

    /// Check if this statement type requires DataFusion execution
    ///
    /// Returns true for SELECT, INSERT, DELETE statements that should be
    /// passed to DataFusion for execution.
    pub fn is_datafusion_statement(&self) -> bool {
        matches!(
            self.kind,
            SqlStatementKind::Select | SqlStatementKind::Insert(_) | SqlStatementKind::Delete(_)
        )
    }

    /// Check if this statement type is a custom KalamDB command
    ///
    /// Returns true for all non-standard SQL commands that need
    /// custom execution logic.
    pub fn is_custom_command(&self) -> bool {
        !matches!(
            self.kind,
            SqlStatementKind::Select | SqlStatementKind::Insert(_) | SqlStatementKind::Unknown
        )
    }

    /// Get a human-readable name for this statement type
    pub fn name(&self) -> &'static str {
        match &self.kind {
            SqlStatementKind::CreateNamespace(_) => "CREATE NAMESPACE",
            SqlStatementKind::AlterNamespace(_) => "ALTER NAMESPACE",
            SqlStatementKind::DropNamespace(_) => "DROP NAMESPACE",
            SqlStatementKind::ShowNamespaces(_) => "SHOW NAMESPACES",
            SqlStatementKind::CreateStorage(_) => "CREATE STORAGE",
            SqlStatementKind::AlterStorage(_) => "ALTER STORAGE",
            SqlStatementKind::DropStorage(_) => "DROP STORAGE",
            SqlStatementKind::ShowStorages(_) => "SHOW STORAGES",
            SqlStatementKind::CreateTable(_) => "CREATE TABLE",
            SqlStatementKind::AlterTable(_) => "ALTER TABLE",
            SqlStatementKind::DropTable(_) => "DROP TABLE",
            SqlStatementKind::ShowTables(_) => "SHOW TABLES",
            SqlStatementKind::DescribeTable(_) => "DESCRIBE TABLE",
            SqlStatementKind::ShowStats(_) => "SHOW STATS",
            SqlStatementKind::FlushTable(_) => "FLUSH TABLE",
            SqlStatementKind::FlushAllTables(_) => "FLUSH ALL TABLES",
            SqlStatementKind::ShowManifest(_) => "SHOW MANIFEST",
            SqlStatementKind::KillJob(_) => "KILL JOB",
            SqlStatementKind::KillLiveQuery(_) => "KILL LIVE QUERY",
            SqlStatementKind::BeginTransaction => "BEGIN",
            SqlStatementKind::CommitTransaction => "COMMIT",
            SqlStatementKind::RollbackTransaction => "ROLLBACK",
            SqlStatementKind::Subscribe(_) => "SUBSCRIBE TO",
            SqlStatementKind::CreateUser(_) => "CREATE USER",
            SqlStatementKind::AlterUser(_) => "ALTER USER",
            SqlStatementKind::DropUser(_) => "DROP USER",
            SqlStatementKind::Update(_) => "UPDATE",
            SqlStatementKind::Delete(_) => "DELETE",
            SqlStatementKind::Select => "SELECT",
            SqlStatementKind::Insert(_) => "INSERT",
            SqlStatementKind::Unknown => "UNKNOWN",
        }
    }
}
