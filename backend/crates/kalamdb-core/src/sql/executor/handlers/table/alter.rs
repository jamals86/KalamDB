//! Typed DDL handler for ALTER TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::helpers::table_registration::{
    register_shared_table_provider, register_stream_table_provider, register_user_table_provider,
    unregister_table_provider,
};
use kalamdb_commons::models::datatypes::KalamDataType;
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

        // RBAC (re-check here in case ownership rules added later)
        let is_owner = false; // TODO: track ownership in system.tables
        if !crate::auth::rbac::can_alter_table(context.user_role, table_def.table_type, is_owner) {
            log::error!(
                "❌ ALTER TABLE {}.{}: Insufficient privileges (user: {}, role: {:?}, table_type: {:?})",
                namespace_id.as_str(),
                statement.table_name.as_str(),
                context.user_id.as_str(),
                context.user_role,
                table_def.table_type
            );
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter this table".to_string(),
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
                        "❌ ALTER TABLE failed: Column '{}' already exists in {}.{}",
                        column_name,
                        namespace_id.as_str(),
                        statement.table_name.as_str()
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
                table_def.columns.push(ColumnDefinition::new(
                    column_name.clone(),
                    ordinal,
                    kalam_type,
                    nullable,
                    false,
                    false,
                    default,
                    None,
                ));
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
                    opts.access_level = Some(access_level.clone());
                }
                change_desc_opt = Some(format!("SET ACCESS LEVEL {:?}", access_level));
                log::debug!("✓ Set access level to {:?}", access_level);
            }
        }

        // Serialize new Arrow schema & bump version
        let arrow_schema = table_def
            .to_arrow_schema()
            .map_err(|e| KalamDbError::SchemaError(format!("Arrow conversion failed: {}", e)))?;
        let schema_json = ArrowSchemaWithOptions::new(arrow_schema.clone())
            .to_json_string()
            .map_err(|e| {
                KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e))
            })?;
        let change_desc =
            change_desc_opt.expect("ALTER TABLE operation must set change description");
        table_def
            .add_schema_version(change_desc.clone(), schema_json)
            .map_err(KalamDbError::SchemaError)?;

        // Persist (write-through) via registry
        registry.put_table_definition(&table_id, &table_def)?;

        // Prime cache with updated definition so existing providers retain memoized access
        {
            use crate::schema_registry::CachedTableData;
            registry.insert(
                table_id.clone(),
                Arc::new(CachedTableData::new(Arc::new(table_def.clone()))),
            );
        }

        // Unregister old provider first to ensure DataFusion catalog is updated
        unregister_table_provider(&self.app_context, &table_id)?;

        // Re-register provider to update schema in DataFusion
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
        // Resolve namespace
        let namespace_id = &statement.namespace_id;
        let table_id = TableId::from_strings(namespace_id.as_str(), statement.table_name.as_str());

        // Lookup table to get type for accurate permission check
        let registry = self.app_context.schema_registry();
        if let Ok(Some(def)) = registry.get_table_definition(&table_id) {
            let is_owner = false; // TODO: track ownership
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
        ColumnOperation::SetAccessLevel { access_level } => {
            format!("SET ACCESS LEVEL {:?}", access_level)
        }
    }
}
