//! DDL (Data Definition Language) statement parsers
//!
//! This module contains parsers for DDL statements like CREATE, ALTER, DROP, and SHOW.

pub mod alter_namespace;
pub mod alter_table;
pub mod backup_namespace;
pub mod create_shared_table;
pub mod create_stream_table;
pub mod describe_table;
pub mod drop_namespace;
pub mod kill_live_query;
pub mod restore_namespace;
pub mod show_backup;
pub mod show_namespaces;
pub mod show_table_stats;
pub mod show_tables;

pub use alter_namespace::AlterNamespaceStatement;
pub use alter_table::{AlterTableStatement, ColumnOperation};
pub use backup_namespace::BackupDatabaseStatement;
pub use create_shared_table::{CreateSharedTableStatement, FlushPolicy};
pub use create_stream_table::CreateStreamTableStatement;
pub use describe_table::DescribeTableStatement;
pub use drop_namespace::DropNamespaceStatement;
pub use kalamdb_sql::ddl::{
    CreateNamespaceStatement, CreateUserTableStatement, DropTableStatement, StorageLocation,
    TableKind, UserTableFlushPolicy,
};
pub use kill_live_query::KillLiveQueryStatement;
pub use restore_namespace::RestoreDatabaseStatement;
pub use show_backup::ShowBackupStatement;
pub use show_namespaces::ShowNamespacesStatement;
pub use show_table_stats::ShowTableStatsStatement;
pub use show_tables::ShowTablesStatement;

impl From<UserTableFlushPolicy> for crate::flush::FlushPolicy {
    fn from(policy: UserTableFlushPolicy) -> Self {
        match policy {
            UserTableFlushPolicy::RowLimit { row_limit } => {
                crate::flush::FlushPolicy::RowLimit { row_limit }
            }
            UserTableFlushPolicy::TimeInterval { interval_seconds } => {
                crate::flush::FlushPolicy::TimeInterval { interval_seconds }
            }
            UserTableFlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => crate::flush::FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            },
        }
    }
}

impl From<&UserTableFlushPolicy> for crate::flush::FlushPolicy {
    fn from(policy: &UserTableFlushPolicy) -> Self {
        policy.clone().into()
    }
}

impl From<TableKind> for crate::catalog::TableType {
    fn from(kind: TableKind) -> Self {
        match kind {
            TableKind::User => crate::catalog::TableType::User,
            TableKind::Shared => crate::catalog::TableType::Shared,
            TableKind::Stream => crate::catalog::TableType::Stream,
        }
    }
}
