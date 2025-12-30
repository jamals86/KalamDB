//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::base::BaseTableProvider; // Phase 13.6: Bring trait methods into scope
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::{NamespaceId, Row, TableName};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::collections::BTreeMap;
use sqlparser::ast::{
    AssignmentTarget, BinaryOperator, Expr as SqlExpr, Ident, ObjectNamePart,
    Statement as SqlStatementAst, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

/// Handler for UPDATE statements
///
/// Delegates to DataFusion for UPDATE execution with parameter binding support.
/// Returns rows_affected count (only counts rows with actual changes, not rows matched).
pub struct UpdateHandler;

impl UpdateHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UpdateHandler {
    fn default() -> Self {
        Self::new()
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
        let app_context = AppContext::get();
        let limits = ParameterLimits::from_config(&app_context.config().execution);
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
        let schema_registry = app_context.schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry
            .get_table_definition(&table_id)?
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

        let mut values = BTreeMap::new();
        for (col, token) in assignments {
            let target_type = col_types.get(&col);
            let val = self.token_to_scalar_value(&token, &params, target_type)?;
            values.insert(col, val);
        }
        let updates = Row::new(values);

        // T153: Use effective user_id for impersonation support (Phase 7)
        let effective_user_id = statement.as_user_id().unwrap_or(&context.user_id);

        // T163: Reject AS USER on Shared tables (Phase 7)
        use kalamdb_commons::schemas::TableType;
        if statement.as_user_id().is_some() && matches!(def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string(),
            ));
        }

        match def.table_type {
            kalamdb_commons::schemas::TableType::User => {
                // Get provider from unified cache and downcast to UserTableProvider
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("User table provider not found".into())
                })?;

                // Coerce updates using provider's schema
                use crate::providers::arrow_json_conversion::coerce_updates;
                let updates = coerce_updates(updates, &provider_arc.schema()).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Update coercion failed: {}", e))
                })?;

                if let Some(provider) = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::UserTableProvider>()
                {
                    // T064: Get actual PK column name from provider instead of assuming "id"
                    let pk_column = provider.primary_key_field_name();

                    // Check if WHERE clause targets PK for fast path
                    let id_value_opt =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?;

                    if let Some(id_value) = id_value_opt {
                        match provider.update_by_id_field(
                            effective_user_id,
                            &id_value,
                            updates.clone(),
                        ) {
                            Ok(_) => {
                                Ok(ExecutionResult::Updated { rows_affected: 1 })
                            }
                            Err(e) => {
                                // Common case: PK is not a string (e.g. INT). Our update_by_id_field
                                // fast-path delegates to update_by_pk_value(&str), which may not be able
                                // to locate the row for non-string PK encodings.
                                // Fall back to a key lookup that compares ScalarValues correctly.
                                if matches!(e, crate::error::KalamDbError::NotFound(_)) {
                                    if let Some(key) = provider
                                        .find_row_key_by_id_field(effective_user_id, &id_value)?
                                    {
                                        provider.update(effective_user_id, &key, updates)?;
                                        return Ok(ExecutionResult::Updated { rows_affected: 1 });
                                    }

                                    // Isolation-friendly semantics: updating a non-existent row under this user_id
                                    // should return success with 0 rows affected (no-op), not an error.
                                    return Ok(ExecutionResult::Updated { rows_affected: 0 });
                                }

                                Err(e)
                            }
                        }
                    } else {
                        // Multi-row update path (scan -> update)
                        // Build filter expression
                        let (filter, filter_col_val) =
                            if let Some((col_name, val_str)) = &where_pair {
                                let col_type = col_types.get(col_name).ok_or_else(|| {
                                    KalamDbError::InvalidOperation(format!(
                                        "Column {} not found",
                                        col_name
                                    ))
                                })?;
                                let val =
                                    self.token_to_scalar_value(val_str, &params, Some(col_type))?;

                                use datafusion::prelude::{col, lit};
                                (
                                    Some(col(col_name).eq(lit(val.clone()))),
                                    Some((col_name.clone(), val)),
                                )
                            } else {
                                (None, None) // Update all rows
                            };

                        // Scan for matching rows
                        let rows = provider.scan_with_version_resolution_to_kvs(
                            effective_user_id,
                            filter.as_ref(),
                            None,
                            None,
                            false,
                        )?;

                        // Update each matching row
                        let mut count = 0;
                        for (key, row) in rows {
                            // Manual filter check (needed because scan_with_version_resolution_to_kvs
                            // might not filter hot rows by the expression)
                            let matches = if let Some((col_name, target_val)) = &filter_col_val {
                                if let Some(row_val) = row.fields.values.get(col_name) {
                                    row_val == target_val
                                } else {
                                    false // Column missing or null, doesn't match value
                                }
                            } else {
                                true // No filter, match all
                            };

                            if matches {
                                provider.update(effective_user_id, &key, updates.clone())?;
                                count += 1;
                            }
                        }

                        Ok(ExecutionResult::Updated {
                            rows_affected: count,
                        })
                    }
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for user table".into(),
                    ))
                }
            }
            kalamdb_commons::schemas::TableType::Shared => {
                // Check write permissions for Shared tables
                use kalamdb_auth::rbac::can_write_shared_table;
                use kalamdb_commons::schemas::TableOptions;
                use kalamdb_commons::TableAccess;

                let access_level = if let TableOptions::Shared(opts) = &def.table_options {
                    opts.access_level.unwrap_or(TableAccess::Private)
                } else {
                    TableAccess::Private
                };

                if !can_write_shared_table(access_level, false, context.user_role) {
                    return Err(KalamDbError::Unauthorized(format!(
                        "Insufficient privileges to write to shared table '{}.{}' (Access Level: {:?})",
                        namespace.as_str(),
                        table_name.as_str(),
                        access_level
                    )));
                }

                // For SHARED tables, also require WHERE on the actual PK column
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("Shared table provider not found".into())
                })?;

                // Coerce updates using provider's schema
                use crate::providers::arrow_json_conversion::coerce_updates;
                let updates = coerce_updates(updates, &provider_arc.schema()).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Update coercion failed: {}", e))
                })?;

                if let Some(provider) = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::SharedTableProvider>()
                {
                    let pk_column = provider.primary_key_field_name();
                    if let Some(id_value) =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                    {
                        // SharedTableProvider ignores user_id parameter (no RLS)
                        match provider.update_by_id_field(
                            effective_user_id,
                            &id_value,
                            updates.clone(),
                        ) {
                            Ok(_) => Ok(ExecutionResult::Updated { rows_affected: 1 }),
                            Err(crate::error::KalamDbError::NotFound(_)) => {
                                // Same fallback as USER tables: allow non-string PKs.
                                if let Some(key) = provider
                                    .find_row_key_by_id_field(effective_user_id, &id_value)?
                                {
                                    provider.update(effective_user_id, &key, updates)?;
                                    Ok(ExecutionResult::Updated { rows_affected: 1 })
                                } else {
                                    Ok(ExecutionResult::Updated { rows_affected: 0 })
                                }
                            }
                            Err(e) => Err(e),
                        }
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
            }
            kalamdb_commons::schemas::TableType::Stream => Err(
                KalamDbError::InvalidOperation("UPDATE not supported for STREAM tables".into()),
            ),
            kalamdb_commons::schemas::TableType::System => Err(
                KalamDbError::InvalidOperation("Cannot UPDATE SYSTEM tables".into()),
            ),
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

impl UpdateHandler {
    /// Parse UPDATE using sqlparser-rs to avoid string-splitting edge cases
    fn parse_update_with_sqlparser(
        &self,
        sql: &str,
        default_namespace: &NamespaceId,
    ) -> Result<
        (
            NamespaceId,
            TableName,
            Vec<(String, String)>,
            Option<(String, String)>,
        ),
        KalamDbError,
    > {
        let dialect = GenericDialect {};
        let mut stmts = Parser::parse_sql(&dialect, sql)
            .into_invalid_operation("Invalid UPDATE syntax")?;
        let stmt = stmts
            .pop()
            .ok_or_else(|| KalamDbError::InvalidOperation("Empty UPDATE statement".into()))?;

        let (ns, tbl, assigns, where_pair) = match stmt {
            SqlStatementAst::Update { table, assignments, selection, .. } => {
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
                            }
                            _ => {
                                return Err(KalamDbError::InvalidOperation(
                                    "Invalid UPDATE table reference".into(),
                                ))
                            }
                        }
                    }
                    _ => {
                        return Err(KalamDbError::InvalidOperation(
                            "Unsupported UPDATE target (only base tables allowed)".into(),
                        ))
                    }
                };

                let mut assigns = Vec::new();
                for assign in assignments {
                    let col = match assign.target {
                        AssignmentTarget::ColumnName(obj_name) => {
                            obj_name
                                .0
                                .iter()
                                .filter_map(ObjectNamePart::as_ident)
                                .map(|id| id.value.clone())
                                .next_back()
                                .ok_or_else(|| {
                                    KalamDbError::InvalidOperation(
                                        "Invalid column name in assignment".into(),
                                    )
                                })?
                        }
                        AssignmentTarget::Tuple(_) => {
                            return Err(KalamDbError::InvalidOperation(
                                "Tuple assignments are not supported".into(),
                            ))
                        }
                    };
                    assigns.push((col, assign.value.to_string()));
                }

                let where_pair = selection.and_then(|expr| match expr {
                    SqlExpr::BinaryOp { left, op: BinaryOperator::Eq, right } => {
                        match *left {
                            SqlExpr::Identifier(ident) => Some((ident.value, right.to_string())),
                            SqlExpr::CompoundIdentifier(idents) => idents
                                .last()
                                .map(|Ident { value, .. }| (value.clone(), right.to_string())),
                            _ => None,
                        }
                    }
                    _ => None,
                });

                (ns, tbl, assigns, where_pair)
            }
            _ => {
                return Err(KalamDbError::InvalidOperation(
                    "Expected UPDATE statement".into(),
                ))
            }
        };

        Ok((ns, tbl, assigns, where_pair))
    }

    fn extract_row_id_for_column(
        &self,
        where_pair: &Option<(String, String)>,
        pk_column: &str,
        params: &[ScalarValue],
    ) -> Result<Option<String>, KalamDbError> {
        if let Some((col, token)) = where_pair {
            // Ensure WHERE references the actual PK column
            if !col.eq_ignore_ascii_case(pk_column) {
                return Ok(None);
            }
            let t = token.trim();
            // Check for placeholder
            if let Some(stripped) = t.strip_prefix('$') {
                let num: usize = stripped.parse().map_err(|_| {
                    KalamDbError::InvalidOperation("Invalid placeholder in WHERE".into())
                })?;
                if num == 0 || num > params.len() {
                    return Err(KalamDbError::InvalidOperation(
                        "Placeholder index out of range".into(),
                    ));
                }
                let sv = &params[num - 1];
                return Ok(match sv {
                    ScalarValue::Int64(Some(i)) => Some(i.to_string()),
                    ScalarValue::Utf8(Some(s)) => Some(s.clone()),
                    _ => None,
                });
            }
            // Otherwise treat as literal (strip quotes if present)
            let unquoted = t.trim_matches('\'').trim_matches('"');
            return Ok(Some(unquoted.to_string()));
        }
        Ok(None)
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

    fn token_to_scalar_value(
        &self,
        token: &str,
        params: &[ScalarValue],
        target_type: Option<&KalamDataType>,
    ) -> Result<ScalarValue, KalamDbError> {
        let t = token.trim();

        // Check for placeholder ($1, $2, etc.)
        if let Some(stripped) = t.strip_prefix('$') {
            let param_num: usize = stripped.parse().map_err(|_| {
                KalamDbError::InvalidOperation(format!("Invalid placeholder: {}", t))
            })?;

            if param_num == 0 || param_num > params.len() {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Parameter ${} out of range (have {} parameters)",
                    param_num,
                    params.len()
                )));
            }

            let val = params[param_num - 1].clone();
            if let Some(target) = target_type {
                return self.coerce_scalar_value(val, target);
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
                return self.coerce_scalar_value(val, target);
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
            return self.coerce_scalar_value(val, target);
        }
        Ok(val)
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
    async fn test_update_authorization_user() {
        let handler = UpdateHandler::new();
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
        let handler = UpdateHandler::new();
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
        let handler = UpdateHandler::new();
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
