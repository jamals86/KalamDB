//! DDL statement definitions shared across KalamDB components.
//!
//! This module consolidates DDL statement parsers (CREATE, DROP, ALTER, SHOW, etc.)
//! so they can be reused without depending on `kalamdb-core`.

pub mod alter_namespace;
pub mod alter_table;
pub mod backup_namespace;
pub mod create_namespace;
pub mod create_shared_table;
pub mod create_stream_table;
pub mod create_user_table;
pub mod describe_table;
pub mod drop_namespace;
pub mod drop_table;
pub mod kill_live_query;
pub mod restore_namespace;
pub mod show_backup;
pub mod show_namespaces;
pub mod show_tables;
pub mod show_table_stats;

/// Result type used by the DDL parsers.
/// Returns String errors to avoid dependencies and allow easy conversion to KalamDbError.
pub type DdlResult<T> = Result<T, String>;

pub use alter_namespace::AlterNamespaceStatement;
pub use alter_table::{AlterTableStatement, ColumnOperation};
pub use backup_namespace::BackupDatabaseStatement;
pub use create_namespace::CreateNamespaceStatement;
pub use create_shared_table::{CreateSharedTableStatement, FlushPolicy};
pub use create_stream_table::CreateStreamTableStatement;
pub use create_user_table::{CreateUserTableStatement, StorageLocation, UserTableFlushPolicy};
pub use describe_table::DescribeTableStatement;
pub use drop_namespace::DropNamespaceStatement;
pub use drop_table::{DropTableStatement, TableKind};
pub use kill_live_query::KillLiveQueryStatement;
pub use restore_namespace::RestoreDatabaseStatement;
pub use show_backup::ShowBackupStatement;
pub use show_namespaces::ShowNamespacesStatement;
pub use show_tables::ShowTablesStatement;
pub use show_table_stats::ShowTableStatsStatement;
