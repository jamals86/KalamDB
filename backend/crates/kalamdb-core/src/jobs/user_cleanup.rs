//! User cleanup job for removing expired soft-deleted users.
//!
//! This job scans `system.users` for soft-deleted users where the
//! `deleted_at` timestamp is older than the configured grace period.
//! For each expired user it:
//! - Deletes all per-user rows from USER tables
//! - Permanently removes the user record from `system.users`
//! - Emits structured log messages for observability

use crate::error::KalamDbError;
use crate::tables::UserTableStore;
use kalamdb_commons::models::UserName;
use kalamdb_commons::system::User as SystemUser;
use kalamdb_commons::TableType;
use kalamdb_sql::{KalamSql, Table};
use std::sync::Arc;

/// Configuration for user cleanup job.
#[derive(Debug, Clone)]
pub struct UserCleanupConfig {
    /// Number of days after soft delete before a user is permanently removed.
    pub grace_period_days: i64,
}

impl Default for UserCleanupConfig {
    fn default() -> Self {
        Self {
            grace_period_days: 30,
        }
    }
}

/// Job for enforcing user cleanup rules.
pub struct UserCleanupJob {
    kalam_sql: Arc<KalamSql>,
    user_table_store: Arc<UserTableStore>,
    config: UserCleanupConfig,
}

impl UserCleanupJob {
    /// Create a new cleanup job with explicit configuration.
    pub fn new(
        kalam_sql: Arc<KalamSql>,
        user_table_store: Arc<UserTableStore>,
        config: UserCleanupConfig,
    ) -> Self {
        Self {
            kalam_sql,
            user_table_store,
            config,
        }
    }

    /// Create a cleanup job using default configuration (30-day grace period).
    pub fn with_defaults(kalam_sql: Arc<KalamSql>, user_table_store: Arc<UserTableStore>) -> Self {
        Self::new(kalam_sql, user_table_store, UserCleanupConfig::default())
    }

    /// Get current configuration.
    pub fn config(&self) -> &UserCleanupConfig {
        &self.config
    }

    /// Update configuration at runtime.
    pub fn set_config(&mut self, config: UserCleanupConfig) {
        self.config = config;
    }

    /// Enforce cleanup policy and return count of permanently deleted users.
    pub fn enforce(&self) -> Result<usize, KalamDbError> {
        let grace_days = self.config.grace_period_days.max(0);
        let grace_period_ms = grace_days * 24 * 60 * 60 * 1000;
        let cutoff = chrono::Utc::now().timestamp_millis() - grace_period_ms;

        let users = self
            .kalam_sql
            .scan_all_users()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan users: {}", e)))?;

        let user_tables: Vec<Table> = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan tables: {}", e)))?
            .into_iter()
            .filter(|t| t.table_type == TableType::User)
            .collect();

        let mut deleted_users = 0;

        for user in users
            .into_iter()
            .filter(|u| u.deleted_at.map(|ts| ts < cutoff).unwrap_or(false))
        {
            match self.cleanup_user(&user, &user_tables) {
                Ok(_) => {
                    deleted_users += 1;
                }
                Err(err) => {
                    log::error!(
                        "User cleanup failed for {}: {}",
                        <UserName as AsRef<str>>::as_ref(&user.username),
                        err
                    );
                }
            }
        }

        Ok(deleted_users)
    }

    /// Cleanup resources for a single user (per-user table rows + user record).
    fn cleanup_user(&self, user: &SystemUser, user_tables: &[Table]) -> Result<(), KalamDbError> {
        let user_id = user.id.as_str();

        for table in user_tables {
            let namespace = table.namespace.as_str();
            let table_name = table.table_name.as_str();

            let removed_rows = self
                .user_table_store
                .delete_all_for_user(namespace, table_name, user_id)
                .map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to delete rows for {}.{}: {}",
                        namespace, table_name, e
                    ))
                })?;

            if removed_rows > 0 {
                log::info!(
                    "Removed {} row(s) from {}.{} for user {}",
                    removed_rows,
                    namespace,
                    table_name,
                    <UserName as AsRef<str>>::as_ref(&user.username)
                );
            }
        }

        self.kalam_sql
            .delete_user(<UserName as AsRef<str>>::as_ref(&user.username))
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to permanently delete user {}: {}",
                    <UserName as AsRef<str>>::as_ref(&user.username),
                    e
                ))
            })?;

        log::info!(
            "Permanently deleted user {} ({})",
            <UserName as AsRef<str>>::as_ref(&user.username),
            user_id
        );

        Ok(())
    }
}
