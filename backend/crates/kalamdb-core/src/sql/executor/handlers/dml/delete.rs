//! DELETE Handler
//!
//! Handles DELETE statements with parameter binding support via DataFusion.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::providers::base::BaseTableProvider; // Phase 13.6: Bring trait methods into scope
use crate::sql::executor::handlers::dml::mod_helpers::extract_pk_from_where_pair;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use crate::sql::executor::parameter_validation::{validate_parameters, ParameterLimits};
use async_trait::async_trait;
use kalamdb_commons::models::{NamespaceId, TableId, TableName, UserId};
use kalamdb_raft::{DataResponse, SharedDataCommand, UserDataCommand};
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};

/// Handler for DELETE statements
///
/// Delegates to DataFusion for DELETE execution with parameter binding support.
/// Returns rows_affected count following MySQL semantics.
pub struct DeleteHandler {
    app_context: std::sync::Arc<AppContext>,
}

impl DeleteHandler {
    pub fn new(app_context: std::sync::Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait]
impl StatementHandler for DeleteHandler {
    async fn execute(
        &self,
        statement: SqlStatement,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // T064: Validate parameters before write using config from AppContext
        let limits = ParameterLimits::from_config(&self.app_context.config().execution);
        validate_parameters(&params, &limits)?;

        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation(
                "DeleteHandler received wrong statement kind".into(),
            ));
        }

        let sql = statement.as_str();
        // Pass the current default namespace from session context for unqualified table names
        let default_namespace = context.default_namespace();
        let (namespace, table_name, where_pair, has_where_clause) =
            self.simple_parse_delete(sql, &default_namespace)?;

        // T153: Use effective user_id for impersonation support (Phase 7)
        let effective_user_id = statement.as_user_id().unwrap_or(&context.user_id);

        // Execute native delete
        let schema_registry = self.app_context.schema_registry();
        use kalamdb_commons::models::TableId;
        let table_id = TableId::new(namespace.clone(), table_name.clone());
        let def = schema_registry
            .get_table_if_exists(self.app_context.as_ref(), &table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {}.{} not found",
                    namespace.as_str(),
                    table_name.as_str()
                ))
            })?;

        // T163: Reject AS USER on Shared tables (Phase 7)
        use kalamdb_commons::schemas::TableType;
        if statement.as_user_id().is_some() && matches!(def.table_type, TableType::Shared) {
            return Err(KalamDbError::InvalidOperation(
                "AS USER impersonation is not supported for SHARED tables".to_string(),
            ));
        }

        match def.table_type {
            TableType::User => {
                // Get provider from unified cache and downcast to UserTableProvider
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("User table provider not found".into())
                })?;

                if let Some(provider) =
                    provider_arc.as_any().downcast_ref::<crate::providers::UserTableProvider>()
                {
                    let pk_column = provider.primary_key_field_name();

                    // Try to extract simple WHERE pk = value first (fast path)
                    let pks_to_delete = if let Some(row_id) =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                    {
                        vec![row_id]
                    } else if has_where_clause {
                        self.collect_pks_with_datafusion(
                            sql,
                            &table_id,
                            effective_user_id,
                            provider_arc.clone(),
                            context,
                        )
                        .await?
                    } else {
                        return Err(KalamDbError::InvalidOperation(
                            "DELETE requires a WHERE clause".to_string(),
                        ));
                    };

                    if pks_to_delete.is_empty() {
                        return Ok(ExecutionResult::Deleted { rows_affected: 0 });
                    }

                    // Phase 20: Always route through Raft (single-node or cluster)
                    let rows_affected = self
                        .execute_user_delete_via_raft(&table_id, effective_user_id, pks_to_delete)
                        .await?;
                    Ok(ExecutionResult::Deleted { rows_affected })
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for user table".into(),
                    ))
                }
            },
            TableType::Shared => {
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

                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("Shared table provider not found".into())
                })?;

                if let Some(provider) =
                    provider_arc.as_any().downcast_ref::<crate::providers::SharedTableProvider>()
                {
                    let pk_column = provider.primary_key_field_name();

                    // Collect PKs to delete
                    let pks_to_delete: Vec<String> = if let Some(row_id) =
                        self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                    {
                        // Single PK from simple WHERE clause
                        vec![row_id]
                    } else if has_where_clause {
                        // Complex WHERE clause - use DataFusion to find matching PKs
                        self.collect_pks_with_datafusion(
                            sql,
                            &table_id,
                            effective_user_id,
                            provider_arc.clone(),
                            context,
                        )
                        .await?
                    } else {
                        return Err(KalamDbError::InvalidOperation(
                            "DELETE requires a WHERE clause".to_string(),
                        ));
                    };

                    if pks_to_delete.is_empty() {
                        return Ok(ExecutionResult::Deleted { rows_affected: 0 });
                    }

                    // Phase 20: Always route through Raft (single-node or cluster)
                    let rows_affected =
                        self.execute_delete_via_raft(&table_id, pks_to_delete).await?;
                    Ok(ExecutionResult::Deleted { rows_affected })
                } else {
                    Err(KalamDbError::InvalidOperation(
                        "Cached provider type mismatch for shared table".into(),
                    ))
                }
            },
            TableType::Stream => {
                // STREAM tables support DELETE for hard deletion
                let provider_arc = schema_registry.get_provider(&table_id).ok_or_else(|| {
                    KalamDbError::InvalidOperation("Stream table provider not found".into())
                })?;

                let provider = provider_arc
                    .as_any()
                    .downcast_ref::<crate::providers::StreamTableProvider>()
                    .ok_or_else(|| {
                        KalamDbError::InvalidOperation(
                            "Cached provider type mismatch for stream table".into(),
                        )
                    })?;

                if !has_where_clause {
                    return Err(KalamDbError::InvalidOperation(
                        "DELETE on STREAM tables requires a WHERE clause".to_string(),
                    ));
                }

                let pk_column = provider.primary_key_field_name();
                let pks_to_delete = if let Some(row_id) =
                    self.extract_row_id_for_column(&where_pair, pk_column, &params)?
                {
                    vec![row_id]
                } else {
                    self.collect_pks_with_datafusion(
                        sql,
                        &table_id,
                        effective_user_id,
                        provider_arc.clone(),
                        context,
                    )
                    .await?
                };

                if pks_to_delete.is_empty() {
                    return Ok(ExecutionResult::Deleted { rows_affected: 0 });
                }

                // Phase 20: Always route through Raft (single-node or cluster)
                let rows_affected = self
                    .execute_user_delete_via_raft(&table_id, effective_user_id, pks_to_delete)
                    .await?;
                Ok(ExecutionResult::Deleted { rows_affected })
            },
            TableType::System => {
                Err(KalamDbError::InvalidOperation("Cannot DELETE from SYSTEM tables".into()))
            },
        }
    }

    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use crate::sql::executor::helpers::guards::block_anonymous_write;

        if !matches!(statement.kind(), SqlStatementKind::Delete(_)) {
            return Err(KalamDbError::InvalidOperation(
                "DeleteHandler received wrong statement kind".into(),
            ));
        }

        // T050: Block anonymous users from write operations
        block_anonymous_write(context, "DELETE")?;

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

