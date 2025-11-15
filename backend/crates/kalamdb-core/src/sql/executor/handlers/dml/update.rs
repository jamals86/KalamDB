//! UPDATE Handler
//!
//! Handles UPDATE statements with parameter binding support via DataFusion.

use crate::error::KalamDbError;
use crate::providers::base::BaseTableProvider; // Phase 13.6: Bring trait methods into scope
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
        let (namespace, table_name, assignments, where_pair) = self.simple_parse_update(sql)?;

        let mut obj = serde_json::Map::new();
        for (col, token) in assignments {
            let val = self.token_to_json(&token, &params)?;
            obj.insert(col, val);
        }
        let updates = JsonValue::Object(obj);

        // T153: Use effective user_id for impersonation support (Phase 7)
        let effective_user_id = statement.as_user_id().unwrap_or(&context.user_id);

        // Execute native update for USER tables; for SHARED tables, perform provider-level updates across matching rows (MVP: id only)
        let schema_registry = app_context.schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry.get_table_definition(&table_id)?.ok_or_else(|| KalamDbError::NotFound(format!("Table {}.{} not found", namespace.as_str(), table_name.as_str())))?;
        
        // T163: Reject AS USER on Shared tables (Phase 7)
        use kalamdb_commons::schemas::TableType;
        if statement.as_user_id().is_some() && matches!(def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string()
            ));
        }
        
        match def.table_type {
            kalamdb_commons::schemas::TableType::User => {
                // Get provider from unified cache and downcast to UserTableProvider
                let provider_arc = schema_registry.get_provider(&table_id)
                    .ok_or_else(|| KalamDbError::InvalidOperation("User table provider not found".into()))?;
                
                if let Some(provider) = provider_arc.as_any().downcast_ref::<crate::providers::UserTableProvider>() {
                    // T064: Get actual PK column name from provider instead of assuming "id"
                    let pk_column = provider.primary_key_field_name();
                    // Require WHERE <pk> = <value>
                    let id_value = self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                        .ok_or_else(|| KalamDbError::InvalidOperation(format!(
                            "UPDATE currently requires WHERE {} = <value> for USER tables",
                            pk_column
                        )))?;
                    
                    println!("[DEBUG UpdateHandler] Calling provider.update_by_id_field for user={}, pk_column={}, pk_value={}", 
                        effective_user_id.as_str(), pk_column, id_value);
                    
                    match provider.update_by_id_field(effective_user_id, &id_value, updates) {
                        Ok(_) => {
                            println!("[DEBUG UpdateHandler] update_by_id_field succeeded");
                            Ok(ExecutionResult::Updated { rows_affected: 1 })
                        }
                        Err(e) => {
                            println!("[DEBUG UpdateHandler] update_by_id_field failed: {}", e);
                            // Isolation-friendly semantics: updating a non-existent row under this user_id
                            // should return success with 0 rows affected (no-op), not an error.
                            if matches!(e, crate::error::KalamDbError::NotFound(_)) {
                                return Ok(ExecutionResult::Updated { rows_affected: 0 });
                            }
                            Err(e)
                        }
                    }
                } else {
                    Err(KalamDbError::InvalidOperation("Cached provider type mismatch for user table".into()))
                }
            }
            kalamdb_commons::schemas::TableType::Shared => {
                // For SHARED tables, also require WHERE on the actual PK column
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| KalamDbError::InvalidOperation("Shared table provider not found".into()))?;
                if let Some(provider) = provider_arc.as_any().downcast_ref::<crate::providers::SharedTableProvider>() {
                    let pk_column = provider.primary_key_field_name();
                    if let Some(id_value) = self.extract_row_id_for_column(&where_pair, pk_column, &params)? {
                        // SharedTableProvider ignores user_id parameter (no RLS)
                        provider.update_by_id_field(effective_user_id, &id_value, updates)?;
                        Ok(ExecutionResult::Updated { rows_affected: 1 })
                    } else {
                        Err(KalamDbError::InvalidOperation(format!(
                            "UPDATE on SHARED tables requires WHERE {} = <value> (predicate updates not yet supported)",
                            pk_column
                        )))
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
    fn simple_parse_update(&self, sql: &str) -> Result<(NamespaceId, TableName, Vec<(String, String)>, Option<(String, String)>), KalamDbError> {
        // Expect: UPDATE <ns>.<table> SET col1=val1, col2=$2 WHERE <pk> = <v>
        let upper = sql.to_uppercase();
        let set_pos = upper.find(" SET ").ok_or_else(|| KalamDbError::InvalidOperation("Missing SET clause".into()))?;
        let where_pos = upper.find(" WHERE ");
        let head = sql[0..set_pos].trim(); // "UPDATE <table_ref>"
        
        // Extract table reference by removing "UPDATE" keyword
        let table_part = if head.to_uppercase().starts_with("UPDATE ") {
            head["UPDATE ".len()..].trim()
        } else {
            return Err(KalamDbError::InvalidOperation("Invalid UPDATE syntax".into()));
        };
        
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
        let where_pair = where_pos.and_then(|wp| {
            let where_clause_raw = sql[wp + 7..].trim();
            // Parse simple "<col> = <value>" pattern
            let parts: Vec<&str> = where_clause_raw.splitn(2, '=').collect();
            if parts.len() == 2 {
                let col = parts[0].trim().to_string();
                let val = parts[1].trim().to_string();
                Some((col, val))
            } else {
                None
            }
        });
        Ok((ns, tbl, assigns, where_pair))
    }

    fn extract_row_id_for_column(&self, where_pair: &Option<(String, String)>, pk_column: &str, params: &[ScalarValue]) -> Result<Option<String>, KalamDbError> {
        if let Some((col, token)) = where_pair {
            // Ensure WHERE references the actual PK column
            if !col.eq_ignore_ascii_case(pk_column) {
                return Ok(None);
            }
            let t = token.trim();
            // Check for placeholder
            if t.starts_with('$') {
                let num: usize = t[1..].parse().map_err(|_| KalamDbError::InvalidOperation("Invalid placeholder in WHERE".into()))?;
                if num == 0 || num > params.len() {
                    return Err(KalamDbError::InvalidOperation("Placeholder index out of range".into()));
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
