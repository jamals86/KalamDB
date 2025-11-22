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
use crate::providers::arrow_json_conversion::scalar_value_to_json;
use crate::providers::base::BaseTableProvider; // bring trait into scope for insert_batch
use crate::sql::executor::default_evaluator::evaluate_default;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::models::{NamespaceId, Row, TableName};
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::schemas::{ColumnDefault, TableType};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use serde_json::Value as JsonValue;
use sqlparser::ast::{Expr, Statement, Values as SqlValues};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::{BTreeMap, HashSet};

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

        if columns.is_empty() {
            // Positional mapping: column order from schema will be used later; here just store as v1,v2...
            // For simplicity require explicit column list for MVP
            return Err(KalamDbError::InvalidOperation(
                "INSERT without explicit column list not supported yet".into(),
            ));
        }

        let provided_columns: HashSet<&str> = columns.iter().map(|c| c.as_str()).collect();
        let default_columns: Vec<&kalamdb_commons::schemas::ColumnDefinition> = table_def
            .columns
            .iter()
            .filter(|col_def| {
                !provided_columns.contains(col_def.column_name.as_str())
                    && !col_def.default_value.is_none()
            })
            .collect();
        let sys_cols = self.app_context.system_columns_service();

        // Create a map of column name to type for fast lookup
        let col_types: std::collections::HashMap<
            String,
            kalamdb_commons::models::datatypes::KalamDataType,
        > = table_def
            .columns
            .iter()
            .map(|c| (c.column_name.clone(), c.data_type.clone()))
            .collect();

        // Bind parameters and construct Row values directly (ScalarValue map)
        let mut rows: Vec<Row> = Vec::new();
        for row_exprs in rows_data {
            let mut values = BTreeMap::new();
            if row_exprs.len() != columns.len() {
                return Err(KalamDbError::InvalidOperation(format!(
                    "VALUES column count mismatch: expected {} got {}",
                    columns.len(),
                    row_exprs.len()
                )));
            }
            for (col, expr) in columns.iter().zip(row_exprs.iter()) {
                let target_type = col_types.get(col);
                let value =
                    self.expr_to_scalar_value(expr, &params, effective_user_id, target_type)?;
                values.insert(col.clone(), value);
            }

            // Apply DEFAULT values for missing columns
            for col_def in &default_columns {
                // Evaluate the default value using the effective user (AS USER subject)
                let default_value = evaluate_default(
                    &col_def.default_value,
                    effective_user_id,
                    Some(sys_cols.clone()),
                )?;
                values.insert(col_def.column_name.clone(), default_value);
            }

            rows.push(Row::new(values));
        }

        // T153: Execute native insert with impersonation support (Phase 7)
        // Use as_user_id if present, otherwise use context.user_id
        let rows_affected = self
            .execute_native_insert(
                &namespace,
                &table_name,
                effective_user_id,
                context.user_role,
                rows,
            )
            .await?;

        // Log DML operation
        use crate::sql::executor::helpers::audit;
        let subject_user_id = statement.as_user_id().cloned();
        let audit_entry = audit::log_dml_operation(
            context,
            "INSERT",
            &format!("{}.{}", namespace.as_str(), table_name.as_str()),
            rows_affected,
            subject_user_id,
        );
        audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

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

    /// Coerce ScalarValue to target type if needed
    fn coerce_scalar_value(
        &self,
        value: ScalarValue,
        target: &KalamDataType,
    ) -> Result<ScalarValue, KalamDbError> {
        match (target, &value) {
            (KalamDataType::Uuid, ScalarValue::Utf8(Some(s))) => {
                // Parse UUID string to bytes
                let uuid = uuid::Uuid::parse_str(s).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Invalid UUID string '{}': {}", s, e))
                })?;
                Ok(ScalarValue::FixedSizeBinary(
                    16,
                    Some(uuid.as_bytes().to_vec()),
                ))
            }
            (KalamDataType::Uuid, ScalarValue::Utf8(None)) => {
                Ok(ScalarValue::FixedSizeBinary(16, None))
            }
            // Add other coercions if needed
            _ => Ok(value),
        }
    }

    /// Convert sqlparser Expr to ScalarValue for row construction
    fn expr_to_scalar_value(
        &self,
        expr: &Expr,
        params: &[ScalarValue],
        user_id: &kalamdb_commons::models::UserId,
        target_type: Option<&KalamDataType>,
    ) -> Result<ScalarValue, KalamDbError> {
        let value = match expr {
            Expr::Value(val_with_span) => {
                match &val_with_span.value {
                    sqlparser::ast::Value::Number(n, _) => {
                        // Try as i64 first, then f64
                        if let Ok(i) = n.parse::<i64>() {
                            Ok(ScalarValue::Int64(Some(i)))
                        } else if let Ok(f) = n.parse::<f64>() {
                            Ok(ScalarValue::Float64(Some(f)))
                        } else {
                            Err(KalamDbError::InvalidOperation(format!(
                                "Invalid number: {}",
                                n
                            )))
                        }
                    }
                    sqlparser::ast::Value::SingleQuotedString(s)
                    | sqlparser::ast::Value::DoubleQuotedString(s) => {
                        Ok(ScalarValue::Utf8(Some(s.clone())))
                    }
                    sqlparser::ast::Value::Boolean(b) => Ok(ScalarValue::Boolean(Some(*b))),
                    sqlparser::ast::Value::Null => Ok(ScalarValue::Null),
                    _ => Ok(ScalarValue::Utf8(Some(val_with_span.to_string()))),
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

                Ok(params[param_num - 1].clone())
            }
            Expr::Function(func) => {
                // Handle function calls (e.g. SNOWFLAKE_ID(), NOW(), etc.)
                let name = func.name.to_string();
                let mut args = Vec::new();

                match &func.args {
                    sqlparser::ast::FunctionArguments::List(arg_list) => {
                        for arg in &arg_list.args {
                            match arg {
                                sqlparser::ast::FunctionArg::Named { .. } => {
                                    return Err(KalamDbError::InvalidOperation(
                                        "Named function arguments not supported".into(),
                                    ));
                                }
                                sqlparser::ast::FunctionArg::Unnamed(arg_expr) => match arg_expr {
                                    sqlparser::ast::FunctionArgExpr::Expr(expr) => {
                                        args.push(self.expr_to_json_arg(expr, params, user_id)?);
                                    }
                                    _ => {
                                        return Err(KalamDbError::InvalidOperation(
                                            "Unsupported function argument expression type".into(),
                                        ));
                                    }
                                },
                                _ => {
                                    return Err(KalamDbError::InvalidOperation(
                                        "Unsupported function argument type".into(),
                                    ));
                                }
                            }
                        }
                    }
                    sqlparser::ast::FunctionArguments::None => {
                        // No arguments
                    }
                    _ => {
                        return Err(KalamDbError::InvalidOperation(
                            "Unsupported function argument format".into(),
                        ));
                    }
                }

                let col_default = ColumnDefault::FunctionCall { name, args };
                let sys_cols = self.app_context.system_columns_service();

                evaluate_default(&col_default, user_id, Some(sys_cols))
            }
            _ => {
                // For other expressions, convert to string (fallback)
                Ok(ScalarValue::Utf8(Some(expr.to_string())))
            }
        }?;

        if let Some(target) = target_type {
            self.coerce_scalar_value(value, target)
        } else {
            Ok(value)
        }
    }

    /// Convert Expr into JsonValue for DEFAULT function arguments
    fn expr_to_json_arg(
        &self,
        expr: &Expr,
        params: &[ScalarValue],
        user_id: &kalamdb_commons::models::UserId,
    ) -> Result<JsonValue, KalamDbError> {
        match expr {
            Expr::Value(val_with_span) => match &val_with_span.value {
                sqlparser::ast::Value::Number(n, _) => {
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
                | sqlparser::ast::Value::DoubleQuotedString(s) => Ok(JsonValue::String(s.clone())),
                sqlparser::ast::Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
                sqlparser::ast::Value::Null => Ok(JsonValue::Null),
                _ => Ok(JsonValue::String(val_with_span.to_string())),
            },
            Expr::Identifier(ident) if ident.value.starts_with('$') => {
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

                scalar_value_to_json(&params[param_num - 1])
            }
            Expr::Function(func) => {
                let name = func.name.to_string();
                let mut args = Vec::new();

                match &func.args {
                    sqlparser::ast::FunctionArguments::List(arg_list) => {
                        for arg in &arg_list.args {
                            match arg {
                                sqlparser::ast::FunctionArg::Named { .. } => {
                                    return Err(KalamDbError::InvalidOperation(
                                        "Named function arguments not supported".into(),
                                    ));
                                }
                                sqlparser::ast::FunctionArg::Unnamed(arg_expr) => match arg_expr {
                                    sqlparser::ast::FunctionArgExpr::Expr(expr) => {
                                        args.push(self.expr_to_json_arg(expr, params, user_id)?);
                                    }
                                    _ => {
                                        return Err(KalamDbError::InvalidOperation(
                                            "Unsupported function argument expression type".into(),
                                        ));
                                    }
                                },
                                _ => {
                                    return Err(KalamDbError::InvalidOperation(
                                        "Unsupported function argument type".into(),
                                    ));
                                }
                            }
                        }
                    }
                    sqlparser::ast::FunctionArguments::None => {}
                    _ => {
                        return Err(KalamDbError::InvalidOperation(
                            "Unsupported function argument format".into(),
                        ));
                    }
                }

                let col_default = ColumnDefault::FunctionCall { name, args };
                let sys_cols = self.app_context.system_columns_service();

                let scalar = evaluate_default(&col_default, user_id, Some(sys_cols))?;
                scalar_value_to_json(&scalar)
            }
            _ => Ok(JsonValue::String(expr.to_string())),
        }
    }

    /// Execute native INSERT using UserTableInsertHandler
    async fn execute_native_insert(
        &self,
        namespace: &NamespaceId,
        table_name: &TableName,
        user_id: &kalamdb_commons::models::UserId,
        role: kalamdb_commons::Role,
        rows: Vec<Row>,
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

        match (table_type, rows) {
            (TableType::User, rows) => {
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
            (TableType::Shared, rows) => {
                // Check write permissions for Shared tables
                // Public shared tables are read-only for regular users
                use kalamdb_auth::rbac::can_write_shared_table;
                use kalamdb_commons::schemas::TableOptions;
                use kalamdb_commons::TableAccess;

                let access_level = if let TableOptions::Shared(opts) = &table_def.table_options {
                    opts.access_level.clone().unwrap_or(TableAccess::Private)
                } else {
                    TableAccess::Private
                };

                if !can_write_shared_table(access_level.clone(), false, role) {
                    return Err(KalamDbError::Unauthorized(format!(
                        "Insufficient privileges to write to shared table '{}.{}' (Access Level: {:?})",
                        namespace.as_str(),
                        table_name.as_str(),
                        access_level
                    )));
                }

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
            (TableType::Stream, rows) => {
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
            (TableType::System, _) => Err(KalamDbError::InvalidOperation(
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
