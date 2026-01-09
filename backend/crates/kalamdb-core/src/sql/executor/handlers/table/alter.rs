//! Typed DDL handler for ALTER TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::helpers::guards::block_system_namespace_modification;
use crate::sql::executor::helpers::table_registration::{
    register_shared_table_provider, register_stream_table_provider, register_user_table_provider,
    unregister_table_provider,
};
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
use kalamdb_commons::models::{NamespaceId, TableId};
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_raft::MetaCommand;
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
    ) -> Result<(TableDefinition, String), KalamDbError> {
        let namespace_id: NamespaceId = statement.namespace_id.clone();
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        log::info!(
            "🔧 ALTER TABLE request: {}.{} (operation: {:?}, user: {}, role: {:?})",
            namespace_id.as_str(),
            statement.table_name.as_str(),
            get_operation_summary(&statement.operation),
            context.user_id.as_str(),
            context.user_role
        );

        // Block ALTER on system tables
        block_system_namespace_modification(
            &namespace_id,
            "ALTER",
            "TABLE",
            Some(statement.table_name.as_str()),
        )?;

        let registry = self.app_context.schema_registry();
        let table_def_arc = registry.get_table_definition(&table_id)?.ok_or_else(|| {
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

        if !crate::auth::rbac::can_alter_table(context.user_role, table_def.table_type, is_owner) {
            log::error!(
                "❌ ALTER TABLE {}.{}: Insufficient privileges",
                namespace_id.as_str(),
                statement.table_name.as_str()
            );
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter table".to_string(),
            ));
        }

        // Apply operation and get change description
        let change_desc = apply_alter_operation(&mut table_def, &statement.operation, &table_id)?;

        // Increment version
        table_def.increment_version();

        log::debug!(
            "✓ Built altered TableDefinition: version={}, columns={}",
            table_def.schema_version,
            table_def.columns.len()
        );

        Ok((table_def, change_desc))
    }

    /// Apply the altered table definition locally (for standalone mode or after Raft apply).
    /// This persists to system.tables, updates cache, and registers providers.
    fn apply_altered_table_locally(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), KalamDbError> {
        use crate::schema_registry::CachedTableData;

        let registry = self.app_context.schema_registry();

        // Persist via registry
        registry.put_table_definition(table_id, table_def)?;

        // Store versioned entry
        let tables_provider = self.app_context.system_tables().tables();
        tables_provider.put_versioned_schema(table_id, table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to persist table version: {}", e))
        })?;

        // Update main tables store
        tables_provider.update_table(table_id, table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to update table definition: {}", e))
        })?;

        // Prime cache with updated definition
        {
            let old_entry = registry.get(table_id);
            let new_data = CachedTableData::from_altered_table(
                table_id,
                Arc::new(table_def.clone()),
                old_entry.as_deref(),
            )?;

            log::debug!(
                "✓ Updated cache for {}: storage_id={:?}, template={}",
                table_id,
                new_data.storage_id.as_ref().map(|s| s.as_str()),
                new_data.storage_path_template
            );

            registry.insert(table_id.clone(), Arc::new(new_data));
        }

        // Get arrow schema and re-register provider
        let arrow_schema = registry.get_arrow_schema(table_id)?;
        unregister_table_provider(&self.app_context, table_id)?;

        match table_def.table_type {
            TableType::User => {
                register_user_table_provider(&self.app_context, table_id, arrow_schema)?;
            }
            TableType::Shared => {
                register_shared_table_provider(&self.app_context, table_id, arrow_schema)?;
            }
            TableType::Stream => {
                let ttl_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(opts) =
                    &table_def.table_options
                {
                    Some(opts.ttl_seconds)
                } else {
                    None
                };
                register_stream_table_provider(&self.app_context, table_id, arrow_schema, ttl_seconds)?;
            }
            TableType::System => {
                // System tables are not altered this way
            }
        }

        Ok(())
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
        let namespace_id: NamespaceId = statement.namespace_id.clone();
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        // Build the altered table definition (validate + apply mutation)
        let (table_def, change_desc) = self.build_altered_table_definition(&statement, context)?;

        // Unified execution path:
        // - Cluster mode: Raft-first (propose to Raft, applier executes on ALL nodes)
        // - Standalone mode: Execute locally
        if self.app_context.executor().is_cluster_mode() {
            // Serialize and propose to Raft - applier will execute on ALL nodes
            let schema_json = serde_json::to_string(&table_def).map_err(|e| {
                KalamDbError::Other(format!("Failed to serialize table definition: {}", e))
            })?;
            let cmd = MetaCommand::AlterTable {
                table_id: table_id.clone(),
                schema_json,
            };
            self.app_context
                .executor()
                .execute_meta(cmd)
                .await
                .map_err(|e| {
                    KalamDbError::ExecutionError(format!(
                        "Failed to replicate table metadata via executor: {}",
                        e
                    ))
                })?;
        } else {
            // Standalone mode: Apply locally
            self.apply_altered_table_locally(&table_id, &table_def)?;
        }

        // Log DDL operation
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_ddl_operation(
            context,
            "ALTER",
            "TABLE",
            &format!("{}.{}", namespace_id.as_str(), statement.table_name.as_str()),
            Some(format!("Operation: {}, New Version: {}", change_desc, table_def.schema_version)),
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
        if let Ok(Some(def)) = registry.get_table_definition(&table_id) {
            let is_owner = matches!(def.table_type, TableType::User);

            if !crate::auth::rbac::can_alter_table(context.user_role, def.table_type, is_owner) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to alter table".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Apply an ALTER TABLE operation to a table definition
fn apply_alter_operation(
    table_def: &mut TableDefinition,
    operation: &ColumnOperation,
    table_id: &TableId,
) -> Result<String, KalamDbError> {
    match operation {
        ColumnOperation::Add {
            column_name,
            data_type,
            nullable,
            default_value,
        } => {
            if table_def.columns.iter().any(|c| c.column_name == *column_name) {
                log::error!("❌ ALTER TABLE failed: Column '{}' already exists in {}", column_name, table_id);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' already exists",
                    column_name
                )));
            }
            let kalam_type = map_string_type(data_type)?;
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
            log::debug!("✓ Added column {} (type: {}, nullable: {})", column_name, data_type, nullable);
            Ok(format!("ADD COLUMN {} {}", column_name, data_type))
        }
        ColumnOperation::Drop { column_name } => {
            let idx = table_def
                .columns
                .iter()
                .position(|c| c.column_name == *column_name)
                .ok_or_else(|| {
                    log::error!("❌ ALTER TABLE failed: Column '{}' does not exist in {}", column_name, table_id);
                    KalamDbError::InvalidOperation(format!("Column '{}' does not exist", column_name))
                })?;
            table_def.columns.remove(idx);
            for (i, c) in table_def.columns.iter_mut().enumerate() {
                c.ordinal_position = (i + 1) as u32;
            }
            log::debug!("✓ Dropped column {}", column_name);
            Ok(format!("DROP COLUMN {}", column_name))
        }
        ColumnOperation::Modify {
            column_name,
            new_data_type,
            nullable,
        } => {
            let col = table_def
                .columns
                .iter_mut()
                .find(|c| c.column_name == *column_name)
                .ok_or_else(|| {
                    log::error!("❌ ALTER TABLE failed: Column '{}' does not exist in {}", column_name, table_id);
                    KalamDbError::InvalidOperation(format!("Column '{}' does not exist", column_name))
                })?;
            col.data_type = map_string_type(new_data_type)?;
            if let Some(n) = nullable {
                col.is_nullable = *n;
            }
            log::debug!("✓ Modified column {} (new type: {})", column_name, new_data_type);
            Ok(format!("MODIFY COLUMN {} {}", column_name, new_data_type))
        }
        ColumnOperation::Rename {
            old_column_name,
            new_column_name,
        } => {
            if !table_def.columns.iter().any(|c| c.column_name == *old_column_name) {
                log::error!("❌ ALTER TABLE failed: Column '{}' does not exist in {}", old_column_name, table_id);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' does not exist",
                    old_column_name
                )));
            }
            if table_def.columns.iter().any(|c| c.column_name == *new_column_name) {
                log::error!("❌ ALTER TABLE failed: Column '{}' already exists in {}", new_column_name, table_id);
                return Err(KalamDbError::InvalidOperation(format!(
                    "Column '{}' already exists",
                    new_column_name
                )));
            }
            if let Some(col) = table_def.columns.iter_mut().find(|c| c.column_name == *old_column_name) {
                col.column_name = new_column_name.clone();
            }
            log::debug!("✓ Renamed column {} to {}", old_column_name, new_column_name);
            Ok(format!("RENAME COLUMN {} TO {}", old_column_name, new_column_name))
        }
        ColumnOperation::SetAccessLevel { access_level } => {
            if table_def.table_type != TableType::Shared {
                log::error!("❌ ALTER TABLE failed: ACCESS LEVEL can only be set on SHARED tables");
                return Err(KalamDbError::InvalidOperation(
                    "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                ));
            }
            if let kalamdb_commons::schemas::TableOptions::Shared(opts) = &mut table_def.table_options {
                opts.access_level = Some(*access_level);
            }
            log::debug!("✓ Set access level to {:?}", access_level);
            Ok(format!("SET ACCESS LEVEL {:?}", access_level))
        }
    }
}

