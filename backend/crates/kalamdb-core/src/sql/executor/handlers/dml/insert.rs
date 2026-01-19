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
use crate::error_extensions::KalamDbResultExt;
use crate::sql::executor::default_evaluator::evaluate_default;
use crate::sql::executor::handlers::dml::mod_helpers::{
    coerce_scalar_to_type, function_expr_to_scalar, scalar_from_placeholder, scalar_from_sql_value,
};
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::TableId;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
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
    pub fn new(app_context: std::sync::Arc<AppContext>) -> Self {
        Self { app_context }
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
        // Pass the current default namespace from session context for unqualified table names
        let default_namespace = context.default_namespace();
        let (namespace, table_name, columns, rows_data) =
            self.parse_insert_with_sqlparser(sql, &default_namespace)?;

        // Get table definition (validates existence and uses cached value if available)
        let namespace_owned = namespace.clone();
        let table_name_owned = table_name.clone();
        let table_id = TableId::new(namespace_owned.clone(), table_name_owned.clone());
        let schema_registry = self.app_context.schema_registry();

        // Single lookup: get_table_if_exists returns None if table doesn't exist
        // This is more efficient than calling table_exists + get_table_if_exists
        let table_def = schema_registry
            .get_table_if_exists(self.app_context.as_ref(), &table_id)?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Table '{}' does not exist", table_id))
            })?;

        // T163: Reject AS USER on Shared tables (Phase 7)
        if statement.as_user_id().is_some() && matches!(table_def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string(),
            ));
        }

        // Determine effective user for AS USER before evaluating defaults
        let effective_user_id = statement.as_user_id().unwrap_or(context.user_id());

        if columns.is_empty() {
            // Positional mapping: column order from schema will be used later; here just store as v1,v2...
            // For simplicity require explicit column list for MVP
            return Err(KalamDbError::InvalidOperation(
                "INSERT without explicit column list not supported yet".into(),
            ));
        }

        let provided_columns: HashSet<&str> = columns.iter().map(|c| c.as_str()).collect();
        // Get columns that need to be filled with defaults
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

        // Create a map of column name to nullable status for validation
        let col_nullable: std::collections::HashMap<String, bool> = table_def
            .columns
            .iter()
            .map(|c| (c.column_name.clone(), c.is_nullable))
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

                // Validate NOT NULL constraint
                if value.is_null() {
                    if let Some(is_nullable) = col_nullable.get(col) {
                        if !*is_nullable {
                            return Err(KalamDbError::ConstraintViolation(format!(
                                "Column '{}' cannot be null",
                                col
                            )));
                        }
                    }
                }

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

            // Validate missing NOT NULL columns without defaults
            for col_def in &table_def.columns {
                if SystemColumnNames::is_system_column(col_def.column_name.as_str()) {
                    continue;
                }
                if !col_def.is_nullable
                    && !values.contains_key(&col_def.column_name)
                    && col_def.default_value.is_none()
                {
                    return Err(KalamDbError::ConstraintViolation(format!(
                        "Column '{}' cannot be null",
                        col_def.column_name
                    )));
                }
            }

            rows.push(Row::new(values));
        }

        // T153: Execute native insert with impersonation support (Phase 7)
        // Use as_user_id if present, otherwise use context.user_id
        let rows_affected = self
            .execute_native_insert(
                &table_id,
                effective_user_id,
                context.user_role(),
                rows,
                table_def, // Pass already-fetched table definition to avoid redundant lookup
            )
            .await?;

        Ok(ExecutionResult::Inserted { rows_affected })
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;

        if !matches!(statement.kind(), SqlStatementKind::Insert(_)) {
            return Err(KalamDbError::InvalidOperation(
                "InsertHandler received wrong statement kind".into(),
            ));
        }

        // T050: Block anonymous users from write operations
        block_anonymous_write(context, "INSERT")?;

        // T152: Validate AS USER authorization - only Service/Dba/System can use AS USER (Phase 7)
        if statement.as_user_id().is_some() {
            use kalamdb_session::can_impersonate_user;
            if !can_impersonate_user(context.user_role()) {
                return Err(KalamDbError::Unauthorized(
                    format!("Role {:?} is not authorized to use AS USER. Only Service, Dba, and System roles are permitted.", context.user_role())
                ));
            }
        }

        use kalamdb_session::can_execute_dml;
        if can_execute_dml(context.user_role()) {
            Ok(())
        } else {
            Err(KalamDbError::Unauthorized(
                format!("Role {:?} is not authorized to execute INSERT.", context.user_role())
            ))
        }
    }
}