impl DeleteHandler {
    /// Extract PK value from Arrow array
    fn extract_pk_value_from_array(
        &self,
        array: &std::sync::Arc<dyn arrow::array::Array>,
        row_idx: usize,
    ) -> Result<String, KalamDbError> {
        kalamdb_commons::arrow_utils::array_value_to_string(array.as_ref(), row_idx).ok_or_else(
            || {
                KalamDbError::InvalidOperation(format!(
                    "Unsupported PK type for deletion: {:?}",
                    array.data_type()
                ))
            },
        )
    }

    /// Simple DELETE parser for basic DELETE statements
    ///
    /// # Arguments
    /// * `sql` - The SQL DELETE statement
    /// * `default_namespace` - The default namespace to use for unqualified table names
    #[allow(clippy::type_complexity)]
    fn simple_parse_delete(
        &self,
        sql: &str,
        default_namespace: &NamespaceId,
    ) -> Result<(NamespaceId, TableName, Option<(String, String)>, bool), KalamDbError> {
        // Expect: DELETE FROM <ns>.<table> WHERE <col> = <value>
        let upper = sql.to_uppercase();
        let from_pos = upper
            .find("DELETE FROM ")
            .ok_or_else(|| KalamDbError::InvalidOperation("Missing 'DELETE FROM'".into()))?;
        let where_pos = upper.find(" WHERE ");
        let head = if let Some(wp) = where_pos {
            &sql[from_pos + 12..wp]
        } else {
            &sql[from_pos + 12..]
        };
        let (ns, tbl) = {
            let mut parts = head.trim().splitn(2, '.');
            let first = parts.next().map(str::trim);
            let second = parts.next().map(str::trim);
            match (first, second) {
                (Some(table), None) => {
                    (default_namespace.clone(), TableName::new(table.to_string()))
                },
                (Some(namespace), Some(table)) => {
                    (NamespaceId::new(namespace.to_string()), TableName::new(table.to_string()))
                },
                _ => return Err(KalamDbError::InvalidOperation("Invalid table reference".into())),
            }
        };
        let (where_pair, has_where_clause) = if let Some(wp) = where_pos {
            let cond = sql[wp + 7..].trim();
            // Support '<col> = <literal>' - handle both numeric and quoted string values
            let mut parts = cond.splitn(2, '=');
            let col = parts.next().map(str::trim);
            let val = parts.next().map(str::trim);
            match (col, val) {
                (Some(col), Some(val)) => (Some((col.to_string(), val.to_string())), true),
                _ => (None, true),
            }
        } else {
            (None, false)
        };
        Ok((ns, tbl, where_pair, has_where_clause))
    }

    fn extract_row_id_for_column(
        &self,
        where_pair: &Option<(String, String)>,
        pk_column: &str,
        params: &[ScalarValue],
    ) -> Result<Option<String>, KalamDbError> {
        extract_pk_from_where_pair(where_pair, pk_column, params)
    }

