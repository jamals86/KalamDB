//! INSERT Handler
//!
//! Handles INSERT statements with native write paths and parameter binding.
//!
//! Flow:
//! 1. Parse INSERT SQL to extract table name, columns, values
//! 2. Bind ScalarValue parameters to $1, $2, etc. placeholders
//! 3. Validate parameter count and types
//! 4. Execute via UserTableInsertHandler native write path
//! 5. Return ExecutionResult::Inserted { rows_affected }

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::providers::base::BaseTableProvider; // bring trait into scope for insert_batch
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use serde_json::Value as JsonValue;
use sqlparser::ast::{Expr, Statement, Values as SqlValues};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Handler for INSERT statements
///
/// Implements native write paths for INSERT operations with parameter binding.
/// Does NOT use DataFusion for writes - uses UserTableInsertHandler directly.
pub struct InsertHandler {
    app_context: std::sync::Arc<AppContext>,
}

impl InsertHandler {
    pub fn new() -> Self {
        Self {
            app_context: AppContext::get(),
        }
    }
}

impl Default for InsertHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StatementHandler for InsertHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // T062: Validate parameters before write using config from AppContext
        let limits = ParameterLimits::from_config(&self.app_context.config().execution);
        validate_parameters(&params, &limits)?;

        // Ensure correct variant
        if !matches!(statement.kind(), SqlStatementKind::Insert(_)) {
            return Err(KalamDbError::InvalidOperation(
                "InsertHandler received wrong statement kind".into(),
            ));
        }

        let sql = statement.as_str();

        // Parse INSERT using sqlparser-rs (handles multi-line, comments, complex expressions)
        let (namespace, table_name, columns, rows_data) = self.parse_insert_with_sqlparser(sql)?;

        // Validate table exists via SchemaRegistry fast path (using TableId)
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let schema_registry = AppContext::get().schema_registry();
        let exists = schema_registry.table_exists(&table_id)?;
        if !exists {
            return Err(KalamDbError::InvalidOperation(format!(
                "Table '{}.{}' does not exist",
                namespace.as_str(),
                table_name.as_str()
            )));
        }

        // Get table definition to access column_defaults
        let table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Table definition not found: {}.{}",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;

        // T163: Reject AS USER on Shared tables (Phase 7)
        use kalamdb_commons::schemas::TableType;
        if statement.as_user_id().is_some() && matches!(table_def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string(),
            ));
        }

        // Determine effective user for AS USER before evaluating defaults
        let effective_user_id = statement.as_user_id().unwrap_or(&context.user_id);

        // Bind parameters and construct JSON rows
        let mut json_rows: Vec<JsonValue> = Vec::new();
        for row_exprs in rows_data {
            let mut obj = serde_json::Map::new();
            if columns.is_empty() {
                // Positional mapping: column order from schema will be used later; here just store as v1,v2...
                // For simplicity require explicit column list for MVP
                return Err(KalamDbError::InvalidOperation(
                    "INSERT without explicit column list not supported yet".into(),
                ));
            }
            if row_exprs.len() != columns.len() {
                return Err(KalamDbError::InvalidOperation(format!(
                    "VALUES column count mismatch: expected {} got {}",
                    columns.len(),
                    row_exprs.len()
                )));
            }
            for (col, expr) in columns.iter().zip(row_exprs.iter()) {
                let value = self.expr_to_json(expr, &params)?;
                obj.insert(col.clone(), value);
            }

            // Apply DEFAULT values for missing columns
            use crate::sql::executor::default_evaluator::evaluate_default;
            let sys_cols = self.app_context.system_columns_service();

            for col_def in &table_def.columns {
                let col_name = &col_def.column_name;

                // Skip if column was explicitly provided in INSERT
                if obj.contains_key(col_name) {
                    continue;
                }

                // Skip if this is a None default (column is optional)
                if col_def.default_value.is_none() {
                    continue;
                }

                // Evaluate the default value using the effective user (AS USER subject)
                let default_value = evaluate_default(
                    &col_def.default_value,
                    effective_user_id,
                    Some(sys_cols.clone()),
                )?;
                obj.insert(col_name.clone(), default_value);
            }

            json_rows.push(JsonValue::Object(obj));
        }

        // T153: Execute native insert with impersonation support (Phase 7)
        // Use as_user_id if present, otherwise use context.user_id
        let rows_affected = self
            .execute_native_insert(&namespace, &table_name, effective_user_id, json_rows)
            .await?;
        Ok(ExecutionResult::Inserted { rows_affected })
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !matches!(statement.kind(), SqlStatementKind::Insert(_)) {
            return Err(KalamDbError::InvalidOperation(
                "InsertHandler received wrong statement kind".into(),
            ));
        }

        // T152: Validate AS USER authorization - only Service/Dba/System can use AS USER (Phase 7)
        if statement.as_user_id().is_some() {
            use kalamdb_commons::Role;
            if !matches!(context.user_role, Role::Service | Role::Dba | Role::System) {
                return Err(KalamDbError::Unauthorized(
                    format!("Role {:?} is not authorized to use AS USER. Only Service, Dba, and System roles are permitted.", context.user_role())
                ));
            }
        }

        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
        }
    }
}

