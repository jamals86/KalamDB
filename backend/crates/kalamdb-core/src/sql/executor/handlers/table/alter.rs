//! Typed DDL handler for ALTER TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::helpers::guards::block_system_namespace_modification;
// Note: table_registration moved to unified applier commands
use crate::sql::context::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
use kalamdb_commons::models::{NamespaceId, TableId};
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_sql::ddl::{AlterTableStatement, ColumnOperation};
use std::sync::Arc;

/// Typed handler for ALTER TABLE statements
pub struct AlterTableHandler {
    app_context: Arc<AppContext>,
}

impl AlterTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    /// Build the altered table definition without persisting or registering providers.
    /// This validates inputs and applies the schema mutation.
    fn build_altered_table_definition(
        &self,
        statement: &AlterTableStatement,
        context: &ExecutionContext,
    ) -> Result<(TableDefinition, String, bool), KalamDbError> {
        let namespace_id: NamespaceId = statement.namespace_id.clone();
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        log::info!(
            "🔧 ALTER TABLE request: {}.{} (operation: {:?}, user: {}, role: {:?})",
            namespace_id.as_str(),
            statement.table_name.as_str(),
            get_operation_summary(&statement.operation),
            context.user_id().as_str(),
            context.user_role()
        );

        // Block ALTER on system tables
        block_system_namespace_modification(
            &namespace_id,
            "ALTER",
            "TABLE",
            Some(statement.table_name.as_str()),
        )?;

        let registry = self.app_context.schema_registry();
        let table_def_arc = registry
            .get_table_if_exists(&table_id)?
            .ok_or_else(|| {
                log::warn!(
                    "⚠️  ALTER TABLE failed: Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    namespace_id.as_str()
                );
                KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    namespace_id.as_str()
                ))
            })?;

        let mut table_def: TableDefinition = (*table_def_arc).clone();

        log::debug!(
            "📋 Current table schema: type={:?}, columns={}, version={}",
            table_def.table_type,
            table_def.columns.len(),
            table_def.schema_version
        );

        // RBAC check
        let is_owner = matches!(table_def.table_type, TableType::User);

        if !kalamdb_session::can_alter_table(context.user_role(), table_def.table_type, is_owner) {
            log::error!(
                "❌ ALTER TABLE {}.{}: Insufficient privileges",
                namespace_id.as_str(),
                statement.table_name.as_str()
            );
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter table".to_string(),
            ));
        }

        // Apply operation and get change description + whether anything actually changed
        let (change_desc, changed) =
            apply_alter_operation(&mut table_def, &statement.operation, &table_id)?;

        // Only increment version if actual changes were made
        if changed {
            table_def.increment_version();
            log::debug!(
                "✓ Built altered TableDefinition: version={}, columns={}",
                table_def.schema_version,
                table_def.columns.len()
            );
        } else {
            log::debug!(
                "⊙ No changes made to TableDefinition: version={} unchanged",
                table_def.schema_version
            );
        }

        Ok((table_def, change_desc, changed))
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterTableStatement> for AlterTableHandler {
    async fn execute(
        &self,
        statement: AlterTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use crate::sql::executor::helpers::audit;

        let namespace_id: NamespaceId = statement.namespace_id.clone();
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        // Build the altered table definition (validate + apply mutation)
        let (table_def, change_desc, changed) =
            self.build_altered_table_definition(&statement, context)?;

        // Only apply changes if something actually changed
        if changed {
            // Delegate to unified applier - pass raw parameters
            self.app_context
                .applier()
                .alter_table(table_id.clone(), table_def.clone())
                .await
                .map_err(|e| KalamDbError::ExecutionError(format!("ALTER TABLE failed: {}", e)))?;

            // Log DDL operation
            let audit_entry = audit::log_ddl_operation(
                context,
                "ALTER",
                "TABLE",
                &format!("{}.{}", namespace_id.as_str(), statement.table_name.as_str()),
                Some(format!(
                    "Operation: {}, New Version: {}",
                    change_desc, table_def.schema_version
                )),
                None,
            );
            audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

            log::info!(
                "✅ ALTER TABLE succeeded: {}.{} | operation: {} | new_version: {} | table_type: {:?}",
                namespace_id.as_str(),
                statement.table_name.as_str(),
                change_desc,
                table_def.schema_version,
                table_def.table_type
            );

            Ok(ExecutionResult::Success {
                message: format!(
                    "Table {}.{} altered successfully: {} (version {})",
                    namespace_id.as_str(),
                    statement.table_name.as_str(),
                    change_desc,
                    table_def.schema_version
                ),
            })
        } else {
            log::info!(
                "⊙ ALTER TABLE no-op: {}.{} | operation: {} | version unchanged: {}",
                namespace_id.as_str(),
                statement.table_name.as_str(),
                change_desc,
                table_def.schema_version
            );

            Ok(ExecutionResult::Success {
                message: format!(
                    "Table {}.{} unchanged: {} (version {} - no changes needed)",
                    namespace_id.as_str(),
                    statement.table_name.as_str(),
                    change_desc,
                    table_def.schema_version
                ),
            })
        }
    }

    async fn check_authorization(
        &self,
        statement: &AlterTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;

        // Block anonymous users from DDL operations
        block_anonymous_write(context, "ALTER TABLE")?;

        let namespace_id = &statement.namespace_id;
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        let registry = self.app_context.schema_registry();
        if let Ok(Some(def)) = registry.get_table_if_exists(&table_id) {
            let is_owner = matches!(def.table_type, TableType::User);

            if !kalamdb_session::can_alter_table(context.user_role(), def.table_type, is_owner) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to alter table".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Check if a column name is a system column that cannot be altered
fn is_system_column(column_name: &str) -> bool {
    SystemColumnNames::is_system_column(column_name)
}

/// Apply an ALTER TABLE operation to a table definition
/// Returns (description, changed) tuple where changed is false if no actual modifications were made
fn apply_alter_operation(
    table_def: &mut TableDefinition,
    operation: &ColumnOperation,
    table_id: &TableId,
) -> Result<(String, bool), KalamDbError> {
    match operation {
        ColumnOperation::Add {
            column_name,
            data_type,
            nullable,
            default_value,
        } => {
            // Block adding columns with system column names
            if is_system_column(column_name) {
                log::error!("❌ ALTER TABLE failed: Cannot add system column '{}'", column_name);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot add column '{}': reserved system column name",
                    column_name
                )));
            }
            // Perform case-insensitive check to prevent duplicates like 'col1' vs 'COL1'
            if table_def
                .columns
                .iter()
                .any(|c| c.column_name.eq_ignore_ascii_case(column_name))
            {
                log::error!(
                    "❌ ALTER TABLE failed: Column '{}' already exists in {}",
                    column_name,
                    table_id
                );
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' already exists",
                    column_name
                )));
            }
            let kalam_type = data_type.clone();
            let default = default_value
                .as_ref()
                .map(|v| ColumnDefault::literal(serde_json::Value::String(v.clone())))
                .unwrap_or(ColumnDefault::None);
            let ordinal = (table_def.columns.len() + 1) as u32;
            let column_id = table_def.next_column_id;
            table_def.columns.push(ColumnDefinition::new(
                column_id,
                column_name.clone(),
                ordinal,
                kalam_type,
                *nullable,
                false,
                false,
                default,
                None,
            ));
            table_def.next_column_id += 1;
            log::debug!(
                "✓ Added column {} (type: {}, nullable: {})",
                column_name,
                data_type.sql_name(),
                nullable
            );
            Ok((
                format!("ADD COLUMN {} {}", column_name, data_type.sql_name()),
                true,
            ))
        },
        ColumnOperation::Drop { column_name } => {
            // Block dropping system columns
            if is_system_column(column_name) {
                log::error!("❌ ALTER TABLE failed: Cannot drop system column '{}'", column_name);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot drop column '{}': system column cannot be modified",
                    column_name
                )));
            }
            let idx = table_def
                .columns
                .iter()
                .position(|c| c.column_name == *column_name)
                .ok_or_else(|| {
                    log::error!(
                        "❌ ALTER TABLE failed: Column '{}' does not exist in {}",
                        column_name,
                        table_id
                    );
                    KalamDbError::InvalidOperation(format!(
                        "Column '{}' does not exist",
                        column_name
                    ))
                })?;
            table_def.columns.remove(idx);
            for (i, c) in table_def.columns.iter_mut().enumerate() {
                c.ordinal_position = (i + 1) as u32;
            }
            log::debug!("✓ Dropped column {}", column_name);
            Ok((format!("DROP COLUMN {}", column_name), true))
        },
        ColumnOperation::Modify {
            column_name,
            new_data_type,
            nullable,
        } => {
            // Block modifying system columns
            if is_system_column(column_name) {
                log::error!("❌ ALTER TABLE failed: Cannot modify system column '{}'", column_name);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot modify column '{}': system column cannot be altered",
                    column_name
                )));
            }
            let col = table_def
                .columns
                .iter_mut()
                .find(|c| c.column_name == *column_name)
                .ok_or_else(|| {
                    log::error!(
                        "❌ ALTER TABLE failed: Column '{}' does not exist in {}",
                        column_name,
                        table_id
                    );
                    KalamDbError::InvalidOperation(format!(
                        "Column '{}' does not exist",
                        column_name
                    ))
                })?;

            // Track if anything actually changes
            let new_type = new_data_type.clone();
            let type_changed = col.data_type != new_type;
            let nullable_changed = nullable.is_some_and(|n| col.is_nullable != n);
            let changed = type_changed || nullable_changed;

            if type_changed {
                col.data_type = new_type;
            }
            if let Some(n) = nullable {
                col.is_nullable = *n;
            }

            if changed {
                log::debug!(
                    "✓ Modified column {} (new type: {})",
                    column_name,
                    new_data_type.sql_name()
                );
            } else {
                log::debug!(
                    "⊙ No changes to column {} (already type: {})",
                    column_name,
                    new_data_type.sql_name()
                );
            }
            Ok((
                format!(
                    "MODIFY COLUMN {} {}",
                    column_name,
                    new_data_type.sql_name()
                ),
                changed,
            ))
        },
        ColumnOperation::Rename {
            old_column_name,
            new_column_name,
        } => {
            // Block renaming system columns (both renaming FROM or TO a system column name)
            if is_system_column(old_column_name) {
                log::error!(
                    "❌ ALTER TABLE failed: Cannot rename system column '{}'",
                    old_column_name
                );
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot rename column '{}': system column cannot be modified",
                    old_column_name
                )));
            }
            if is_system_column(new_column_name) {
                log::error!("❌ ALTER TABLE failed: Cannot rename column to reserved system column name '{}'", new_column_name);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Cannot rename column to '{}': reserved system column name",
                    new_column_name
                )));
            }
            if !table_def.columns.iter().any(|c| c.column_name == *old_column_name) {
                log::error!(
                    "❌ ALTER TABLE failed: Column '{}' does not exist in {}",
                    old_column_name,
                    table_id
                );
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' does not exist",
                    old_column_name
                )));
            }
            if table_def.columns.iter().any(|c| c.column_name == *new_column_name) {
                log::error!(
                    "❌ ALTER TABLE failed: Column '{}' already exists in {}",
                    new_column_name,
                    table_id
                );
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' already exists",
                    new_column_name
                )));
            }
            if let Some(col) =
                table_def.columns.iter_mut().find(|c| c.column_name == *old_column_name)
            {
                col.column_name = new_column_name.clone();
            }
            log::debug!("✓ Renamed column {} to {}", old_column_name, new_column_name);
            Ok((format!("RENAME COLUMN {} TO {}", old_column_name, new_column_name), true))
        },
        ColumnOperation::SetAccessLevel { access_level } => {
            if table_def.table_type != TableType::Shared {
                log::error!("❌ ALTER TABLE failed: ACCESS LEVEL can only be set on SHARED tables");
                return Err(KalamDbError::InvalidOperation(
                    "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                ));
            }
            let changed = if let kalamdb_commons::schemas::TableOptions::Shared(opts) =
                &mut table_def.table_options
            {
                let current = opts.access_level;
                let new_level = Some(*access_level);
                let changed = current != new_level;
                opts.access_level = new_level;
                changed
            } else {
                false
            };

            if changed {
                log::debug!("✓ Set access level to {:?}", access_level);
            } else {
                log::debug!("⊙ No change to access level (already {:?})", access_level);
            }
            Ok((format!("SET ACCESS LEVEL {:?}", access_level), changed))
        },
    }
}

/// Get a summary string for the operation for logging
fn get_operation_summary(op: &ColumnOperation) -> String {
    match op {
        ColumnOperation::Add {
            column_name,
            data_type,
            ..
        } => format!("ADD COLUMN {} {}", column_name, data_type.sql_name()),
        ColumnOperation::Drop { column_name } => format!("DROP COLUMN {}", column_name),
        ColumnOperation::Modify {
            column_name,
            new_data_type,
            ..
        } => format!(
            "MODIFY COLUMN {} {}",
            column_name,
            new_data_type.sql_name()
        ),
        ColumnOperation::Rename {
            old_column_name,
            new_column_name,
        } => format!("RENAME COLUMN {} TO {}", old_column_name, new_column_name),
        ColumnOperation::SetAccessLevel { access_level } => {
            format!("SET ACCESS LEVEL {:?}", access_level)
        },
    }
}
