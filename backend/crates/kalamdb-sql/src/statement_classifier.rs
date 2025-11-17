//! SQL Statement Classifier
//!
//! Fast SQL statement classification with integrated parsing.
//! Parses DDL statements during classification to avoid double-parsing.

use crate::ddl::*;

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
    sql_text: String,
    /// Parsed statement variant
    kind: SqlStatementKind,
    /// Optional AS USER impersonation (Phase 7)
    /// Extracted from "AS USER 'user_id'" clause in DML statements
    as_user_id: Option<kalamdb_commons::models::UserId>,
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
        as_user_id: kalamdb_commons::models::UserId,
    ) -> Self {
        Self {
            sql_text,
            kind,
            as_user_id: Some(as_user_id),
        }
    }

    /// Extract AS USER 'user_id' clause from SQL if present
    /// Returns (cleaned_sql, optional_user_id)
    fn extract_as_user(sql: &str) -> (String, Option<kalamdb_commons::models::UserId>) {
        let upper = sql.to_uppercase();

        // Find "AS USER 'user_id'" or "AS USER \"user_id\"" pattern
        if let Some(as_user_pos) = upper.find(" AS USER ") {
            // Extract after "AS USER "
            let after_as_user = &sql[as_user_pos + 9..].trim_start();

            // Determine quote type
            let user_id_str = if after_as_user.starts_with('\'') {
                // Single quote: 'user_id'
                if let Some(end_quote) = after_as_user[1..].find('\'') {
                    &after_as_user[1..end_quote + 1]
                } else {
                    return (sql.to_string(), None); // Malformed, ignore
                }
            } else if after_as_user.starts_with('"') {
                // Double quote: "user_id"
                if let Some(end_quote) = after_as_user[1..].find('"') {
                    &after_as_user[1..end_quote + 1]
                } else {
                    return (sql.to_string(), None); // Malformed, ignore
                }
            } else {
                return (sql.to_string(), None); // No quote, ignore
            };

            // Remove AS USER clause from SQL
            let before_as_user = &sql[..as_user_pos];
            let quote_end_pos = as_user_pos + 9 + 1 + user_id_str.len() + 1; // " AS USER " + quote + user_id + quote
            let after_as_user_clause = if quote_end_pos < sql.len() {
                &sql[quote_end_pos..]
            } else {
                ""
            };

            let cleaned_sql = format!("{} {}", before_as_user.trim(), after_as_user_clause.trim())
                .trim()
                .to_string();
            let user_id = kalamdb_commons::models::UserId::from(user_id_str.to_string());

            (cleaned_sql, Some(user_id))
        } else {
            (sql.to_string(), None)
        }
    }

    /// Wrap a parsed statement into SqlStatement with sql_text
    fn wrap<F>(sql: &str, parser: F) -> Self
    where
        F: FnOnce() -> Option<SqlStatementKind>,
    {
        Self::new(
            sql.to_string(),
            parser().unwrap_or(SqlStatementKind::Unknown),
        )
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
    pub fn as_user_id(&self) -> Option<&kalamdb_commons::models::UserId> {
        self.as_user_id.as_ref()
    }

    /// Check if this is a specific statement kind (helper for tests and matching)
    pub fn is_kind<F>(&self, checker: F) -> bool
    where
        F: FnOnce(&SqlStatementKind) -> bool,
    {
        checker(&self.kind)
    }

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
        .unwrap_or_else(|_| Self::new(sql.to_string(), SqlStatementKind::Unknown))
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

        // Use sqlparser's tokenizer to get the first keyword (skips comments automatically)
        let dialect = sqlparser::dialect::GenericDialect {};
        let mut tokenizer = sqlparser::tokenizer::Tokenizer::new(&dialect, sql);
        let tokens = match tokenizer.tokenize() {
            Ok(t) => t,
            Err(_) => {
                // If tokenization fails, use simple whitespace split as fallback
                let sql_upper = sql.trim().to_uppercase();
                let words: Vec<&str> = sql_upper.split_whitespace().collect();
                if words.is_empty() {
                    return Ok(Self::new(sql.to_string(), SqlStatementKind::Unknown));
                }
                // Continue with simple word-based matching
                vec![]
            }
        };

        // Get first non-whitespace, non-comment token
        let first_keyword_upper = if !tokens.is_empty() {
            tokens
                .iter()
                .find_map(|tok| match tok {
                    sqlparser::tokenizer::Token::Word(w) => Some(w.value.to_uppercase()),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    // Fallback to simple parsing
                    let sql_upper = sql.trim().to_uppercase();
                    let words: Vec<&str> = sql_upper.split_whitespace().collect();
                    words.first().map(|s| s.to_string()).unwrap_or_default()
                })
        } else {
            // Fallback to simple parsing
            let sql_upper = sql.trim().to_uppercase();
            let words: Vec<&str> = sql_upper.split_whitespace().collect();
            words.first().map(|s| s.to_string()).unwrap_or_default()
        };

        // Build words list from non-comment tokens for pattern matching
        let words: Vec<String> = if !tokens.is_empty() {
            tokens
                .iter()
                .filter_map(|tok| match tok {
                    sqlparser::tokenizer::Token::Word(w) => Some(w.value.to_uppercase()),
                    _ => None,
                })
                .collect()
        } else {
            let sql_upper = sql.trim().to_uppercase();
            sql_upper
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        };
        let word_refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();

        if word_refs.is_empty() {
            return Ok(Self::new(sql.to_string(), SqlStatementKind::Unknown));
        }

        // Admin users (DBA, System) can do anything - skip authorization checks
        let is_admin = matches!(role, Role::Dba | Role::System);

        // Hot path: Check SELECT/INSERT/DELETE first (99% of queries)
        // DML statements - create typed markers for handler pattern
        match first_keyword_upper.as_str() {
            "SELECT" => return Ok(Self::new(sql.to_string(), SqlStatementKind::Select)),
            "INSERT" => {
                // T151: Extract AS USER clause from INSERT statement (Phase 7)
                let (cleaned_sql2, as_user_id) = Self::extract_as_user(sql);
                return if let Some(user_id) = as_user_id {
                    Ok(Self::with_as_user(
                        cleaned_sql2,
                        SqlStatementKind::Insert(crate::ddl::InsertStatement),
                        user_id,
                    ))
                } else {
                    Ok(Self::new(
                        sql.to_string(),
                        SqlStatementKind::Insert(crate::ddl::InsertStatement),
                    ))
                };
            }
            "DELETE" => {
                // T151: Extract AS USER clause from DELETE statement (Phase 7)
                let (cleaned_sql2, as_user_id) = Self::extract_as_user(sql);
                return if let Some(user_id) = as_user_id {
                    Ok(Self::with_as_user(
                        cleaned_sql2,
                        SqlStatementKind::Delete(crate::ddl::DeleteStatement),
                        user_id,
                    ))
                } else {
                    Ok(Self::new(
                        sql.to_string(),
                        SqlStatementKind::Delete(crate::ddl::DeleteStatement),
                    ))
                };
            }
            "UPDATE" => {
                // T151: Extract AS USER clause from UPDATE statement (Phase 7)
                let (cleaned_sql2, as_user_id) = Self::extract_as_user(sql);
                return if let Some(user_id) = as_user_id {
                    Ok(Self::with_as_user(
                        cleaned_sql2,
                        SqlStatementKind::Update(crate::ddl::UpdateStatement),
                        user_id,
                    ))
                } else {
                    Ok(Self::new(
                        sql.to_string(),
                        SqlStatementKind::Update(crate::ddl::UpdateStatement),
                    ))
                };
            }
            _ => {}
        }

        // Check multi-word prefixes and parse DDL statements
        match word_refs.as_slice() {
            // Namespace operations - require admin
            ["CREATE", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for namespace operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    CreateNamespaceStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::CreateNamespace)
                }))
            }
            ["ALTER", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for namespace operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    AlterNamespaceStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::AlterNamespace)
                }))
            }
            ["DROP", "NAMESPACE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for namespace operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    DropNamespaceStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::DropNamespace)
                }))
            }
            ["SHOW", "NAMESPACES", ..] => {
                // Read-only, allowed for all users
                Ok(Self::wrap(sql, || {
                    ShowNamespacesStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowNamespaces)
                }))
            }

            // Storage operations - require admin
            ["CREATE", "STORAGE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for storage operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    CreateStorageStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::CreateStorage)
                }))
            }
            ["ALTER", "STORAGE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for storage operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    AlterStorageStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::AlterStorage)
                }))
            }
            ["DROP", "STORAGE", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for storage operations"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    DropStorageStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::DropStorage)
                }))
            }
            ["SHOW", "STORAGES", ..] => {
                // Read-only, allowed for all users
                Ok(Self::wrap(sql, || {
                    ShowStoragesStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowStorages)
                }))
            }

            // Table operations - authorization deferred to table ownership checks
            ["CREATE", "USER", "TABLE", ..]
            | ["CREATE", "SHARED", "TABLE", ..]
            | ["CREATE", "STREAM", "TABLE", ..]
            | ["CREATE", "TABLE", ..] => {
                // Parse CREATE TABLE statement with detailed error logging
                match CreateTableStatement::parse(sql, default_namespace) {
                    Ok(stmt) => Ok(Self::new(
                        sql.to_string(),
                        SqlStatementKind::CreateTable(stmt),
                    )),
                    Err(e) => {
                        log::error!(
                            target: "sql::parse",
                            "❌ CREATE TABLE parsing failed | sql='{}' | error='{}'",
                            sql,
                            e
                        );
                        Ok(Self::new(sql.to_string(), SqlStatementKind::Unknown))
                    }
                }
            }
            ["ALTER", "TABLE", ..]
            | ["ALTER", "USER", "TABLE", ..]
            | ["ALTER", "SHARED", "TABLE", ..]
            | ["ALTER", "STREAM", "TABLE", ..] => Ok(Self::wrap(sql, || {
                AlterTableStatement::parse(sql, default_namespace)
                    .ok()
                    .map(SqlStatementKind::AlterTable)
            })),
            ["DROP", "USER", "TABLE", ..]
            | ["DROP", "SHARED", "TABLE", ..]
            | ["DROP", "STREAM", "TABLE", ..]
            | ["DROP", "TABLE", ..] => Ok(Self::wrap(sql, || {
                DropTableStatement::parse(sql, default_namespace)
                    .ok()
                    .map(SqlStatementKind::DropTable)
            })),
            ["SHOW", "TABLES", ..] => {
                // Read-only, allowed for all users
                Ok(Self::wrap(sql, || {
                    ShowTablesStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowTables)
                }))
            }
            ["DESCRIBE", "TABLE", ..] | ["DESC", "TABLE", ..] => {
                // Read-only, allowed for all users
                Ok(Self::wrap(sql, || {
                    DescribeTableStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::DescribeTable)
                }))
            }
            ["SHOW", "STATS", ..] => {
                // Read-only, allowed for all users
                Ok(Self::wrap(sql, || {
                    ShowTableStatsStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowStats)
                }))
            }

            // Flush operations - authorization deferred to table ownership checks
            ["FLUSH", "ALL", "TABLES", ..] => Ok(Self::wrap(sql, || {
                FlushAllTablesStatement::parse_with_default(sql, default_namespace)
                    .ok()
                    .map(SqlStatementKind::FlushAllTables)
            })),
            ["FLUSH", "TABLE", ..] => Ok(Self::wrap(sql, || {
                FlushTableStatement::parse(sql)
                    .ok()
                    .map(SqlStatementKind::FlushTable)
            })),
            ["SHOW", "MANIFEST"] => {
                // SHOW MANIFEST command for inspecting manifest cache
                Ok(Self::wrap(sql, || {
                    ShowManifestStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowManifest)
                }))
            }
            ["SHOW", "MANIFEST", "CACHE", ..] => {
                // Legacy alias for SHOW MANIFEST - backwards compatibility
                Ok(Self::wrap(sql, || {
                    ShowManifestStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::ShowManifest)
                }))
            }

            // Job management - require admin
            ["KILL", "JOB", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for job management"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    parse_job_command(sql).ok().map(SqlStatementKind::KillJob)
                }))
            }
            ["KILL", "LIVE", "QUERY", ..] => {
                // Users can kill their own live queries
                Ok(Self::wrap(sql, || {
                    KillLiveQueryStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::KillLiveQuery)
                }))
            }

            // Transaction control (no parsing needed - just markers)
            ["BEGIN", ..] | ["START", "TRANSACTION", ..] => Ok(Self::new(
                sql.to_string(),
                SqlStatementKind::BeginTransaction,
            )),
            ["COMMIT", ..] => Ok(Self::new(
                sql.to_string(),
                SqlStatementKind::CommitTransaction,
            )),
            ["ROLLBACK", ..] => Ok(Self::new(
                sql.to_string(),
                SqlStatementKind::RollbackTransaction,
            )),

            // Live query subscriptions - allowed for all users
            ["SUBSCRIBE", "TO", ..] => Ok(Self::wrap(sql, || {
                SubscribeStatement::parse(sql)
                    .ok()
                    .map(SqlStatementKind::Subscribe)
            })),

            // User management - require admin (except ALTER USER for self)
            ["CREATE", "USER", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for user management"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    CreateUserStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::CreateUser)
                }))
            }
            ["ALTER", "USER", ..] => {
                // Authorization deferred to handler (users can alter their own account)
                Ok(Self::wrap(sql, || {
                    AlterUserStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::AlterUser)
                }))
            }
            ["DROP", "USER", ..] => {
                if !is_admin {
                    return Err(
                        "Admin privileges (DBA or System role) required for user management"
                            .to_string(),
                    );
                }
                Ok(Self::wrap(sql, || {
                    DropUserStatement::parse(sql)
                        .ok()
                        .map(SqlStatementKind::DropUser)
                }))
            }

            // Unknown
            _ => Ok(Self::new(sql.to_string(), SqlStatementKind::Unknown)),
        }
    }

    // (helper moved to top-level to avoid nested impl issues)

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

        match &self.kind {
            // Storage and global operations require admin privileges
            SqlStatementKind::CreateStorage(_)
            | SqlStatementKind::AlterStorage(_)
            | SqlStatementKind::DropStorage(_)
            | SqlStatementKind::KillJob(_) => Err(
                "Admin privileges (DBA or System role) required for storage and job operations"
                    .to_string(),
            ),

            // User management requires admin privileges (except for self-modification in ALTER USER)
            SqlStatementKind::CreateUser(_) | SqlStatementKind::DropUser(_) => Err(
                "Admin privileges (DBA or System role) required for user management".to_string(),
            ),

            // ALTER USER allowed for self (changing own password), admin for others
            // The actual target user check is deferred to execute_alter_user method
            SqlStatementKind::AlterUser(_) => Ok(()),

            // Namespace DDL requires admin privileges
            SqlStatementKind::CreateNamespace(_)
            | SqlStatementKind::AlterNamespace(_)
            | SqlStatementKind::DropNamespace(_) => Err(
                "Admin privileges (DBA or System role) required for namespace operations"
                    .to_string(),
            ),

            // Read-only operations on system tables are allowed for all authenticated users
            SqlStatementKind::ShowNamespaces(_)
            | SqlStatementKind::ShowTables(_)
            | SqlStatementKind::ShowStorages(_)
            | SqlStatementKind::ShowStats(_)
            | SqlStatementKind::ShowManifest(_)
            | SqlStatementKind::DescribeTable(_) => Ok(()),

            // CREATE TABLE, DROP TABLE, FLUSH TABLE, ALTER TABLE - defer to table ownership checks
            SqlStatementKind::CreateTable(_)
            | SqlStatementKind::AlterTable(_)
            | SqlStatementKind::DropTable(_)
            | SqlStatementKind::FlushTable(_)
            | SqlStatementKind::FlushAllTables(_) => {
                // Table-level authorization will be checked in the execution methods
                // Users can only create/modify/drop tables they own
                // Admin users can operate on any table (already returned above)
                Ok(())
            }

            // SELECT, INSERT, UPDATE, DELETE - defer to table access control
            SqlStatementKind::Select
            | SqlStatementKind::Insert(_)
            | SqlStatementKind::Update(_)
            | SqlStatementKind::Delete(_) => {
                // Query-level authorization will be enforced by using per-user sessions
                // User tables are filtered by user_id in UserTableProvider
                // Shared tables enforce access control based on access_level
                Ok(())
            }

            // Subscriptions, transactions, and other operations allowed for all users
            SqlStatementKind::Subscribe(_)
            | SqlStatementKind::KillLiveQuery(_)
            | SqlStatementKind::BeginTransaction
            | SqlStatementKind::CommitTransaction
            | SqlStatementKind::RollbackTransaction => Ok(()),

            SqlStatementKind::Unknown => {
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
            SqlStatement::classify("CREATE NAMESPACE test").kind(),
            SqlStatementKind::CreateNamespace(_)
        ));
        // Note: ALTER NAMESPACE syntax not fully supported by parser yet
        // assert!(matches!(
        //     SqlStatement::classify("alter namespace test set description = 'foo'").kind(),
        //     SqlStatementKind::AlterNamespace(_)
        // ));
        assert!(matches!(
            SqlStatement::classify("DROP NAMESPACE test CASCADE").kind(),
            SqlStatementKind::DropNamespace(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SHOW NAMESPACES").kind(),
            SqlStatementKind::ShowNamespaces(_)
        ));
    }

    #[test]
    fn test_classify_storage_commands() {
        // Note: Storage parsers need implementation - returning Unknown for now
        // assert!(matches!(
        //     SqlStatement::classify("CREATE STORAGE s3_storage TYPE s3").kind(),
        //     SqlStatementKind::CreateStorage(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("ALTER STORAGE local SET description = 'test'").kind(),
        //     SqlStatementKind::AlterStorage(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("DROP STORAGE old_storage").kind(),
        //     SqlStatementKind::DropStorage(_)
        // ));
        assert!(matches!(
            SqlStatement::classify("SHOW STORAGES").kind(),
            SqlStatementKind::ShowStorages(_)
        ));
    }

    #[test]
    fn test_classify_transactions() {
        assert!(matches!(
            SqlStatement::classify("BEGIN").kind(),
            SqlStatementKind::BeginTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("BEGIN TRANSACTION").kind(),
            SqlStatementKind::BeginTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("COMMIT").kind(),
            SqlStatementKind::CommitTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("ROLLBACK").kind(),
            SqlStatementKind::RollbackTransaction
        ));
    }

    #[test]
    fn test_classify_kill_live_query() {
        assert!(matches!(
            SqlStatement::classify("KILL LIVE QUERY 'user123-conn_abc-messages-q1'").kind(),
            SqlStatementKind::KillLiveQuery(_)
        ));
    }

    #[test]
    fn test_classify_table_commands() {
        assert!(matches!(
            SqlStatement::classify("CREATE USER TABLE test.users (id INT)").kind(),
            SqlStatementKind::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("CREATE SHARED TABLE test.messages (id INT)").kind(),
            SqlStatementKind::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("CREATE TABLE test.data (id INT)").kind(),
            SqlStatementKind::CreateTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER TABLE test.users ADD COLUMN email TEXT").kind(),
            SqlStatementKind::AlterTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER SHARED TABLE test.messages SET ACCESS LEVEL public")
                .kind(),
            SqlStatementKind::AlterTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DROP TABLE test.users").kind(),
            SqlStatementKind::DropTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SHOW TABLES").kind(),
            SqlStatementKind::ShowTables(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DESCRIBE TABLE test.users").kind(),
            SqlStatementKind::DescribeTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DESC TABLE test.users").kind(),
            SqlStatementKind::DescribeTable(_)
        ));
    }

    #[test]
    fn test_classify_flush_commands() {
        assert!(matches!(
            SqlStatement::classify("FLUSH TABLE test.users").kind(),
            SqlStatementKind::FlushTable(_)
        ));
        assert!(matches!(
            SqlStatement::classify("FLUSH ALL TABLES").kind(),
            SqlStatementKind::FlushAllTables(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SHOW MANIFEST").kind(),
            SqlStatementKind::ShowManifest(_)
        ));
        assert!(matches!(
            SqlStatement::classify("show manifest").kind(),
            SqlStatementKind::ShowManifest(_)
        ));
    }

    #[test]
    fn test_classify_dml_commands() {
        assert!(matches!(
            SqlStatement::classify("UPDATE users SET name = 'John'").kind(),
            SqlStatementKind::Update(_)
        ));
        assert!(matches!(
            SqlStatement::classify("DELETE FROM users WHERE id = 1").kind(),
            SqlStatementKind::Delete(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SELECT * FROM users").kind(),
            SqlStatementKind::Select
        ));
        assert!(matches!(
            SqlStatement::classify("INSERT INTO users VALUES (1, 'John')").kind(),
            SqlStatementKind::Insert(_)
        ));
    }

    #[test]
    fn test_classify_job_commands() {
        assert!(matches!(
            SqlStatement::classify("KILL JOB job-123").kind(),
            SqlStatementKind::KillJob(_)
        ));
    }

    #[test]
    fn test_classify_subscribe_commands() {
        assert!(matches!(
            SqlStatement::classify("SUBSCRIBE TO app.messages").kind(),
            SqlStatementKind::Subscribe(_)
        ));
        assert!(matches!(
            SqlStatement::classify("SUBSCRIBE TO app.messages WHERE user_id = 'alice'").kind(),
            SqlStatementKind::Subscribe(_)
        ));
        assert!(matches!(
            SqlStatement::classify("subscribe to test.events options (last_rows=10)").kind(),
            SqlStatementKind::Subscribe(_)
        ));
    }

    #[test]
    fn test_classify_user_commands() {
        assert!(matches!(
            SqlStatement::classify("CREATE USER alice WITH PASSWORD 'secret123' ROLE developer")
                .kind(),
            SqlStatementKind::CreateUser(_)
        ));
        assert!(matches!(
            SqlStatement::classify(
                "create user bob with oauth email='bob@example.com' role readonly"
            )
            .kind(),
            SqlStatementKind::CreateUser(_)
        ));
        assert!(matches!(
            SqlStatement::classify("ALTER USER alice SET PASSWORD 'newpass'").kind(),
            SqlStatementKind::AlterUser(_)
        ));
        // Note: ALTER USER SET ROLE might have parser issues
        // assert!(matches!(
        //     SqlStatement::classify("alter user bob set role dba").kind(),
        //     SqlStatementKind::AlterUser(_)
        // ));

        // Note: DROP USER parser needs implementation
        // assert!(matches!(
        //     SqlStatement::classify("DROP USER alice").kind(),
        //     SqlStatementKind::DropUser(_)
        // ));
        // assert!(matches!(
        //     SqlStatement::classify("drop user old_user").kind(),
        //     SqlStatementKind::DropUser(_)
        // ));
    }

    #[test]
    fn test_classify_unknown() {
        assert!(matches!(
            SqlStatement::classify("GRANT SELECT ON users TO alice").kind(),
            SqlStatementKind::Unknown
        ));
        assert!(matches!(
            SqlStatement::classify("").kind(),
            SqlStatementKind::Unknown
        ));
    }

    #[test]
    fn test_is_datafusion_statement() {
        assert!(
            SqlStatement::new("SELECT".to_string(), SqlStatementKind::Select)
                .is_datafusion_statement()
        );
        assert!(SqlStatement::new(
            "INSERT".to_string(),
            SqlStatementKind::Insert(crate::ddl::InsertStatement)
        )
        .is_datafusion_statement());
        assert!(SqlStatement::new(
            "DELETE".to_string(),
            SqlStatementKind::Delete(crate::ddl::DeleteStatement)
        )
        .is_datafusion_statement());
        let create_table = SqlStatement::classify("CREATE TABLE test (id INT)");
        assert!(!create_table.is_datafusion_statement());
        assert!(!SqlStatement::new(
            "UPDATE".to_string(),
            SqlStatementKind::Update(crate::ddl::UpdateStatement)
        )
        .is_datafusion_statement());
    }

    #[test]
    fn test_is_custom_command() {
        let create_ns = SqlStatement::classify("CREATE NAMESPACE test");
        assert!(create_ns.is_custom_command());
        let flush = SqlStatement::classify("FLUSH TABLE test.users"); // Use full table name
        assert!(flush.is_custom_command());
        assert!(SqlStatement::new(
            "UPDATE".to_string(),
            SqlStatementKind::Update(crate::ddl::UpdateStatement)
        )
        .is_custom_command());
        assert!(
            !SqlStatement::new("SELECT".to_string(), SqlStatementKind::Select).is_custom_command()
        );
        assert!(!SqlStatement::new(
            "INSERT".to_string(),
            SqlStatementKind::Insert(crate::ddl::InsertStatement)
        )
        .is_custom_command());
    }
}
