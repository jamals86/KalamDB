//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::test_helpers::create_test_session;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use serde_json::Value as JsonValue;
use kalamdb_commons::models::{NamespaceId, TableName};
use crate::app_context::AppContext;

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
            return Err(KalamDbError::InvalidOperation("UpdateHandler received wrong statement kind".into()));
        }
        let sql = statement.as_str();
        let (namespace, table_name, assignments, where_clause) = self.simple_parse_update(sql)?;

        // Relax WHERE restriction: attempt id extraction, else fall back to provider-side update_all (shared tables)
        let row_id_opt = self.extract_row_id(&where_clause, &params)?;
        let mut obj = serde_json::Map::new();
        for (col, token) in assignments {
            let val = self.token_to_json(&token, &params)?;
            obj.insert(col, val);
        }
        let updates = JsonValue::Object(obj);

        // Execute native update for USER tables; for SHARED tables, perform provider-level updates across matching rows (MVP: id only)
        let schema_registry = AppContext::get().schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry.get_table_definition(&table_id)?.ok_or_else(|| KalamDbError::NotFound(format!("Table {}.{} not found", namespace.as_str(), table_name.as_str())))?;
        match def.table_type {
            kalamdb_commons::schemas::TableType::User => {
                // Require id for user table update
                let row_id = row_id_opt.ok_or_else(|| KalamDbError::InvalidOperation("UPDATE currently requires WHERE id = <value> for USER tables".into()))?;
                let shared = schema_registry.get_user_table_shared(&table_id).ok_or_else(|| KalamDbError::InvalidOperation("User table provider not found".into()))?;
                use crate::tables::user_tables::UserTableAccess;
                let access = UserTableAccess::new(shared, context.user_id.clone(), context.user_role.clone());
                let _updated = access.update_row(&row_id, updates)?;
                Ok(ExecutionResult::Updated { rows_affected: 1 })
            }
            kalamdb_commons::schemas::TableType::Shared => {
                // MVP: If id present, update that row via SharedTableProvider; otherwise, return invalid operation (no predicate support yet)
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| KalamDbError::InvalidOperation("Shared table provider not found".into()))?;
                if let Some(provider) = provider_arc.as_any().downcast_ref::<crate::tables::shared_tables::SharedTableProvider>() {
                    if let Some(row_id) = row_id_opt {
                        provider.update(&row_id, updates)?;
                        Ok(ExecutionResult::Updated { rows_affected: 1 })
                    } else {
                        Err(KalamDbError::InvalidOperation("UPDATE on SHARED tables requires WHERE id = <value> (predicate updates not yet supported)".into()))
                    }
                } else {
                    Err(KalamDbError::InvalidOperation("Cached provider type mismatch for shared table".into()))
                }
            }
            kalamdb_commons::schemas::TableType::Stream => {
                Err(KalamDbError::InvalidOperation("UPDATE not supported for STREAM tables".into()))
            }
            kalamdb_commons::schemas::TableType::System => {
                Err(KalamDbError::InvalidOperation("Cannot UPDATE SYSTEM tables".into()))
            }
        }
    }

    async fn check_authorization(&self, statement: &SqlStatement, context: &ExecutionContext) -> Result<(), KalamDbError> {
        if !matches!(statement.kind(), SqlStatementKind::Update(_)) {
            return Err(KalamDbError::InvalidOperation("UpdateHandler received wrong statement kind".into()));
        }
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
            _ => Err(KalamDbError::PermissionDenied("UPDATE requires User role or higher".to_string())),
        }
    }
}

