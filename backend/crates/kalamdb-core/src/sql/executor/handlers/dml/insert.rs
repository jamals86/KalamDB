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
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_sql::ddl::InsertStatement;
use kalamdb_sql::statement_classifier::SqlStatement;
use serde_json::Value as JsonValue;
use sqlparser::ast::{Expr, Statement};
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
impl TypedStatementHandler<InsertStatement> for InsertHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: InsertStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement INSERT handler with TypedStatementHandler pattern
        // The marker type doesn't contain SQL text, needs architectural decision
        Err(KalamDbError::InvalidOperation(
            "INSERT handler not yet implemented for TypedStatementHandler pattern".into(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &InsertStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // INSERT requires at least User role
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
            _ => Err(KalamDbError::PermissionDenied(
                "INSERT requires User role or higher".to_string(),
            )),
        }
    }
}

impl InsertHandler {
    /// Parse table name from sqlparser TableObject
    fn parse_table_object(
        table_ref: &sqlparser::ast::TableObject,
    ) -> Result<(NamespaceId, TableName), KalamDbError> {
        // TableObject should be converted to string and parsed
        let table_str = table_ref.to_string();
        let parts: Vec<&str> = table_str.split('.').collect();

        match parts.len() {
            1 => {
                // Table without namespace -> use default
                Ok((
                    NamespaceId::new("default"),
                    TableName::new(parts[0].to_string()),
                ))
            }
            2 => {
                // Namespace.table
                Ok((
                    NamespaceId::new(parts[0].to_string()),
                    TableName::new(parts[1].to_string()),
                ))
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Invalid table name: {}",
                table_str
            ))),
        }
    }

    /// Parse table name from sqlparser ObjectName (legacy method, kept for compatibility)
    fn parse_table_name(
        table_ref: &sqlparser::ast::ObjectName,
    ) -> Result<(NamespaceId, TableName), KalamDbError> {
        let parts: Vec<String> = table_ref.0.iter().map(|ident| ident.to_string()).collect();

        match parts.len() {
            1 => {
                // Table without namespace -> use default
                Ok((
                    NamespaceId::new("default"),
                    TableName::new(parts[0].clone()),
                ))
            }
            2 => {
                // Namespace.table
                Ok((
                    NamespaceId::new(parts[0].clone()),
                    TableName::new(parts[1].clone()),
                ))
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Invalid table name: {}",
                table_ref
            ))),
        }
    }

    /// Count total placeholders in VALUES clause for validation (T062)
    fn count_placeholders(
        &self,
        rows: &[Vec<Expr>],
    ) -> Result<usize, KalamDbError> {
        let mut count = 0;
        for row in rows {
            for expr in row {
                if matches!(expr, Expr::Value(val) if matches!(val.value, sqlparser::ast::Value::Placeholder(_))) {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Bind a parameter to an expression value
    fn bind_parameter(
        &self,
        expr: &Expr,
        params: &[ScalarValue],
        param_index: &mut usize,
    ) -> Result<JsonValue, KalamDbError> {
        match expr {
            // $1, $2, etc.
            Expr::Value(val_with_span) => {
                match &val_with_span.value {
                    sqlparser::ast::Value::Placeholder(placeholder) => {
                        // Extract placeholder number
                        let param_num: usize = placeholder
                            .trim_start_matches('$')
                            .parse()
                            .map_err(|_| {
                                KalamDbError::InvalidOperation(format!(
                                    "Invalid placeholder: {}",
                                    placeholder
                                ))
                            })?;

                        if param_num == 0 || param_num > params.len() {
                            return Err(KalamDbError::InvalidOperation(format!(
                                "Parameter ${} out of range (have {} parameters)",
                                param_num,
                                params.len()
                            )));
                        }

                        // Convert DataFusion ScalarValue to JSON
                        self.scalar_value_to_json(&params[param_num - 1])
                    }
                    // Literal values (no parameter)
                    other_value => self.sqlparser_value_to_json(other_value),
                }
            }

            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported expression in INSERT VALUES: {:?}",
                expr
            ))),
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
            ScalarValue::Float32(Some(f)) => {
                serde_json::Number::from_f64(*f as f64)
                    .map(JsonValue::Number)
                    .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into()))
            }
            ScalarValue::Float64(Some(f)) => {
                serde_json::Number::from_f64(*f)
                    .map(JsonValue::Number)
                    .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into()))
            }
            ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => {
                Ok(JsonValue::String(s.clone()))
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported parameter type: {:?}",
                value
            ))),
        }
    }

    /// Convert sqlparser Value to JSON (for literals)
    fn sqlparser_value_to_json(
        &self,
        value: &sqlparser::ast::Value,
    ) -> Result<JsonValue, KalamDbError> {
        use sqlparser::ast::Value as SqlValue;

        match value {
            SqlValue::Null => Ok(JsonValue::Null),
            SqlValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
            SqlValue::Number(n, _) => {
                // Try parsing as i64 first, then f64
                if let Ok(i) = n.parse::<i64>() {
                    Ok(JsonValue::Number(i.into()))
                } else if let Ok(f) = n.parse::<f64>() {
                    serde_json::Number::from_f64(f)
                        .map(JsonValue::Number)
                        .ok_or_else(|| {
                            KalamDbError::InvalidOperation("Invalid number value".into())
                        })
                } else {
                    Err(KalamDbError::InvalidOperation(format!(
                        "Invalid number: {}",
                        n
                    )))
                }
            }
            SqlValue::SingleQuotedString(s) | SqlValue::DoubleQuotedString(s) => {
                Ok(JsonValue::String(s.clone()))
            }
            _ => Err(KalamDbError::InvalidOperation(format!(
                "Unsupported literal type: {:?}",
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

        // Get table definition (validates table exists)
        let _table_def = schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "Table not found: {}.{}",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Get UserTableShared from cache
        let user_table_shared = schema_registry
            .get_user_table_shared(&table_id)
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!(
                    "User table provider not found for: {}.{}",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Create UserTableAccess with current user context
        use crate::tables::user_tables::UserTableAccess;
        let user_access = UserTableAccess::new(
            user_table_shared,
            user_id.clone(),
            kalamdb_commons::Role::User,
        );

        // Execute insert batch
        let row_ids = user_access.insert_batch(rows)?;

        // Return rows_affected (T065)
        Ok(row_ids.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), role)
    }

    #[tokio::test]
    async fn test_insert_authorization_user() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::User);
        let stmt = kalamdb_sql::ddl::InsertStatement; // Typed statement marker

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_dba() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = kalamdb_sql::ddl::InsertStatement;

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_service() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = kalamdb_sql::ddl::InsertStatement;

        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual INSERT execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
