//! Typed DDL handler for ALTER TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::schema_registry::arrow_schema::ArrowSchemaWithOptions;
use kalamdb_commons::models::{TableId, NamespaceId};
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
use kalamdb_commons::models::datatypes::KalamDataType;
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

        let registry = self.app_context.schema_registry();
        let table_def_arc = registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!(
                "Table '{}' not found in namespace '{}'",
                statement.table_name.as_str(),
                namespace_id.as_str()
            )))?;

        // Clone for mutation
        let mut table_def: TableDefinition = (*table_def_arc).clone();

        // RBAC (re-check here in case ownership rules added later)
        let is_owner = false; // TODO: track ownership in system.tables
        if !crate::auth::rbac::can_alter_table(context.user_role, table_def.table_type, is_owner) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter this table".to_string(),
            ));
        }

        // Apply operation
        let mut change_desc = String::new();
        match statement.operation {
            ColumnOperation::Add { column_name, data_type, nullable, default_value } => {
                // Prevent duplicates
                if table_def.columns.iter().any(|c| c.column_name == column_name) {
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
                    column_name.clone(), ordinal, kalam_type, nullable, false, false, default, None,
                ));
                change_desc = format!("ADD COLUMN {} {}", column_name, data_type);
            }
            ColumnOperation::Drop { column_name } => {
                let idx = table_def
                    .columns
                    .iter()
                    .position(|c| c.column_name == column_name)
                    .ok_or_else(|| KalamDbError::InvalidOperation(format!(
                        "Column '{}' does not exist",
                        column_name
                    )))?;
                table_def.columns.remove(idx);
                // Reassign ordinal positions sequentially
                for (i, c) in table_def.columns.iter_mut().enumerate() {
                    c.ordinal_position = (i + 1) as u32;
                }
                change_desc = format!("DROP COLUMN {}", column_name);
            }
            ColumnOperation::Modify { column_name, new_data_type, nullable } => {
                let col = table_def
                    .columns
                    .iter_mut()
                    .find(|c| c.column_name == column_name)
                    .ok_or_else(|| KalamDbError::InvalidOperation(format!(
                        "Column '{}' does not exist",
                        column_name
                    )))?;
                col.data_type = map_string_type(&new_data_type)?;
                if let Some(n) = nullable { col.is_nullable = n; }
                change_desc = format!("MODIFY COLUMN {} {}", column_name, new_data_type);
            }
            ColumnOperation::SetAccessLevel { access_level } => {
                if table_def.table_type != TableType::Shared {
                    return Err(KalamDbError::InvalidOperation(
                        "ACCESS LEVEL can only be set on SHARED tables".to_string(),
                    ));
                }
                if let kalamdb_commons::schemas::TableOptions::Shared(opts) = &mut table_def.table_options {
                    opts.access_level = Some(access_level);
                }
                change_desc = format!("SET ACCESS LEVEL {:?}", access_level);
            }
        }

        // Serialize new Arrow schema & bump version
        let arrow_schema = table_def
            .to_arrow_schema()
            .map_err(|e| KalamDbError::SchemaError(format!("Arrow conversion failed: {}", e)))?;
        let schema_json = ArrowSchemaWithOptions::new(arrow_schema).to_json_string().map_err(|e| {
            KalamDbError::SchemaError(format!("Failed to serialize Arrow schema: {}", e))
        })?;
        table_def
            .add_schema_version(change_desc.clone(), schema_json)
            .map_err(KalamDbError::SchemaError)?;

        // Persist (write-through) via registry
        registry.put_table_definition(&table_id, &table_def)?;

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
        _statement: &AlterTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Delegate to RBAC helper (basic owner flag = false for now)
        let is_owner = false;
        let table_type_guess = TableType::User; // Without lookup yet, assume USER for initial auth gate
        if !crate::auth::rbac::can_alter_table(context.user_role, table_type_guess, is_owner) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter table".to_string(),
            ));
        }
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
        other => return Err(KalamDbError::InvalidOperation(format!(
            "Unsupported data type '{}' for ALTER TABLE",
            other
        ))),
    };
    Ok(t)
}