impl InsertHandler {
    /// Parse INSERT statement using sqlparser-rs
    /// Returns (namespace, table_name, columns, rows_of_exprs)
    ///
    /// # Arguments
    /// * `sql` - The SQL INSERT statement
    /// * `default_namespace` - The default namespace to use for unqualified table names
    #[allow(clippy::type_complexity)]
    fn parse_insert_with_sqlparser(
        &self,
        sql: &str,
        default_namespace: &NamespaceId,
    ) -> Result<(NamespaceId, TableName, Vec<String>, Vec<Vec<Expr>>), KalamDbError> {
        let dialect = GenericDialect {};
        let stmts = Parser::parse_sql(&dialect, sql).into_invalid_operation("SQL parse error")?;

        if stmts.is_empty() {
            return Err(KalamDbError::InvalidOperation("No SQL statement found".into()));
        }

        let Statement::Insert(insert) = &stmts[0] else {
            return Err(KalamDbError::InvalidOperation("Expected INSERT statement".into()));
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
            },
        };

        let (namespace, table_name) = match table_parts.len() {
            1 => (default_namespace.clone(), TableName::new(table_parts[0].clone())),
            2 => (NamespaceId::new(table_parts[0].clone()), TableName::new(table_parts[1].clone())),
            _ => return Err(KalamDbError::InvalidOperation("Invalid table name".into())),
        };

        // Extract columns
        let columns: Vec<String> = insert.columns.iter().map(|ident| ident.value.clone()).collect();

        // Extract VALUES rows
        let Some(ref source) = insert.source else {
            return Err(KalamDbError::InvalidOperation("INSERT missing VALUES clause".into()));
        };