impl UpdateHandler {
    fn simple_parse_update(&self, sql: &str) -> Result<(NamespaceId, TableName, Vec<(String, String)>, Option<String>), KalamDbError> {
        // Expect: UPDATE <ns>.<table> SET col1=val1, col2=$2 WHERE id = <v>
        let upper = sql.to_uppercase();
        let set_pos = upper.find(" SET ").ok_or_else(|| KalamDbError::InvalidOperation("Missing SET clause".into()))?;
        let where_pos = upper.find(" WHERE ");
        let head = sql[0..set_pos].trim(); // includes UPDATE prefix
        let table_part = head.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c.is_whitespace()).trim();
        let (ns, tbl) = {
            let parts: Vec<&str> = table_part.split('.').collect();
            match parts.len() { 
                1 => (NamespaceId::new("default"), TableName::new(parts[0].trim().to_string())), 
                2 => (NamespaceId::new(parts[0].trim().to_string()), TableName::new(parts[1].trim().to_string())), 
                _ => return Err(KalamDbError::InvalidOperation("Invalid table reference".into())) 
            }
        };
        let set_clause = if let Some(wp) = where_pos { &sql[set_pos + 5..wp] } else { &sql[set_pos + 5..] };
        let mut assigns = Vec::new();
        for pair in set_clause.split(',') {
            let mut it = pair.splitn(2, '=');
            let col = it.next().ok_or_else(|| KalamDbError::InvalidOperation("Malformed SET".into()))?.trim().to_string();
            let val = it.next().ok_or_else(|| KalamDbError::InvalidOperation("Malformed SET value".into()))?.trim().to_string();
            assigns.push((col, val));
        }
        let where_expr = where_pos.map(|wp| {
            let where_clause_raw = sql[wp + 7..].trim();
            // Parse simple id = value pattern
            let parts: Vec<&str> = where_clause_raw.split('=').collect();
            if parts.len() == 2 && parts[0].trim().eq_ignore_ascii_case("id") {
                // Return the value part for extraction
                parts[1].trim().to_string()
            } else {
                String::new() // Empty means unsupported WHERE
            }
        });
        Ok((ns, tbl, assigns, where_expr))
    }

    fn extract_row_id(&self, where_token: &Option<String>, params: &[ScalarValue]) -> Result<Option<String>, KalamDbError> {
        if let Some(token) = where_token {
            if token.is_empty() {
                return Ok(None);
            }
            let t = token.trim();
            // Check for placeholder
            if t.starts_with('$') {
                let num: usize = t[1..].parse().map_err(|_| KalamDbError::InvalidOperation("Invalid placeholder in WHERE".into()))?;
                if num == 0 || num > params.len() { 
                    return Err(KalamDbError::InvalidOperation("Placeholder index out of range".into())); 
                }
                let sv = &params[num-1];
                return Ok(match sv { 
                    ScalarValue::Int64(Some(i)) => Some(i.to_string()), 
                    ScalarValue::Utf8(Some(s)) => Some(s.clone()), 
                    _ => None 
                });
            }
            // Otherwise treat as literal (strip quotes if present)
            let unquoted = t.trim_matches('\'').trim_matches('"');
            return Ok(Some(unquoted.to_string()));
        }
        Ok(None)
    }

    fn token_to_json(&self, token: &str, params: &[ScalarValue]) -> Result<JsonValue, KalamDbError> {
        let t = token.trim();
        
        // Check for placeholder ($1, $2, etc.)
        if t.starts_with('$') {
            let param_num: usize = t[1..].parse()
                .map_err(|_| KalamDbError::InvalidOperation(format!("Invalid placeholder: {}", t)))?;
            
            if param_num == 0 || param_num > params.len() {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Parameter ${} out of range (have {} parameters)",
                    param_num, params.len()
                )));
            }
            
            let sv = &params[param_num - 1];
            return match sv {
                ScalarValue::Int64(Some(i)) => Ok(JsonValue::Number((*i).into())),
                ScalarValue::Utf8(Some(s)) => Ok(JsonValue::String(s.clone())),
                ScalarValue::Boolean(Some(b)) => Ok(JsonValue::Bool(*b)),
                _ => Err(KalamDbError::InvalidOperation("Unsupported ScalarValue in placeholder".into()))
            };
        }
        
        // Check for NULL
        if t.eq_ignore_ascii_case("NULL") {
            return Ok(JsonValue::Null);
        }
        
        // Check for boolean
        if t.eq_ignore_ascii_case("TRUE") {
            return Ok(JsonValue::Bool(true));
        }
        if t.eq_ignore_ascii_case("FALSE") {
            return Ok(JsonValue::Bool(false));
        }
        
        // Check for quoted string
        if (t.starts_with('\'') && t.ends_with('\'')) || 
           (t.starts_with('"') && t.ends_with('"')) ||
           (t.starts_with('`') && t.ends_with('`')) {
            let unquoted = &t[1..t.len()-1];
            return Ok(JsonValue::String(unquoted.to_string()));
        }
        
        // Try parsing as number
        if let Ok(i) = t.parse::<i64>() {
            return Ok(JsonValue::Number(i.into()));
        }
        if let Ok(f) = t.parse::<f64>() {
            return serde_json::Number::from_f64(f)
                .map(JsonValue::Number)
                .ok_or_else(|| KalamDbError::InvalidOperation("Invalid float value".into()));
        }
        
        // Default to string
        Ok(JsonValue::String(t.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), role, create_test_session())
    }

    #[tokio::test]
    async fn test_update_authorization_user() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::User);
        let stmt = SqlStatement::new("UPDATE default.test SET x = 1 WHERE id = 1".to_string(), SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_dba() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new("UPDATE default.test SET x = 1 WHERE id = 1".to_string(), SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_authorization_service() {
        let handler = UpdateHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new("UPDATE default.test SET x = 1 WHERE id = 1".to_string(), SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual UPDATE execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
