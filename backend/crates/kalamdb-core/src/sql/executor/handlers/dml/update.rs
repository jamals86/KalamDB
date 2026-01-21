//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::base::BaseTableProvider; // Phase 13.6: Bring trait methods into scope
use crate::sql::executor::handlers::dml::mod_helpers::{
    coerce_scalar_to_type, extract_pk_from_where_pair, scalar_from_placeholder,
    scalar_from_sql_value,
};
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{NamespaceId, TableId, TableName, UserId};
use kalamdb_raft::{DataResponse, SharedDataCommand, UserDataCommand};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use sqlparser::ast::{
    AssignmentTarget, BinaryOperator, Expr as SqlExpr, Ident, ObjectNamePart,
    Statement as SqlStatementAst, TableFactor, UnaryOperator,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::collections::BTreeMap;

/// Handler for UPDATE statements
///
/// Delegates to DataFusion for UPDATE execution with parameter binding support.
/// Returns rows_affected count (only counts rows with actual changes, not rows matched).
pub struct UpdateHandler {
    app_context: std::sync::Arc<AppContext>,
}

impl UpdateHandler {
    pub fn new(app_context: std::sync::Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait]
impl StatementHandler for UpdateHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // T063: Validate parameters before write using config from AppContext
        let limits = ParameterLimits::from_config(&self.app_context.config().execution);
        validate_parameters(&params, &limits)?;

        if !matches!(statement.kind(), SqlStatementKind::Update(_)) {
            return Err(KalamDbError::InvalidOperation(
                "UpdateHandler received wrong statement kind".into(),
            ));
        }
        let sql = statement.as_str();
        // Pass the current default namespace from session context for unqualified table names
        let default_namespace = context.default_namespace();
        let (namespace, table_name, assignments, where_pair) =
            self.parse_update_with_sqlparser(sql, &default_namespace)?;

        // Get table definition early to access schema for type coercion
        let schema_registry = self.app_context.schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry
            .get_table_if_exists(&table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {}.{} not found",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;

        // Create a map of column name to type for fast lookup
        let col_types: std::collections::HashMap<String, KalamDataType> = def
            .columns
            .iter()
            .map(|c| (c.column_name.clone(), c.data_type.clone()))
            .collect();

        let needs_row = assignments.iter().any(|(_, expr)| Self::expr_needs_row(expr));

        // T153: Use effective user_id for impersonation support (Phase 7)
        let effective_user_id = statement.as_user_id().unwrap_or(context.user_id());

        // T163: Reject AS USER on Shared tables (Phase 7)
        use kalamdb_commons::schemas::TableType;
        if statement.as_user_id().is_some() && matches!(def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string(),
            ));
        }

        match def.table_type {
            kalamdb_commons::schemas::TableType::User => {
                // Check write permissions for USER tables
                use kalamdb_session::check_user_table_write_access_level;
                check_user_table_write_access_level(
                    context.user_role(),
                    &namespace,
                    &table_name,
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                // Get provider from unified cache and downcast to UserTableProvider
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("User table provider not found".into())
                })?;

                if let Some(provider) =
                    provider_arc.as_any().downcast_ref::<crate::providers::UserTableProvider>()
                {
                    // T064: Get actual PK column name from provider instead of assuming "id"
                    let pk_column = provider.primary_key_field_name();

                    // Check if WHERE clause targets PK for fast path
                    let id_value_opt =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?;

                    // Phase 20: Always route through Raft (single-node or cluster)
                    // PK filter is required for update operations
                    if let Some(id_value) = id_value_opt {
                        let current_row = if needs_row {
                            let pk_scalar = self.token_to_scalar_value(
                                &id_value,
                                &params,
                                col_types.get(pk_column),
                            )?;
                            let (_row_id, row) = provider
                                .find_by_pk(effective_user_id, &pk_scalar)?
                                .ok_or_else(|| {
                                    KalamDbError::NotFound(format!(
                                        "Row with {}={} not found",
                                        pk_column, id_value
                                    ))
                                })?;
                            Some(row.fields)
                        } else {
                            None
                        };

                        let updates = self.build_updates(
                            &assignments,
                            &params,
                            &col_types,
                            current_row.as_ref(),
                        )?;

                        if updates.get(pk_column).is_some() {
                            return Err(KalamDbError::InvalidOperation(format!(
                                "UPDATE cannot modify primary key column '{}'",
                                pk_column
                            )));
                        }

                        // Coerce updates using provider's schema
                        use crate::providers::arrow_json_conversion::coerce_updates;
                        let updates =
                            coerce_updates(updates, &provider_arc.schema()).map_err(|e| {
                                KalamDbError::InvalidOperation(format!(
                                    "Update coercion failed: {}",
                                    e
                                ))
                            })?;

                        let rows_affected = self
                            .execute_user_update_via_raft(
                                &table_id,
                                effective_user_id,
                                &id_value,
                                updates.clone(),
                            )
                            .await?;
                        Ok(ExecutionResult::Updated { rows_affected })
                    } else {
                        Err(KalamDbError::InvalidOperation(
                            "UPDATE on user tables requires a primary key filter".to_string(),
                        ))
                    }
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for user table".into(),
                    ))
                }
            },
            kalamdb_commons::schemas::TableType::Shared => {
                // Check write permissions for Shared tables
                use kalamdb_commons::schemas::TableOptions;
                use kalamdb_commons::TableAccess;
                use kalamdb_session::check_shared_table_write_access_level;

                let access_level = if let TableOptions::Shared(opts) = &def.table_options {
                    opts.access_level.unwrap_or(TableAccess::Private)
                } else {
                    TableAccess::Private
                };

                check_shared_table_write_access_level(
                    context.user_role(),
                    access_level,
                    &namespace,
                    &table_name,
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                // For SHARED tables, also require WHERE on the actual PK column
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("Shared table provider not found".into())
                })?;

                if let Some(provider) =
                    provider_arc.as_any().downcast_ref::<crate::providers::SharedTableProvider>()
                {
                    let pk_column = provider.primary_key_field_name();

                    if let Some(id_value) =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                    {
                        let current_row = if needs_row {
                            let (_row_id, row) =
                                crate::providers::base::find_row_by_pk(provider, None, &id_value)?
                                    .ok_or_else(|| {
                                        KalamDbError::NotFound(format!(
                                            "Row with {}={} not found",
                                            pk_column, id_value
                                        ))
                                    })?;
                            Some(row.fields)
                        } else {
                            None
                        };

                        let updates = self.build_updates(
                            &assignments,
                            &params,
                            &col_types,
                            current_row.as_ref(),
                        )?;

                        if updates.get(pk_column).is_some() {
                            return Err(KalamDbError::InvalidOperation(format!(
                                "UPDATE cannot modify primary key column '{}'",
                                pk_column
                            )));
                        }

                        // Coerce updates using provider's schema
                        use crate::providers::arrow_json_conversion::coerce_updates;
                        let updates =
                            coerce_updates(updates, &provider_arc.schema()).map_err(|e| {
                                KalamDbError::InvalidOperation(format!(
                                    "Update coercion failed: {}",
                                    e
                                ))
                            })?;
                        // Phase 20: Always route through Raft (single-node or cluster)
                        let rows_affected =
                            self.execute_update_via_raft(&table_id, &id_value, updates).await?;
                        Ok(ExecutionResult::Updated { rows_affected })
                    } else {
                        Err(KalamDbError::InvalidOperation(format!(
                            "UPDATE on SHARED tables requires WHERE {} = <value> (predicate updates not yet supported)",
                            pk_column
                        )))
                    }
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for shared table".into(),
                    ))
                }
            },
            kalamdb_commons::schemas::TableType::Stream => {
                // STREAM tables share the same write permissions as USER tables
                use kalamdb_session::check_stream_table_write_access_level;
                check_stream_table_write_access_level(
                    context.user_role(),
                    &namespace,
                    &table_name,
                )
                .map_err(|e| KalamDbError::Unauthorized(e.to_string()))?;

                Err(KalamDbError::InvalidOperation("UPDATE not supported for STREAM tables".into()))
            },
            kalamdb_commons::schemas::TableType::System => {
                Err(KalamDbError::InvalidOperation("Cannot UPDATE SYSTEM tables".into()))
            },
        }
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;

        if !matches!(statement.kind(), SqlStatementKind::Update(_)) {
            return Err(KalamDbError::InvalidOperation(
                "UpdateHandler received wrong statement kind".into(),
            ));
        }

        // T050: Block anonymous users from write operations
        block_anonymous_write(context, "UPDATE")?;

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
                format!("Role {:?} is not authorized to execute UPDATE.", context.user_role())
            ))
        }
    }
}

