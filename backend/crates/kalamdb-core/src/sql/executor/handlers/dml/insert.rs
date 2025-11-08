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
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::{NamespaceId, TableName};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use serde_json::Value as JsonValue;

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
        _session: &SessionContext,
        statement: SqlStatement,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Ensure correct variant
        if !matches!(statement.kind(), SqlStatementKind::Insert(_)) {
            return Err(KalamDbError::InvalidOperation("InsertHandler received wrong statement kind".into()));
        }

        let sql = statement.as_str();
        // Very lightweight INSERT parser (avoids tight coupling to sqlparser internals)
        let (namespace, table_name, columns, rows_tokens) = self.simple_parse_insert(sql)?;
        // Validate table exists via SchemaRegistry fast path (using TableId)
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let schema_registry = AppContext::get().schema_registry();
        let exists = schema_registry.table_exists(&table_id)?;
        if !exists {
            return Err(KalamDbError::InvalidOperation(format!("Table '{}.{}' does not exist", namespace.as_str(), table_name.as_str())));
        }

        // Bind parameters and construct JSON rows
        let mut json_rows: Vec<JsonValue> = Vec::new();
        for row_tokens in rows_tokens {
            let mut obj = serde_json::Map::new();
            if columns.is_empty() {
                // Positional mapping: column order from schema will be used later; here just store as v1,v2...
                // For simplicity require explicit column list for MVP
                return Err(KalamDbError::InvalidOperation("INSERT without explicit column list not supported yet".into()));
            }
            if row_tokens.len() != columns.len() {
                return Err(KalamDbError::InvalidOperation(format!("VALUES column count mismatch: expected {} got {}", columns.len(), row_tokens.len())));
            }
            for (col, token) in columns.iter().zip(row_tokens.iter()) {
                let value = self.token_to_json(token, &params)?;
                obj.insert(col.clone(), value);
            }
            json_rows.push(JsonValue::Object(obj));
        }

        // Execute native insert
        let rows_affected = self.execute_native_insert(&namespace, &table_name, &context.user_id, json_rows).await?;
        Ok(ExecutionResult::Inserted { rows_affected })
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !matches!(statement.kind(), SqlStatementKind::Insert(_)) {
            return Err(KalamDbError::InvalidOperation("InsertHandler received wrong statement kind".into()));
        }
        use kalamdb_commons::Role;
        match context.user_role {
            Role::System | Role::Dba | Role::Service | Role::User => Ok(()),
            _ => Err(KalamDbError::PermissionDenied("INSERT requires User role or higher".to_string())),
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

    /// Convert token string to JSON value, binding parameters as needed
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
            
            return self.scalar_value_to_json(&params[param_num - 1]);
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

impl InsertHandler {
    fn simple_parse_insert(&self, sql: &str) -> Result<(NamespaceId, TableName, Vec<String>, Vec<Vec<String>>), KalamDbError> {
        // Expect pattern: INSERT INTO <ns>.<table> (<cols>) VALUES (...),(...)
        let upper = sql.to_uppercase();
        let into_pos = upper.find("INSERT INTO").ok_or_else(|| KalamDbError::InvalidOperation("Missing 'INSERT INTO'".into()))?;
        let values_pos = upper.find(" VALUES ").ok_or_else(|| KalamDbError::InvalidOperation("Missing 'VALUES' clause".into()))?;
        let head = sql[into_pos + 11..values_pos].trim();
        // Split head into table and optional column list
        let (table_part, cols_part) = if let Some(lp) = head.find('(') {
            let rp = head.rfind(')').ok_or_else(|| KalamDbError::InvalidOperation("Malformed column list".into()))?;
            (head[..lp].trim(), Some(&head[lp + 1..rp]))
        } else { (head, None) };

        let (namespace, table_name) = {
            let parts: Vec<&str> = table_part.split('.').collect();
            match parts.len() { 
                1 => (NamespaceId::new("default"), TableName::new(parts[0].trim().to_string())), 
                2 => (NamespaceId::new(parts[0].trim().to_string()), TableName::new(parts[1].trim().to_string())), 
                _ => return Err(KalamDbError::InvalidOperation("Invalid table reference".into())) 
            }
        };

        let columns: Vec<String> = match cols_part {
            Some(cols) => cols.split(',').map(|s| s.trim().trim_matches('"').trim_matches('`').to_string()).collect(),
            None => Vec::new(),
        };

        // Parse VALUES rows as string tokens (we'll bind params later)
        let values_str = &sql[values_pos + 8..];
        let mut rows_tokens: Vec<Vec<String>> = Vec::new();
        for row_str in values_str.split(')').filter(|s| !s.trim().is_empty()) {
            let row_str = row_str.trim().trim_start_matches(',').trim().trim_start_matches('(').trim();
            if row_str.is_empty() { continue; }
            let row_tokens: Vec<String> = row_str.split(',').map(|s| s.trim().to_string()).collect();
            rows_tokens.push(row_tokens);
        }

        Ok((namespace, table_name, columns, rows_tokens))
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
        let stmt = SqlStatement::new("INSERT INTO default.test (id) VALUES (1)".to_string(), SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_dba() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new("INSERT INTO default.test (id) VALUES (1)".to_string(), SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_insert_authorization_service() {
        let handler = InsertHandler::new();
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new("INSERT INTO default.test (id) VALUES (1)".to_string(), SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement));
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual INSERT execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
