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
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterTableStatement> for AlterTableHandler {
    async fn execute(
        &self,
        statement: AlterTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Resolve namespace (statement always has namespace_id already parsed)
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

        // Block ALTER on system tables - they are managed internally
        // Future versions may use ALTER for schema migrations during upgrades
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

        // Clone for mutation
        let mut table_def: TableDefinition = (*table_def_arc).clone();

        log::debug!(
            "📋 Current table schema: type={:?}, columns={}, version={}",
            table_def.table_type,
            table_def.columns.len(),
            table_def.schema_version
        );

        // RBAC: For USER tables, if the user can access it, they're the owner
        // For other table types, ownership is not applicable (only Dba/System can alter)
        let is_owner = matches!(table_def.table_type, TableType::User);
        
        log::debug!(
            "🔐 RBAC check: user={}, role={:?}, table_type={:?}, is_owner={}",
            context.user_id.as_str(),
            context.user_role,
            table_def.table_type,
            is_owner
        );
        
        if !crate::auth::rbac::can_alter_table(context.user_role, table_def.table_type, is_owner) {
            log::error!(
                "❌ ALTER TABLE {}.{}: Insufficient privileges (user: {}, role: {:?}, table_type: {:?}, is_owner: {})",
                namespace_id.as_str(),
                statement.table_name.as_str(),
                context.user_id.as_str(),
                context.user_role,
                table_def.table_type,
                is_owner
            );
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter table".to_string(),
            ));
        }

        // Apply operation
        // Defer building change description until operation applied
        #[allow(unused_assignments)]
        let mut change_desc_opt: Option<String> = None;
        match statement.operation {
            ColumnOperation::Add {
                column_name,
                data_type,
                nullable,
                default_value,
            } => {
                // Prevent duplicates
                if table_def
                    .columns
                    .iter()
                    .any(|c| c.column_name == column_name)
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
                let kalam_type = map_string_type(&data_type)?;
                let default = default_value
                    .map(|v| ColumnDefault::literal(serde_json::Value::String(v)))
                    .unwrap_or(ColumnDefault::None);
                let ordinal = (table_def.columns.len() + 1) as u32;
                let column_id = table_def.next_column_id;
                table_def.columns.push(ColumnDefinition::new(
                    column_id,
                    column_name.clone(),
                    ordinal,
                    kalam_type,
                    nullable,
                    false,
                    false,
                    default,
                    None,
                ));
                table_def.next_column_id += 1;
                change_desc_opt = Some(format!("ADD COLUMN {} {}", column_name, data_type));
                log::debug!(
                    "✓ Added column {} (type: {}, nullable: {})",
                    column_name,
                    data_type,
                    nullable
                );
            }
            ColumnOperation::Drop { column_name } => {
                let idx = table_def
                    .columns
                    .iter()
                    .position(|c| c.column_name == column_name)
                    .ok_or_else(|| {
                        log::error!(
                            "❌ ALTER TABLE failed: Column '{}' does not exist in {}.{}",
                            column_name,
                            namespace_id.as_str(),
                            statement.table_name.as_str()
                        );
                        KalamDbError::InvalidOperation(format!(
                            "Column '{}' does not exist",
                            column_name
                        ))
                    })?;
                table_def.columns.remove(idx);
                // Reassign ordinal positions sequentially
                for (i, c) in table_def.columns.iter_mut().enumerate() {
                    c.ordinal_position = (i + 1) as u32;
                }
                change_desc_opt = Some(format!("DROP COLUMN {}", column_name));
                log::debug!("✓ Dropped column {}", column_name);
            }
            ColumnOperation::Modify {
                column_name,
                new_data_type,
                nullable,
            } => {
                let col = table_def
                    .columns
                    .iter_mut()
                    .find(|c| c.column_name == column_name)
                    .ok_or_else(|| {
                        log::error!(
                            "❌ ALTER TABLE failed: Column '{}' does not exist in {}.{}",
                            column_name,
                            namespace_id.as_str(),
                            statement.table_name.as_str()
                        );
                        KalamDbError::InvalidOperation(format!(
                            "Column '{}' does not exist",
                            column_name
                        ))
                    })?;
                col.data_type = map_string_type(&new_data_type)?;
                if let Some(n) = nullable {
                    col.is_nullable = n;
                }
                change_desc_opt = Some(format!("MODIFY COLUMN {} {}", column_name, new_data_type));
                log::debug!(
                    "✓ Modified column {} (new type: {})",
                    column_name,
                    new_data_type
                );
            }
            ColumnOperation::Rename {
                old_column_name,
                new_column_name,
            } => {
                // Check that old column exists
                if !table_def
                    .columns
                    .iter()
                    .any(|c| c.column_name == old_column_name)
                {
                    log::error!(
                        "❌ ALTER TABLE failed: Column '{}' does not exist in {}.{}",
                        old_column_name,
                        namespace_id.as_str(),
                        statement.table_name.as_str()
                    );
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Column '{}' does not exist",
                        old_column_name
                    )));
                }
                
                // Check that new column name doesn't already exist
                if table_def
                    .columns
                    .iter()
                    .any(|c| c.column_name == new_column_name)
                {
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
                
                // Rename the column (metadata only, no data migration needed)
                if let Some(col) = table_def
                    .columns
                    .iter_mut()
                    .find(|c| c.column_name == old_column_name)
                {
                    col.column_name = new_column_name.clone();
                }
                
                change_desc_opt = Some(format!(
                    "RENAME COLUMN {} TO {}",
                    old_column_name, new_column_name
                ));
                log::debug!(
                    "✓ Renamed column {} to {}",
                    old_column_name,
                    new_column_name
                );
            }
            ColumnOperation::SetAccessLevel { access_level } => {
                if table_def.table_type != TableType::Shared {
                    log::error!(
                        "❌ ALTER TABLE failed: ACCESS LEVEL can only be set on SHARED tables (table {}.{} is {:?})",
                        namespace_id.as_str(),
                        statement.table_name.as_str(),
                        table_def.table_type
                    );
                    return Err(KalamDbError::InvalidOperation(
                        "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                    ));
                }
                if let kalamdb_commons::schemas::TableOptions::Shared(opts) =
                    &mut table_def.table_options
                {
                    opts.access_level = Some(access_level);
                }
                change_desc_opt = Some(format!("SET ACCESS LEVEL {:?}", access_level));
                log::debug!("✓ Set access level to {:?}", access_level);
            }
        }

        let change_desc =
            change_desc_opt.expect("ALTER TABLE operation must set change description");
        
        // Phase 16: Increment version (schema history is now stored externally)
        table_def.increment_version();

        // Persist (write-through) via registry - stores latest in cache
        registry.put_table_definition(&table_id, &table_def)?;

        // Store versioned entry in versioned tables store (for schema history)
        let tables_provider = self.app_context.system_tables().tables();
        tables_provider.put_versioned_schema(&table_id, &table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to persist table version: {}", e))
        })?;

        // Also update the main tables store (for latest table definition lookup)
        tables_provider.update_table(&table_id, &table_def).map_err(|e| {
            KalamDbError::Other(format!("Failed to update table definition: {}", e))
        })?;

        // Prime cache with updated definition, preserving storage config from old entry
        {
            use crate::schema_registry::CachedTableData;
            
            // Retrieve old cache entry to preserve storage_id and storage_path_template
            let old_entry = registry.get(&table_id);
            
            // Use the new from_altered_table factory method
            let new_data = CachedTableData::from_altered_table(
                &table_id,
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

        // Get arrow schema from cache (memoized in CachedTableData) instead of to_arrow_schema()
        let arrow_schema = registry.get_arrow_schema(&table_id)?;

        // Unregister old provider first to ensure DataFusion catalog is updated
        unregister_table_provider(&self.app_context, &table_id)?;

        // Re-register provider to update schema in DataFusion (using cached arrow schema)
        match table_def.table_type {
            TableType::User => {
                register_user_table_provider(&self.app_context, &table_id, arrow_schema)?;
            }
            TableType::Shared => {
                register_shared_table_provider(&self.app_context, &table_id, arrow_schema)?;
            }
            TableType::Stream => {
                let ttl_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(opts) =
                    &table_def.table_options
                {
                    Some(opts.ttl_seconds)
                } else {
                    None
                };
                register_stream_table_provider(
                    &self.app_context,
                    &table_id,
                    arrow_schema,
                    ttl_seconds,
                )?;
            }
            TableType::System => {
                // System tables are not altered this way usually
            }
        }

        if self.app_context.executor().is_cluster_mode() {
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
        }

        // Log DDL operation
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_ddl_operation(
            context,
            "ALTER",
            "TABLE",
            &format!(
                "{}.{}",
                namespace_id.as_str(),
                statement.table_name.as_str()
            ),
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
    }

    async fn check_authorization(
        &self,
        statement: &AlterTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;
        
        // T050: Block anonymous users from DDL operations
        block_anonymous_write(context, "ALTER TABLE")?;
        
        // Resolve namespace
        let namespace_id = &statement.namespace_id;
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        // Lookup table to get type for accurate permission check
        let registry = self.app_context.schema_registry();
        if let Ok(Some(def)) = registry.get_table_definition(&table_id) {
            // For USER tables, if the user can access it, they're the owner
            // For other table types, ownership is not applicable (only Dba/System can alter)
            let is_owner = matches!(def.table_type, TableType::User);
            
            if !crate::auth::rbac::can_alter_table(context.user_role, def.table_type, is_owner) {
                return Err(KalamDbError::Unauthorized(
                    "Insufficient privileges to alter table".to_string(),
                ));
            }
        }
        // If table not found, let execute() handle the NotFound error
        Ok(())
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
        ColumnOperation::Add {
            column_name,
            data_type,
            ..
        } => format!("ADD COLUMN {} {}", column_name, data_type),
        ColumnOperation::Drop { column_name } => format!("DROP COLUMN {}", column_name),
        ColumnOperation::Modify {
            column_name,
            new_data_type,
            ..
        } => format!("MODIFY COLUMN {} {}", column_name, new_data_type),
        ColumnOperation::Rename {
            old_column_name,
            new_column_name,
        } => format!("RENAME COLUMN {} TO {}", old_column_name, new_column_name),
        ColumnOperation::SetAccessLevel { access_level } => {
            format!("SET ACCESS LEVEL {:?}", access_level)
        }
    }
}