impl UpdateHandler {
    /// Parse UPDATE using sqlparser-rs to avoid string-splitting edge cases
    #[allow(clippy::type_complexity)]
    fn parse_update_with_sqlparser(
        &self,
        sql: &str,
        default_namespace: &NamespaceId,
    ) -> Result<
        (NamespaceId, TableName, Vec<(String, SqlExpr)>, Option<(String, String)>),
        KalamDbError,
    > {
        let dialect = GenericDialect {};
        let mut stmts =
            Parser::parse_sql(&dialect, sql).into_invalid_operation("Invalid UPDATE syntax")?;
        let stmt = stmts
            .pop()
            .ok_or_else(|| KalamDbError::InvalidOperation("Empty UPDATE statement".into()))?;

        let (ns, tbl, assigns, where_pair) = match stmt {
            SqlStatementAst::Update {
                table,
                assignments,
                selection,
                ..
            } => {

                let (ns, tbl) = match table.relation {
                    TableFactor::Table { name, .. } => {
                        let parts: Vec<String> = name
                            .0
                            .iter()
                            .filter_map(ObjectNamePart::as_ident)
                            .map(|id| id.value.clone())
                            .collect();
                        match parts.as_slice() {
                            [table] => (default_namespace.clone(), TableName::new(table.clone())),
                            [ns, table] => {
                                (NamespaceId::new(ns.clone()), TableName::new(table.clone()))
                            },
                            _ => {
                                return Err(KalamDbError::InvalidOperation(
                                    "Invalid UPDATE table reference".into(),
                                ))
                            },
                        }
                    },
                    _ => {
                        return Err(KalamDbError::InvalidOperation(
                            "Unsupported UPDATE target (only base tables allowed)".into(),
                        ))
                    },
                };

                let mut assigns = Vec::new();
                for assign in assignments {
                    let col = match assign.target {
                        AssignmentTarget::ColumnName(obj_name) => obj_name
                            .0
                            .iter()
                            .filter_map(ObjectNamePart::as_ident)
                            .map(|id| id.value.clone())
                            .next_back()
                            .ok_or_else(|| {
                                KalamDbError::InvalidOperation(
                                    "Invalid column name in assignment".into(),
                                )
                            })?,
                        AssignmentTarget::Tuple(_) => {
                            return Err(KalamDbError::InvalidOperation(
                                "Tuple assignments are not supported".into(),
                            ))
                        },
                    };
                    assigns.push((col, assign.value));
                }

                let where_pair =
                    selection.and_then(|expr| Self::extract_pk_filter_from_expr(&expr));

                (ns, tbl, assigns, where_pair)
            },
            _ => return Err(KalamDbError::InvalidOperation("Expected UPDATE statement".into())),
        };

        Ok((ns, tbl, assigns, where_pair))
    }

