//! DDL statement definitions shared across KalamDB components.
//!
//! This module consolidates DDL statement parsers (CREATE, DROP, ALTER, SHOW, etc.)
//! so they can be reused without depending on `kalamdb-core`.

pub mod parsing;

pub mod alter_namespace;
pub mod alter_table;
pub mod backup_namespace;
pub mod create_namespace;
pub mod create_table; // Unified parser for all table types (USER, SHARED, STREAM)
pub mod describe_table;
pub mod drop_namespace;
pub mod drop_table;
pub mod flush_commands;
pub mod job_commands;
pub mod kill_live_query;
pub mod restore_namespace;
pub mod show_backup;
pub mod show_namespaces;
pub mod show_tables;
pub mod show_table_stats;
pub mod storage_commands;
pub mod subscribe_commands;

/// Result type used by the DDL parsers.
/// Returns String errors to avoid dependencies and allow easy conversion to KalamDbError.
pub type DdlResult<T> = Result<T, String>;

pub use alter_namespace::AlterNamespaceStatement;
pub use alter_table::{AlterTableStatement, ColumnOperation};
pub use backup_namespace::BackupDatabaseStatement;
pub use create_namespace::CreateNamespaceStatement;
pub use create_table::{CreateTableStatement, FlushPolicy};
pub use describe_table::DescribeTableStatement;
pub use drop_namespace::DropNamespaceStatement;
pub use drop_table::{DropTableStatement, TableKind};
pub use flush_commands::{FlushAllTablesStatement, FlushTableStatement};
pub use job_commands::{parse_job_command, JobCommand};
pub use kill_live_query::KillLiveQueryStatement;
pub use restore_namespace::RestoreDatabaseStatement;
pub use show_backup::ShowBackupStatement;
pub use show_namespaces::ShowNamespacesStatement;
pub use show_tables::ShowTablesStatement;
pub use show_table_stats::ShowTableStatsStatement;
pub use storage_commands::{
    AlterStorageStatement, CreateStorageStatement, DropStorageStatement, ShowStoragesStatement,
};
pub use subscribe_commands::{SubscribeStatement, SubscribeOptions};
