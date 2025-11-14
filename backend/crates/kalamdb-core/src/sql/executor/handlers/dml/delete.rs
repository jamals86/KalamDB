//! DELETE Handler
//!
//! Handles DELETE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::providers::base::BaseTableProvider; // Phase 13.6: Bring trait methods into scope
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use kalamdb_commons::models::{NamespaceId, TableName};
use crate::app_context::AppContext;

/// Handler for DELETE statements
///
/// Delegates to DataFusion for DELETE execution with parameter binding support.
/// Returns rows_affected count following MySQL semantics.
pub struct DeleteHandler;

impl DeleteHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DeleteHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StatementHandler for DeleteHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // T064: Validate parameters before write using config from AppContext
        let app_context = AppContext::get();
        let limits = ParameterLimits::from_config(&app_context.config().execution);
        validate_parameters(&_params, &limits)?;

        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation("DeleteHandler received wrong statement kind".into()));
        }
    let sql = statement.as_str();
    let (namespace, table_name, row_id) = self.simple_parse_delete(sql)?;
        if row_id.is_none() { return Err(KalamDbError::InvalidOperation("DELETE currently requires WHERE id = <value>".into())); }
        let row_id = row_id.unwrap();

        // Execute native delete
        let schema_registry = AppContext::get().schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry.get_table_definition(&table_id)?.ok_or_else(|| KalamDbError::NotFound(format!("Table {}.{} not found", namespace.as_str(), table_name.as_str())))?;
        use kalamdb_commons::schemas::TableType;
        match def.table_type {
            TableType::User => {
                // Get provider from unified cache and downcast to UserTableProvider
                let provider_arc = schema_registry.get_provider(&table_id)
                    .ok_or_else(|| KalamDbError::InvalidOperation("User table provider not found".into()))?;
                
                if let Some(provider) = provider_arc.as_any().downcast_ref::<crate::providers::UserTableProvider>() {
                    let _deleted = provider.delete_by_id_field(&context.user_id, &row_id)?;
                    Ok(ExecutionResult::Deleted { rows_affected: 1 })
                } else {
                    Err(KalamDbError::InvalidOperation("Cached provider type mismatch for user table".into()))
                }
            }
            TableType::Shared => {
                // DELETE FROM <ns>.<table> WHERE id = <value> for SHARED tables maps logical id -> row_id
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| KalamDbError::InvalidOperation("Shared table provider not found".into()))?;
                if let Some(provider) = provider_arc.as_any().downcast_ref::<crate::providers::SharedTableProvider>() {
                    // SharedTableProvider ignores user_id parameter (no RLS)
                    provider.delete_by_id_field(&context.user_id, &row_id)?;
                    Ok(ExecutionResult::Deleted { rows_affected: 1 })
                } else {
                    Err(KalamDbError::InvalidOperation("Cached provider type mismatch for shared table".into()))
                }
            }
            TableType::Stream => Err(KalamDbError::InvalidOperation("DELETE not supported for STREAM tables".into())),
            TableType::System => Err(KalamDbError::InvalidOperation("Cannot DELETE from SYSTEM tables".into())),
        }
    }

    async fn check_authorization(&self, statement: &SqlStatement, context: &ExecutionContext) -> Result<(), KalamDbError> {
        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation("DeleteHandler received wrong statement kind".into()));
        }
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
        }
    }
}

impl DeleteHandler {
    fn simple_parse_delete(&self, sql: &str) -> Result<(NamespaceId, TableName, Option<String>), KalamDbError> {
        // Expect: DELETE FROM <ns>.<table> WHERE id = <value>
        let upper = sql.to_uppercase();
        let from_pos = upper.find("DELETE FROM ").ok_or_else(|| KalamDbError::InvalidOperation("Missing 'DELETE FROM'".into()))?;
        let where_pos = upper.find(" WHERE ");
        let head = if let Some(wp) = where_pos { &sql[from_pos + 12..wp] } else { &sql[from_pos + 12..] };
        let (ns, tbl) = {
            let parts: Vec<&str> = head.trim().split('.').collect();
            match parts.len() { 1 => (NamespaceId::new("default"), TableName::new(parts[0].trim().to_string())), 2 => (NamespaceId::new(parts[0].trim().to_string()), TableName::new(parts[1].trim().to_string())), _ => return Err(KalamDbError::InvalidOperation("Invalid table reference".into())) }
        };
        let row_id = if let Some(wp) = where_pos {
            let cond = sql[wp + 7..].trim();
            // Support only 'id = <literal>'
            let parts: Vec<&str> = cond.split('=').collect();
            if parts.len() == 2 && parts[0].trim().eq_ignore_ascii_case("id") {
                let v = parts[1].trim().trim_matches('\'').trim_matches('"').to_string();
                Some(v)
            } else { None }
        } else { None };
        Ok((ns, tbl, row_id))
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
    async fn test_delete_authorization_user() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::User);
        let stmt = SqlStatement::new("DELETE FROM default.test WHERE id = 1".to_string(), SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_dba() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new("DELETE FROM default.test WHERE id = 1".to_string(), SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_service() {
        let handler = DeleteHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new("DELETE FROM default.test WHERE id = 1".to_string(), SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual DELETE execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