    /// Extract primary key filter from a WHERE expression.
    /// Handles compound expressions like "pk_col = value AND other_col = value".
    fn extract_pk_filter_from_expr(expr: &SqlExpr) -> Option<(String, String)> {
        match expr {
            // Simple equality: id = value
            SqlExpr::BinaryOp {
                left,
                op: BinaryOperator::Eq,
                right,
            } => match left.as_ref() {
                SqlExpr::Identifier(ident) => Some((ident.value.clone(), right.to_string())),
                SqlExpr::CompoundIdentifier(idents) => {
                    idents.last().map(|Ident { value, .. }| (value.clone(), right.to_string()))
                },
                _ => None,
            },
            // Compound condition with AND: try to find PK filter in either side
            SqlExpr::BinaryOp {
                left,
                op: BinaryOperator::And,
                right,
            } => {
                // Try left side first, then right side
                Self::extract_pk_filter_from_expr(left)
                    .or_else(|| Self::extract_pk_filter_from_expr(right))
            },
            // Parenthesized expression: unwrap and recurse
            SqlExpr::Nested(inner) => Self::extract_pk_filter_from_expr(inner),
            _ => None,
        }
    }

    fn extract_row_id_for_column(
        &self,
        where_pair: &Option<(String, String)>,
        pk_column: &str,
        params: &[ScalarValue],
    ) -> Result<Option<String>, KalamDbError> {
        extract_pk_from_where_pair(where_pair, pk_column, params)
    }