impl InsertHandler {
    /// Parse INSERT statement using sqlparser-rs
    /// Returns (namespace, table_name, columns, rows_of_exprs)
    fn parse_insert_with_sqlparser(
        &self,
        sql: &str,
    ) -> Result<(NamespaceId, TableName, Vec<String>, Vec<Vec<Expr>>), KalamDbError> {
        let dialect = GenericDialect {};
        let stmts = Parser::parse_sql(&dialect, sql)
            .map_err(|e| KalamDbError::InvalidOperation(format!("SQL parse error: {}", e)))?;

        if stmts.is_empty() {
            return Err(KalamDbError::InvalidOperation(
                "No SQL statement found".into(),
            ));
        }

        let Statement::Insert(insert) = &stmts[0] else {
            return Err(KalamDbError::InvalidOperation(
                "Expected INSERT statement".into(),
            ));
        };

        // Extract table name from TableObject
        let table_parts: Vec<String> = match &insert.table {
            sqlparser::ast::TableObject::TableName(obj_name) => obj_name
                .0
                .iter()
                .filter_map(|part| match part {
                    sqlparser::ast::ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
                    _ => None,
                })
                .collect(),
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    "Unsupported table reference type".into(),
                ))
            }
        };

        let (namespace, table_name) = match table_parts.len() {
            1 => (
                NamespaceId::new("default"),
                TableName::new(table_parts[0].clone()),
            ),
            2 => (
                NamespaceId::new(table_parts[0].clone()),
                TableName::new(table_parts[1].clone()),
            ),
            _ => return Err(KalamDbError::InvalidOperation("Invalid table name".into())),
        };

        // Extract columns
        let columns: Vec<String> = insert
            .columns
            .iter()
            .map(|ident| ident.value.clone())
            .collect();

        // Extract VALUES rows
        let Some(ref source) = insert.source else {
            return Err(KalamDbError::InvalidOperation(
                "INSERT missing VALUES clause".into(),
            ));
        };

        let rows = match source.body.as_ref() {
            sqlparser::ast::SetExpr::Values(SqlValues { rows, .. }) => {
                rows.iter().map(|row| row.clone()).collect()
            }
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    "Only VALUES clause supported for INSERT".into(),
                ))
            }
        };

        Ok((namespace, table_name, columns, rows))
    }

    /// Convert sqlparser Expr to JSON value, binding parameters as needed
    fn expr_to_json(&self, expr: &Expr, params: &[ScalarValue]) -> Result<JsonValue, KalamDbError> {
        match expr {
            Expr::Value(val_with_span) => {
                match &val_with_span.value {
                    sqlparser::ast::Value::Number(n, _) => {
                        // Try as i64 first, then f64
                        if let Ok(i) = n.parse::<i64>() {
                            Ok(JsonValue::Number(i.into()))
                        } else if let Ok(f) = n.parse::<f64>() {
                            serde_json::Number::from_f64(f)
                                .map(JsonValue::Number)
                                .ok_or_else(|| {
                                    KalamDbError::InvalidOperation("Invalid float value".into())
                                })
                        } else {
                            Err(KalamDbError::InvalidOperation(format!(
                                "Invalid number: {}",
                                n
                            )))
                        }
                    }
                    sqlparser::ast::Value::SingleQuotedString(s)
                    | sqlparser::ast::Value::DoubleQuotedString(s) => {
                        Ok(JsonValue::String(s.clone()))
                    }
                    sqlparser::ast::Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
                    sqlparser::ast::Value::Null => Ok(JsonValue::Null),
                    _ => Ok(JsonValue::String(val_with_span.to_string())),
                }
            }
            Expr::Identifier(ident) if ident.value.starts_with('$') => {
                // Parameter placeholder: $1, $2, etc.
                let param_num: usize = ident.value[1..].parse().map_err(|_| {
                    KalamDbError::InvalidOperation(format!("Invalid placeholder: {}", ident.value))
                })?;

                if param_num == 0 || param_num > params.len() {
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Parameter ${} out of range (have {} parameters)",
                        param_num,
                        params.len()
                    )));
                }

                self.scalar_value_to_json(&params[param_num - 1])
            }
            _ => {
                // For other expressions, convert to string (fallback)
                Ok(JsonValue::String(expr.to_string()))
            }
        }
    }

    /// Convert DataFusion ScalarValue to JSON
    fn scalar_value_to_json(&self, value: &ScalarValue) -> Result<JsonValue, KalamDbError> {
        match value {
            ScalarValue::Null => Ok(JsonValue::Null),
            ScalarValue::Boolean(Some(b)) => Ok(JsonValue::Bool(*b)),
            ScalarValue::Int8(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::Int16(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::Int32(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::Int64(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::UInt8(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::UInt16(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::UInt32(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::UInt64(Some(i)) => Ok(JsonValue::Number((*i).into())),
            ScalarValue::Float32(Some(f)) => serde_json::Number::from_f64(*f as f64)
                .map(JsonValue::Number)
                .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into())),
            ScalarValue::Float64(Some(f)) => serde_json::Number::from_f64(*f)
                .map(JsonValue::Number)
                .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into())),
            ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => {
                Ok(JsonValue::String(s.clone()))
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported parameter type: {:?}",
                value
            ))),
        }
    }

    /// Execute native INSERT using UserTableInsertHandler
    async fn execute_native_insert(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
        user_id: &kalamdb_commons::models::UserId,
        rows: Vec<JsonValue>,
    ) -> Result<usize, KalamDbError> {
        // Get schema registry to access table providers
        let schema_registry = self.app_context.schema_registry();

        // Construct TableId from namespace and table_name
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());

        // Get table definition (validates table exists) and determine table type
        let table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Table not found: {}.{}",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;
        let table_type = table_def.table_type;

        match table_type {
            TableType::User => {
                // Get UserTableProvider (new providers module) and downcast
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "User table provider not found for: {}.{}",
                        namespace.as_str(),
                        table_name.as_str()
                    ))
                })?;

                if let Some(provider) = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::UserTableProvider>()
                {
                    let row_ids = provider.insert_batch(&user_id, rows)?;
                    Ok(row_ids.len())
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for user table".into(),
                    ))
                }
            }
            TableType::Shared => {
                // Downcast to new providers::SharedTableProvider and batch insert
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Shared table provider not found for: {}.{}",
                        namespace.as_str(),
                        table_name.as_str()
                    ))
                })?;

                if let Some(shared_provider) = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::SharedTableProvider>(
                ) {
                    let row_ids = shared_provider.insert_batch(&user_id, rows)?;
                    Ok(row_ids.len())
                } else {
                    Err(KalamDbError::InvalidOperation(format!(
                        "Cached provider type mismatch for shared table {}.{}",
                        namespace.as_str(),
                        table_name.as_str()
                    )))
                }
            }
            TableType::Stream => {
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Stream table provider not found for: {}.{}",
                        namespace.as_str(),
                        table_name.as_str()
                    ))
                })?;

                // New providers::StreamTableProvider supports batch insert with user_id
                if let Some(stream_provider) = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::StreamTableProvider>(
                ) {
                    let row_ids = stream_provider.insert_batch(&user_id, rows)?;
                    Ok(row_ids.len())
                } else {
                    Err(KalamDbError::InvalidOperation(format!(
                        "Cached provider type mismatch for stream table {}.{}",
                        namespace.as_str(),
                        table_name.as_str()
                    )))
                }
            }
            TableType::System => Err(KalamDbError::InvalidOperation(
                "Cannot INSERT into SYSTEM tables".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_session;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), role, create_test_session())
    }

    #[tokio::test]
    async fn test_insert_authorization_user() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::User);
        let stmt = SqlStatement::new(
            "INSERT INTO default.test (id) VALUES (1)".to_string(),
            SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_dba() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new(
            "INSERT INTO default.test (id) VALUES (1)".to_string(),
            SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_service() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new(
            "INSERT INTO default.test (id) VALUES (1)".to_string(),
            SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }
}