/// Map simplified SQL type strings to KalamDataType
fn map_string_type(dt: &str) -> Result<KalamDataType, KalamDbError> {
    let upper = dt.to_uppercase();
    use KalamDataType as K;
    let t = match upper.as_str() {
        "INT" | "INT32" => K::Int,
        "BIGINT" | "INT64" => K::BigInt,
        "TEXT" | "STRING" | "UTF8" => K::Text,
        "BOOL" | "BOOLEAN" => K::Boolean,
        "TIMESTAMP" => K::Timestamp,
        "UUID" => K::Uuid,
        "FLOAT" | "FLOAT32" => K::Float,
        "DOUBLE" | "FLOAT64" => K::Double,
        other => {
            return Err(KalamDbError::InvalidOperation(format!(
                "Unsupported data type '{}' for ALTER TABLE",
                other
            )))
        }
    };
    Ok(t)
}

/// Get a summary string for the operation for logging
fn get_operation_summary(op: &ColumnOperation) -> String {
    match op {
        ColumnOperation::Add { column_name, data_type, .. } => format!("ADD COLUMN {} {}", column_name, data_type),
        ColumnOperation::Drop { column_name } => format!("DROP COLUMN {}", column_name),
        ColumnOperation::Modify { column_name, new_data_type, .. } => format!("MODIFY COLUMN {} {}", column_name, new_data_type),
        ColumnOperation::Rename { old_column_name, new_column_name } => format!("RENAME COLUMN {} TO {}", old_column_name, new_column_name),
        ColumnOperation::SetAccessLevel { access_level } => format!("SET ACCESS LEVEL {:?}", access_level),
    }
}