    /// Collect primary keys to delete using DataFusion to evaluate complex WHERE clauses
    async fn collect_pks_with_datafusion(
        &self,
        sql: &str,
        table_id: &TableId,
        _user_id: &kalamdb_commons::models::UserId,
        provider: std::sync::Arc<dyn datafusion::datasource::TableProvider>,
        context: &ExecutionContext,
    ) -> Result<Vec<String>, KalamDbError> {
        // Convert DELETE to SELECT to find matching PKs
        let select_sql = sql.replace("DELETE FROM", "SELECT * FROM");

        let df_ctx = context.create_session_with_user();
        let table_name = table_id.full_name();

        match df_ctx.register_table(&table_name, provider.clone()) {
            Ok(_) => {},
            Err(e) => {
                let msg = e.to_string();
                if !msg.to_lowercase().contains("already exists")
                    && !msg.to_lowercase().contains("exists")
                {
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Failed to register table: {}",
                        e
                    )));
                }
            },
        }

        let df = df_ctx.sql(&select_sql).await?;
        let batches = df.collect().await?;

        if batches.is_empty() {
            return Ok(vec![]);
        }

        // Get PK column name from provider
        let pk_column = if let Some(shared_provider) =
            provider.as_any().downcast_ref::<crate::providers::SharedTableProvider>()
        {
            shared_provider.primary_key_field_name()
        } else if let Some(user_provider) =
            provider.as_any().downcast_ref::<crate::providers::UserTableProvider>()
        {
            user_provider.primary_key_field_name()
        } else if let Some(stream_provider) =
            provider.as_any().downcast_ref::<crate::providers::StreamTableProvider>()
        {
            stream_provider.primary_key_field_name()
        } else {
            return Err(KalamDbError::InvalidOperation("Unknown provider type".into()));
        };

        let mut pks = Vec::new();
        for batch in batches {
            if batch.num_rows() == 0 {
                continue;
            }

            let schema = batch.schema();
            let pk_idx =
                schema.fields().iter().position(|f| f.name() == pk_column).ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "PK column '{}' not found in result",
                        pk_column
                    ))
                })?;

            let pk_array = batch.column(pk_idx);

            for row_idx in 0..batch.num_rows() {
                let pk_value = self.extract_pk_value_from_array(pk_array, row_idx)?;
                pks.push(pk_value);
            }
        }

        Ok(pks)
    }

    /// Execute DELETE via Raft consensus for cluster replication
    async fn execute_delete_via_raft(
        &self,
        table_id: &TableId,
        pk_values: Vec<String>,
    ) -> Result<usize, KalamDbError> {
        let executor = self.app_context.executor();
        let pk_count = pk_values.len();

        // Serialize the list of PKs to delete
        let cmd = SharedDataCommand::Delete {
            required_meta_index: 0, // Stamped by executor
            table_id: table_id.clone(),
            pk_values: Some(pk_values),
        };

        let response = executor
            .execute_shared_data(cmd)
            .await
            .map_err(|e| KalamDbError::InvalidOperation(format!("Raft delete failed: {}", e)))?;

        match response {
            DataResponse::RowsAffected(n) => Ok(n),
            DataResponse::Ok => Ok(pk_count),
            DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
            _ => Ok(pk_count),
        }
    }

    /// Execute DELETE via Raft consensus for user tables
    async fn execute_user_delete_via_raft(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        pk_values: Vec<String>,
    ) -> Result<usize, KalamDbError> {
        let executor = self.app_context.executor();
        let pk_count = pk_values.len();

        let cmd = UserDataCommand::Delete {
            required_meta_index: 0, // Stamped by executor
            table_id: table_id.clone(),
            user_id: user_id.clone(),
            pk_values: Some(pk_values),
        };

        let response = executor.execute_user_data(user_id, cmd).await.map_err(|e| {
            KalamDbError::InvalidOperation(format!("Raft user delete failed: {}", e))
        })?;

        match response {
            DataResponse::RowsAffected(n) => Ok(n),
            DataResponse::Ok => Ok(pk_count),
            DataResponse::Error { message } => Err(KalamDbError::InvalidOperation(message)),
            _ => Ok(pk_count),
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
    async fn test_delete_authorization_user() {
        let handler = DeleteHandler::new(init_app_context());
        let ctx = test_context(Role::User);
        let stmt = SqlStatement::new(
            "DELETE FROM default.test WHERE id = 1".to_string(),
            SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_dba() {
        let handler = DeleteHandler::new(init_app_context());
        let ctx = test_context(Role::Dba);
        let stmt = SqlStatement::new(
            "DELETE FROM default.test WHERE id = 1".to_string(),
            SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_authorization_service() {
        let handler = DeleteHandler::new(init_app_context());
        let ctx = test_context(Role::Service);
        let stmt = SqlStatement::new(
            "DELETE FROM default.test WHERE id = 1".to_string(),
            SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
        );
        let result = handler.check_authorization(&stmt, &ctx).await;
        assert!(result.is_ok());
    }

    // Note: TypedStatementHandler pattern doesn't require wrong-type checking -
    // type safety is enforced at compile time by the type parameter.
    //
    // Actual DELETE execution tests require table creation and SQL text parsing,
    // which are better suited for integration tests in Phase 7.
}