        let rows = match source.body.as_ref() {
            sqlparser::ast::SetExpr::Values(SqlValues { rows, .. }) => rows.to_vec(),
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    "Only VALUES clause supported for INSERT".into(),
                ))
            },
        };

        Ok((namespace, table_name, columns, rows))
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
            Expr::Value(val_with_span) => scalar_from_sql_value(&val_with_span.value, params),
            Expr::Identifier(ident) if ident.value.starts_with('$') => {
                scalar_from_placeholder(&ident.value, params)
            },
            Expr::Function(func) => {
                function_expr_to_scalar(func, params, user_id, self.app_context.as_ref())
            },
            _ => {
                // For other expressions, convert to string (fallback)
                Ok(ScalarValue::Utf8(Some(expr.to_string())))
            },
        }?;

        if let Some(target) = target_type {
            coerce_scalar_to_type(value, target)
        } else {
            Ok(value)
        }
    }

    /// Execute native INSERT using UserTableInsertHandler
    ///
    /// **Phase 20**: Always routes through Raft consensus (single-node or cluster).
    /// This ensures consistent behavior and eliminates mode-specific code paths.
    async fn execute_native_insert(
        &self,
        table_id: &kalamdb_commons::models::TableId,
        user_id: &kalamdb_commons::models::UserId,
        role: kalamdb_commons::Role,
        rows: Vec<Row>,
        table_def: std::sync::Arc<kalamdb_commons::models::schemas::TableDefinition>,
    ) -> Result<usize, KalamDbError> {
        let table_type = table_def.table_type;
        let table_options = table_def.table_options.clone();

        // Always route through Raft for consistency (single-node or cluster)
        self.execute_insert_via_raft(table_id, table_type, &table_options, user_id, role, rows)
            .await
    }

    /// Execute INSERT via Raft consensus for cluster replication
    ///
    /// Serializes rows to bincode and submits through the appropriate data shard.
    /// All nodes (leader and followers) will apply the insert via their appliers.
    async fn execute_insert_via_raft(
        &self,
        table_id: &kalamdb_commons::models::TableId,
        table_type: TableType,
        table_options: &kalamdb_commons::schemas::TableOptions,
        user_id: &kalamdb_commons::models::UserId,
        role: kalamdb_commons::Role,
        rows: Vec<Row>,
    ) -> Result<usize, KalamDbError> {
        use kalamdb_raft::{DataResponse, SharedDataCommand, UserDataCommand};

        let row_count = rows.len();

        let executor = self.app_context.executor();

        match table_type {
            TableType::User => {
                // Validate write permissions for user tables
                use kalamdb_session::check_user_table_write_access_level;

                check_user_table_write_access_level(
                    role,
                    table_id.namespace_id(),
                    table_id.table_name(),
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                let cmd = UserDataCommand::Insert {
                    required_meta_index: 0, // Stamped by executor
                    table_id: table_id.clone(),
                    user_id: user_id.clone(),
                    rows,
                };

                let response = executor.execute_user_data(user_id, cmd).await.map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Raft insert failed: {}", e))
                })?;

                // Response contains the number of affected rows
                match response {
                    DataResponse::RowsAffected(n) => Ok(n),
                    DataResponse::Ok => Ok(row_count),
                    DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
                    _ => Ok(row_count),
                }
            },
            TableType::Shared => {
                // Validate write permissions for shared tables
                use kalamdb_commons::schemas::TableOptions;
                use kalamdb_commons::TableAccess;
                use kalamdb_session::check_shared_table_write_access_level;

                let access_level = if let TableOptions::Shared(opts) = table_options {
                    opts.access_level.unwrap_or(TableAccess::Private)
                } else {
                    TableAccess::Private
                };

                check_shared_table_write_access_level(
                    role,
                    access_level,
                    table_id.namespace_id(),
                    table_id.table_name(),
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                let cmd = SharedDataCommand::Insert {
                    required_meta_index: 0, // Stamped by executor
                    table_id: table_id.clone(),
                    rows: rows.clone(),
                };

                let response = executor.execute_shared_data(cmd).await.map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Raft insert failed: {}", e))
                })?;

                match response {
                    DataResponse::RowsAffected(n) => Ok(n),
                    DataResponse::Ok => Ok(row_count),
                    DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
                    _ => Ok(row_count),
                }
            },
            TableType::Stream => {
                // STREAM tables share the same write permissions as USER tables
                use kalamdb_session::check_stream_table_write_access_level;

                check_stream_table_write_access_level(
                    role,
                    table_id.namespace_id(),
                    table_id.table_name(),
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                let cmd = UserDataCommand::Insert {
                    required_meta_index: 0, // Stamped by executor
                    table_id: table_id.clone(),
                    user_id: user_id.clone(),
                    rows,
                };

                let response = executor.execute_user_data(user_id, cmd).await.map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Raft insert failed: {}", e))
                })?;

                match response {
                    DataResponse::RowsAffected(n) => Ok(n),
                    DataResponse::Ok => Ok(row_count),
                    DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
                    _ => Ok(row_count),
                }
            },
            TableType::System => {
                // System tables should not be written to through normal INSERT
                Err(KalamDbError::Unauthorized(format!(
                    "Cannot INSERT into system table '{}'",
                    table_id
                )))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{create_test_session_simple, test_app_context_simple};
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;
    use std::sync::Arc;

    fn init_app_context() -> Arc<AppContext> {
        test_app_context_simple()
    }

    fn test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), role, create_test_session_simple())
    }

    #[tokio::test]
    async fn test_insert_authorization_user() {
        let handler = InsertHandler::new(init_app_context());
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
        let handler = InsertHandler::new(init_app_context());
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
        let handler = InsertHandler::new(init_app_context());
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new(
            "INSERT INTO default.test (id) VALUES (1)".to_string(),
            SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }
}