    fn token_to_scalar_value(
        &self,
        token: &str,
        params: &[ScalarValue],
        target_type: Option<&KalamDataType>,
    ) -> Result<ScalarValue, KalamDbError> {
        let t = token.trim();

        // Check for placeholder ($1, $2, etc.)
        if t.starts_with('$') {
            let val = scalar_from_placeholder(t, params)?;
            if let Some(target) = target_type {
                return coerce_scalar_to_type(val, target);
            }
            return Ok(val);
        }

        // Check for NULL
        if t.eq_ignore_ascii_case("NULL") {
            return Ok(ScalarValue::Null);
        }

        // Check for boolean
        if t.eq_ignore_ascii_case("TRUE") {
            return Ok(ScalarValue::Boolean(Some(true)));
        }
        if t.eq_ignore_ascii_case("FALSE") {
            return Ok(ScalarValue::Boolean(Some(false)));
        }

        // Check for quoted string
        if (t.starts_with('\'') && t.ends_with('\''))
            || (t.starts_with('"') && t.ends_with('"'))
            || (t.starts_with('`') && t.ends_with('`'))
        {
            let unquoted = &t[1..t.len() - 1];
            let val = ScalarValue::Utf8(Some(unquoted.to_string()));
            if let Some(target) = target_type {
                return coerce_scalar_to_type(val, target);
            }
            return Ok(val);
        }

        // Try parsing as number
        if let Ok(i) = t.parse::<i64>() {
            return Ok(ScalarValue::Int64(Some(i)));
        }
        if let Ok(f) = t.parse::<f64>() {
            return Ok(ScalarValue::Float64(Some(f)));
        }

        // Default to string
        let val = ScalarValue::Utf8(Some(t.to_string()));
        if let Some(target) = target_type {
            return coerce_scalar_to_type(val, target);
        }
        Ok(val)
    }

    fn build_updates(
        &self,
        assignments: &[(String, SqlExpr)],
        params: &[ScalarValue],
        col_types: &std::collections::HashMap<String, KalamDataType>,
        current_row: Option<&Row>,
    ) -> Result<Row, KalamDbError> {
        let mut values = BTreeMap::new();
        for (col, expr) in assignments {
            let target_type = col_types.get(col);
            let val = self.expr_to_scalar_value(expr, params, current_row, target_type)?;
            values.insert(col.clone(), val);
        }
        Ok(Row::new(values))
    }

    fn expr_needs_row(expr: &SqlExpr) -> bool {
        match expr {
            SqlExpr::Identifier(_) | SqlExpr::CompoundIdentifier(_) => true,
            SqlExpr::BinaryOp { left, right, .. } => {
                Self::expr_needs_row(left) || Self::expr_needs_row(right)
            },
            SqlExpr::UnaryOp { expr, .. } => Self::expr_needs_row(expr),
            SqlExpr::Nested(inner) => Self::expr_needs_row(inner),
            SqlExpr::Cast { expr, .. } => Self::expr_needs_row(expr),
            _ => false,
        }
    }

    fn expr_to_scalar_value(
        &self,
        expr: &SqlExpr,
        params: &[ScalarValue],
        current_row: Option<&Row>,
        target_type: Option<&KalamDataType>,
    ) -> Result<ScalarValue, KalamDbError> {
        let mut value = match expr {
            SqlExpr::Value(v) => scalar_from_sql_value(&v.value, params)?,
            SqlExpr::Identifier(ident) => {
                let row = current_row.ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Cannot resolve column '{}' without a row context",
                        ident.value
                    ))
                })?;
                row.values.get(&ident.value).cloned().ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Column '{}' not found in current row",
                        ident.value
                    ))
                })?
            },
            SqlExpr::CompoundIdentifier(idents) => {
                let col =
                    idents.last().map(|Ident { value, .. }| value.clone()).ok_or_else(|| {
                        KalamDbError::InvalidOperation(
                            "Invalid compound identifier in update expression".into(),
                        )
                    })?;
                let row = current_row.ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Cannot resolve column '{}' without a row context",
                        col
                    ))
                })?;
                row.values.get(&col).cloned().ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Column '{}' not found in current row",
                        col
                    ))
                })?
            },
            SqlExpr::BinaryOp { left, op, right } => {
                let left_val = self.expr_to_scalar_value(left, params, current_row, None)?;
                let right_val = self.expr_to_scalar_value(right, params, current_row, None)?;
                self.apply_numeric_binary_op(op.clone(), left_val, right_val)?
            },
            SqlExpr::UnaryOp { op, expr } => {
                let inner = self.expr_to_scalar_value(expr, params, current_row, None)?;
                match op {
                    UnaryOperator::Minus => {
                        if let Some(n) = Self::scalar_to_i64(&inner) {
                            ScalarValue::Int64(Some(-n))
                        } else if let Some(n) = Self::scalar_to_f64(&inner) {
                            ScalarValue::Float64(Some(-n))
                        } else {
                            return Err(KalamDbError::InvalidOperation(format!(
                                "Unary '-' not supported for value {:?}",
                                inner
                            )));
                        }
                    },
                    UnaryOperator::Plus => inner,
                    _ => {
                        return Err(KalamDbError::InvalidOperation(format!(
                            "Unsupported unary operator {:?} in UPDATE expression",
                            op
                        )))
                    },
                }
            },
            SqlExpr::Nested(inner) => {
                self.expr_to_scalar_value(inner, params, current_row, None)?
            },
            SqlExpr::Cast { expr, .. } => {
                self.expr_to_scalar_value(expr, params, current_row, None)?
            },
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unsupported UPDATE expression: {}",
                    expr
                )))
            },
        };

        if let Some(target) = target_type {
            value = coerce_scalar_to_type(value, target)?;
        }

        Ok(value)
    }

    fn apply_numeric_binary_op(
        &self,
        op: BinaryOperator,
        left: ScalarValue,
        right: ScalarValue,
    ) -> Result<ScalarValue, KalamDbError> {
        if matches!(left, ScalarValue::Null) || matches!(right, ScalarValue::Null) {
            return Ok(ScalarValue::Null);
        }

        let left_int = Self::scalar_to_i64(&left);
        let right_int = Self::scalar_to_i64(&right);
        let left_float = Self::scalar_to_f64(&left);
        let right_float = Self::scalar_to_f64(&right);

        match op {
            BinaryOperator::Plus
            | BinaryOperator::Minus
            | BinaryOperator::Multiply
            | BinaryOperator::Divide => {},
            _ => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Unsupported binary operator {:?} in UPDATE expression",
                    op
                )))
            },
        }

        if let (Some(l), Some(r)) = (left_int, right_int) {
            let result = match op {
                BinaryOperator::Plus => ScalarValue::Int64(Some(l + r)),
                BinaryOperator::Minus => ScalarValue::Int64(Some(l - r)),
                BinaryOperator::Multiply => ScalarValue::Int64(Some(l * r)),
                BinaryOperator::Divide => ScalarValue::Float64(Some(l as f64 / r as f64)),
                _ => unreachable!(),
            };
            return Ok(result);
        }

        if left_float.is_none() || right_float.is_none() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Non-numeric operands in UPDATE expression: {:?} {:?}",
                left, right
            )));
        }

        let l = left_float.unwrap();
        let r = right_float.unwrap();
        let result = match op {
            BinaryOperator::Plus => l + r,
            BinaryOperator::Minus => l - r,
            BinaryOperator::Multiply => l * r,
            BinaryOperator::Divide => l / r,
            _ => unreachable!(),
        };

        Ok(ScalarValue::Float64(Some(result)))
    }

    fn scalar_to_f64(value: &ScalarValue) -> Option<f64> {
        match value {
            ScalarValue::Float64(Some(v)) => Some(*v),
            ScalarValue::Float32(Some(v)) => Some(f64::from(*v)),
            ScalarValue::Int64(Some(v)) => Some(*v as f64),
            ScalarValue::Int32(Some(v)) => Some(*v as f64),
            ScalarValue::Int16(Some(v)) => Some(*v as f64),
            ScalarValue::UInt64(Some(v)) => Some(*v as f64),
            ScalarValue::UInt32(Some(v)) => Some(*v as f64),
            _ => None,
        }
    }

    fn scalar_to_i64(value: &ScalarValue) -> Option<i64> {
        match value {
            ScalarValue::Int64(Some(v)) => Some(*v),
            ScalarValue::Int32(Some(v)) => Some(*v as i64),
            ScalarValue::Int16(Some(v)) => Some(*v as i64),
            ScalarValue::UInt64(Some(v)) => i64::try_from(*v).ok(),
            ScalarValue::UInt32(Some(v)) => Some(*v as i64),
            _ => None,
        }
    }

    /// Execute UPDATE via Raft consensus for cluster replication
    async fn execute_update_via_raft(
        &self,
        table_id: &TableId,
        pk_value: &str,
        updates: Row,
    ) -> Result<usize, KalamDbError> {
        let executor = self.app_context.executor();

        let cmd = SharedDataCommand::Update {
            required_meta_index: 0, // Stamped by executor
            table_id: table_id.clone(),
            updates: vec![updates],
            filter: Some(pk_value.to_string()),
        };

        let response = executor
            .execute_shared_data(cmd)
            .await
            .map_err(|e| KalamDbError::InvalidOperation(format!("Raft update failed: {}", e)))?;

        match response {
            DataResponse::RowsAffected(n) => Ok(n),
            DataResponse::Ok => Ok(1),
            DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
            _ => Ok(1),
        }
    }

    /// Execute UPDATE via Raft consensus for user tables
    async fn execute_user_update_via_raft(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<usize, KalamDbError> {
        let executor = self.app_context.executor();

        let cmd = UserDataCommand::Update {
            required_meta_index: 0, // Stamped by executor
            table_id: table_id.clone(),
            user_id: user_id.clone(),
            updates: vec![updates],
            filter: Some(pk_value.to_string()),
        };

        let response = executor.execute_user_data(user_id, cmd).await.map_err(|e| {
            KalamDbError::InvalidOperation(format!("Raft user update failed: {}", e))
        })?;

        match response {
            DataResponse::RowsAffected(n) => Ok(n),
            DataResponse::Ok => Ok(1),
            DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
            _ => Ok(1),
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
    async fn test_update_authorization_user() {
        let handler = UpdateHandler::new(init_app_context());
        let ctx = test_context(Role::User);
        let stmt = SqlStatement::new(
            "UPDATE default.test SET x = 1 WHERE id = 1".to_string(),
            SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_dba() {
        let handler = UpdateHandler::new(init_app_context());
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new(
            "UPDATE default.test SET x = 1 WHERE id = 1".to_string(),
            SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_service() {
        let handler = UpdateHandler::new(init_app_context());
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new(
            "UPDATE default.test SET x = 1 WHERE id = 1".to_string(),
            SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual UPDATE execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
