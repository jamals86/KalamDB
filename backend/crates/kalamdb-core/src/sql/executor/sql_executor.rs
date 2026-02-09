use super::SqlExecutor;
use crate::error::KalamDbError;
use crate::sql::executor::helpers::guards::block_system_namespace_modification;
use crate::sql::plan_cache::PlanCacheKey;
use crate::sql::{ExecutionContext, ExecutionMetadata, ExecutionResult};
use arrow::array::RecordBatch;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::conversions::arrow_json_conversion::arrow_value_to_scalar;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use std::time::Duration;
use tracing::Instrument;

#[derive(Debug, Clone, Copy)]
enum DmlKind {
    Insert,
    Update,
    Delete,
}

impl SqlExecutor {
    fn dml_operation_name(dml_kind: DmlKind) -> &'static str {
        match dml_kind {
            DmlKind::Insert => "INSERT",
            DmlKind::Update => "UPDATE",
            DmlKind::Delete => "DELETE",
        }
    }

    fn block_system_namespace_dml(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        dml_kind: DmlKind,
    ) -> Result<(), KalamDbError> {
        let Some(table_id) =
            kalamdb_sql::extract_dml_table_id(sql, exec_ctx.default_namespace().as_str())
        else {
            return Ok(());
        };

        block_system_namespace_modification(
            table_id.namespace_id(),
            Self::dml_operation_name(dml_kind),
            "TABLE",
            Some(table_id.table_name().as_str()),
        )
    }

    fn is_table_not_found_error(e: &datafusion::error::DataFusionError) -> bool {
        let msg = e.to_string().to_lowercase();
        (msg.contains("table") && msg.contains("not found"))
            || (msg.contains("relation") && msg.contains("does not exist"))
            || msg.contains("unknown table")
    }

    /// Construct a new executor hooked into the shared `AppContext`.
    pub fn new(
        app_context: std::sync::Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        let handler_registry =
            std::sync::Arc::new(crate::sql::executor::handler_registry::HandlerRegistry::new(
                app_context.clone(),
                enforce_password_complexity,
            ));
        let plan_cache = std::sync::Arc::new(crate::sql::plan_cache::PlanCache::with_config(
            app_context.config().execution.sql_plan_cache_max_entries,
            Duration::from_secs(app_context.config().execution.sql_plan_cache_ttl_seconds),
        ));
        Self {
            app_context,
            handler_registry,
            plan_cache,
        }
    }

    /// Clear the plan cache (e.g., after DDL operations)
    pub fn clear_plan_cache(&self) {
        self.plan_cache.clear();
    }

    /// Get current plan cache size (diagnostics/testing)
    pub fn plan_cache_len(&self) -> usize {
        self.plan_cache.len()
    }

    /// Execute a statement without request metadata.
    pub async fn execute(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(sql, exec_ctx, None, params).await
    }

    /// Execute a statement with optional metadata.
    pub async fn execute_with_metadata(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        _metadata: Option<&ExecutionMetadata>,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        let span = tracing::info_span!(
            "sql.execute",
            user_id = %exec_ctx.user_id().as_str(),
            namespace = %exec_ctx.default_namespace().as_str(),
            command = tracing::field::Empty,
            rows = tracing::field::Empty,
        );
        // Enter the span for the entire execution
        async {
        // Step 0: Check SQL query length to prevent DoS attacks
        // Most legitimate queries are under 10KB, we allow up to 1MB
        if sql.len() > kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH {
            log::warn!(
                "❌ SQL query rejected: length {} bytes exceeds maximum {} bytes",
                sql.len(),
                kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH
            );
            return Err(KalamDbError::InvalidSql(format!(
                "SQL query too long: {} bytes (maximum {} bytes)",
                sql.len(),
                kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH
            )));
        }

        // Step 1: Classify, authorize, and parse statement in one pass
        // Prioritize SELECT/DML checks as they represent 99% of queries
        // Authorization happens before parsing for fail-fast behavior
        let classified = SqlStatement::classify_and_parse(
            sql,
            &exec_ctx.default_namespace(),
            exec_ctx.user_role(),
        )
        .map_err(|e| match e {
            kalamdb_sql::classifier::StatementClassificationError::Unauthorized(msg) => {
                KalamDbError::Unauthorized(msg)
            },
            kalamdb_sql::classifier::StatementClassificationError::InvalidSql {
                sql: _,
                message,
            } => KalamDbError::InvalidSql(message),
        })?;

        // Record the command kind in the span
        let command_label = format!("{:?}", classified.kind());
        tracing::Span::current().record("command", &command_label.as_str());

        // Step 2: Route based on statement type
        let result = match classified.kind() {
            // Hot path: SELECT queries use DataFusion
            // Tables are already registered in base session, we just inject user_id
            SqlStatementKind::Select => {
                self.execute_via_datafusion(classified.as_str(), params, exec_ctx).await
            },

            // DataFusion meta commands (EXPLAIN, SET, SHOW, etc.) - admin only
            // No caching needed - these are diagnostic/config commands
            // Authorization already checked in classifier
            SqlStatementKind::DataFusionMetaCommand => {
                self.execute_meta_command(sql, exec_ctx).await
            },

            // Native DataFusion DML path (provider insert/update/delete hooks)
            SqlStatementKind::Insert(_) => {
                self.execute_dml_via_datafusion(
                    classified.as_str(),
                    params,
                    exec_ctx,
                    DmlKind::Insert,
                )
                .await
            },
            SqlStatementKind::Update(_) => {
                self.execute_dml_via_datafusion(
                    classified.as_str(),
                    params,
                    exec_ctx,
                    DmlKind::Update,
                )
                .await
            },
            SqlStatementKind::Delete(_) => {
                self.execute_dml_via_datafusion(
                    classified.as_str(),
                    params,
                    exec_ctx,
                    DmlKind::Delete,
                )
                .await
            },

            // DDL operations that modify table/view structure require plan cache invalidation
            // This prevents stale cached plans from referencing dropped/altered tables
            SqlStatementKind::CreateTable(_)
            | SqlStatementKind::DropTable(_)
            | SqlStatementKind::AlterTable(_)
            | SqlStatementKind::CreateView(_)
            | SqlStatementKind::CreateNamespace(_)
            | SqlStatementKind::DropNamespace(_) => {
                let result = self.handler_registry.handle(classified, params, exec_ctx).await;
                // Clear plan cache after DDL to invalidate any cached plans
                // that may reference the modified schema
                if result.is_ok() {
                    self.plan_cache.clear();
                    log::debug!("Plan cache cleared after DDL operation");
                }
                result
            },

            // All other statements: Delegate to handler registry (no cache invalidation needed)
            _ => self.handler_registry.handle(classified, params, exec_ctx).await,
        };

        // Record row count in the span
        if let Ok(ref res) = result {
            let rows = match res {
                ExecutionResult::Rows { row_count, .. } => *row_count,
                ExecutionResult::Inserted { rows_affected } => *rows_affected,
                ExecutionResult::Updated { rows_affected } => *rows_affected,
                ExecutionResult::Deleted { rows_affected } => *rows_affected,
                _ => 0,
            };
            tracing::Span::current().record("rows", rows);
        }

        result
        }.instrument(span).await
    }

    #[tracing::instrument(
        name = "sql.dml_datafusion",
        skip_all,
        fields(
            dml_kind = %Self::dml_operation_name(dml_kind),
            rows_affected = tracing::field::Empty,
        )
    )]
    async fn execute_dml_via_datafusion(
        &self,
        sql: &str,
        params: Vec<ScalarValue>,
        exec_ctx: &ExecutionContext,
        dml_kind: DmlKind,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.block_system_namespace_dml(sql, exec_ctx, dml_kind)?;

        use crate::sql::executor::parameter_binding::{
            replace_placeholders_in_plan, validate_params,
        };

        if !params.is_empty() {
            validate_params(&params)?;
        }

        // Parameterized DML: reuse cached template plans and only bind placeholders per request.
        // This avoids reparsing/replanning the same INSERT/UPDATE/DELETE shape repeatedly.
        let df = if params.is_empty() {
            let session = exec_ctx.create_session_with_user();
            match session.sql(sql).await {
                Ok(df) => df,
                Err(e) => {
                    if Self::is_table_not_found_error(&e) {
                        if let Err(load_err) = self.load_existing_tables().await {
                            log::warn!(
                                target: "sql::dml",
                                "⚠️  Failed to reload table providers after missing table in DML | sql='{}' | error='{}'",
                                sql,
                                load_err
                            );
                        }
                        let retry_session = exec_ctx.create_session_with_user();
                        retry_session
                            .sql(sql)
                            .await
                            .map_err(|e2| self.log_sql_error(sql, exec_ctx, e2))?
                    } else {
                        return Err(self.log_sql_error(sql, exec_ctx, e));
                    }
                },
            }
        } else {
            let cache_key =
                PlanCacheKey::new(exec_ctx.default_namespace().clone(), exec_ctx.user_role(), sql);
            let session = exec_ctx.create_session_with_user();

            if let Some(template_plan) = self.plan_cache.get(&cache_key) {
                let bound_plan = replace_placeholders_in_plan((*template_plan).clone(), &params)?;
                match session.execute_logical_plan(bound_plan).await {
                    Ok(df) => df,
                    Err(e) => {
                        log::warn!(
                            target: "sql::dml",
                            "Failed to execute cached DML plan, reparsing SQL: {}",
                            e
                        );

                        match session.sql(sql).await {
                            Ok(planned_df) => {
                                let template_plan = planned_df.logical_plan().clone();
                                self.plan_cache.insert(cache_key.clone(), template_plan.clone());
                                let rebound_plan =
                                    replace_placeholders_in_plan(template_plan, &params)?;
                                session
                                    .execute_logical_plan(rebound_plan)
                                    .await
                                    .map_err(|e2| KalamDbError::ExecutionError(e2.to_string()))?
                            },
                            Err(e) => {
                                if Self::is_table_not_found_error(&e) {
                                    if let Err(load_err) = self.load_existing_tables().await {
                                        log::warn!(
                                            target: "sql::dml",
                                            "⚠️  Failed to reload table providers after missing table in DML | sql='{}' | error='{}'",
                                            sql,
                                            load_err
                                        );
                                    }
                                    let retry_session = exec_ctx.create_session_with_user();
                                    let retry_df = retry_session
                                        .sql(sql)
                                        .await
                                        .map_err(|e2| self.log_sql_error(sql, exec_ctx, e2))?;
                                    let template_plan = retry_df.logical_plan().clone();
                                    self.plan_cache.insert(cache_key.clone(), template_plan.clone());
                                    let rebound_plan =
                                        replace_placeholders_in_plan(template_plan, &params)?;
                                    retry_session
                                        .execute_logical_plan(rebound_plan)
                                        .await
                                        .map_err(|e3| {
                                            KalamDbError::ExecutionError(e3.to_string())
                                        })?
                                } else {
                                    return Err(self.log_sql_error(sql, exec_ctx, e));
                                }
                            },
                        }
                    },
                }
            } else {
                match session.sql(sql).await {
                    Ok(planned_df) => {
                        let template_plan = planned_df.logical_plan().clone();
                        self.plan_cache.insert(cache_key.clone(), template_plan.clone());
                        let bound_plan = replace_placeholders_in_plan(template_plan, &params)?;
                        session
                            .execute_logical_plan(bound_plan)
                            .await
                            .map_err(|e2| KalamDbError::ExecutionError(e2.to_string()))?
                    },
                    Err(e) => {
                        if Self::is_table_not_found_error(&e) {
                            if let Err(load_err) = self.load_existing_tables().await {
                                log::warn!(
                                    target: "sql::dml",
                                    "⚠️  Failed to reload table providers after missing table in DML | sql='{}' | error='{}'",
                                    sql,
                                    load_err
                                );
                            }
                            let retry_session = exec_ctx.create_session_with_user();
                            let retry_df = retry_session
                                .sql(sql)
                                .await
                                .map_err(|e2| self.log_sql_error(sql, exec_ctx, e2))?;

                            let template_plan = retry_df.logical_plan().clone();
                            self.plan_cache.insert(cache_key.clone(), template_plan.clone());
                            let bound_plan = replace_placeholders_in_plan(template_plan, &params)?;
                            retry_session
                                .execute_logical_plan(bound_plan)
                                .await
                                .map_err(|e3| KalamDbError::ExecutionError(e3.to_string()))?
                        } else {
                            return Err(self.log_sql_error(sql, exec_ctx, e));
                        }
                    },
                }
            }
        };

        let batches = df.collect().await.map_err(|e| {
            KalamDbError::Other(format!("Error executing DML statement '{}': {}", sql, e))
        })?;

        let rows_affected = Self::extract_rows_affected(&batches)?;
        tracing::Span::current().record("rows_affected", rows_affected);

        Ok(match dml_kind {
            DmlKind::Insert => ExecutionResult::Inserted { rows_affected },
            DmlKind::Update => ExecutionResult::Updated { rows_affected },
            DmlKind::Delete => ExecutionResult::Deleted { rows_affected },
        })
    }

    /// Execute SELECT via DataFusion with per-user session
    #[tracing::instrument(
        name = "sql.select_datafusion",
        skip_all,
        fields(row_count = tracing::field::Empty)
    )]
    async fn execute_via_datafusion(
        &self,
        sql: &str,
        params: Vec<ScalarValue>,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use crate::sql::executor::default_ordering::apply_default_order_by;
        use crate::sql::executor::parameter_binding::{
            replace_placeholders_in_plan, validate_params,
        };

        // Validate parameters if present
        if !params.is_empty() {
            validate_params(&params)?;
        }

        let session = exec_ctx.create_session_with_user();

        // Try cached template plan first (works for both plain and parameterized SQL).
        // Key excludes user_id because LogicalPlan is user-agnostic - filtering happens at scan time.
        let cache_key =
            PlanCacheKey::new(exec_ctx.default_namespace().clone(), exec_ctx.user_role(), sql);

        let df = if let Some(template_plan) = self.plan_cache.get(&cache_key) {
            let executable_plan = if params.is_empty() {
                (*template_plan).clone()
            } else {
                replace_placeholders_in_plan((*template_plan).clone(), &params)?
            };

            match session.execute_logical_plan(executable_plan).await {
                Ok(df) => df,
                Err(e) => {
                    log::warn!("Failed to execute cached plan, reparsing SQL: {}", e);
                    let planned_df = match session.sql(sql).await {
                        Ok(df) => df,
                        Err(e) => {
                            if Self::is_table_not_found_error(&e) {
                                log::warn!(
                                    target: "sql::plan",
                                    "⚠️  Table not found during planning; reloading table providers and retrying once | sql='{}'",
                                    sql
                                );
                                if let Err(e) = self.load_existing_tables().await {
                                    log::warn!(
                                        target: "sql::plan",
                                        "⚠️  Failed to reload table providers after missing table | sql='{}' | error='{}'",
                                        sql,
                                        e
                                    );
                                }
                                let retry_session = exec_ctx.create_session_with_user();
                                match retry_session.sql(sql).await {
                                    Ok(df) => df,
                                    Err(e2) => {
                                        return Err(self.log_sql_error(sql, exec_ctx, e2));
                                    },
                                }
                            } else {
                                return Err(self.log_sql_error(sql, exec_ctx, e));
                            }
                        },
                    };

                    let ordered_template = apply_default_order_by(
                        planned_df.logical_plan().clone(),
                        &self.app_context,
                    )
                    .await?;
                    self.plan_cache.insert(cache_key.clone(), ordered_template.clone());

                    let executable_plan = if params.is_empty() {
                        ordered_template
                    } else {
                        replace_placeholders_in_plan(ordered_template, &params)?
                    };

                    match session.execute_logical_plan(executable_plan).await {
                        Ok(df) => df,
                        Err(e) => {
                            log::error!(
                                target: "sql::exec",
                                "❌ SQL execution failed after replan | sql='{}' | params={} | error='{}'",
                                sql,
                                params.len(),
                                e
                            );
                            return Err(KalamDbError::ExecutionError(e.to_string()));
                        },
                    }
                },
            }
        } else {
            let planned_df = match session.sql(sql).await {
                Ok(df) => df,
                Err(e) => {
                    if Self::is_table_not_found_error(&e) {
                        log::warn!(
                            target: "sql::plan",
                            "⚠️  Table not found during planning; reloading table providers and retrying once | sql='{}'",
                            sql
                        );
                        if let Err(e) = self.load_existing_tables().await {
                            log::warn!(
                                target: "sql::plan",
                                "⚠️  Failed to reload table providers after missing table | sql='{}' | error='{}'",
                                sql,
                                e
                            );
                        }
                        let retry_session = exec_ctx.create_session_with_user();
                        match retry_session.sql(sql).await {
                            Ok(df) => df,
                            Err(e2) => {
                                return Err(self.log_sql_error(sql, exec_ctx, e2));
                            },
                        }
                    } else {
                        return Err(self.log_sql_error(sql, exec_ctx, e));
                    }
                },
            };

            // Apply default ORDER BY by primary key columns (or _seq as fallback)
            // and cache the ordered template plan for subsequent executions.
            let ordered_template =
                apply_default_order_by(planned_df.logical_plan().clone(), &self.app_context)
                    .await?;
            self.plan_cache.insert(cache_key, ordered_template.clone());

            let executable_plan = if params.is_empty() {
                ordered_template
            } else {
                replace_placeholders_in_plan(ordered_template, &params)?
            };

            match session.execute_logical_plan(executable_plan).await {
                Ok(df) => df,
                Err(e) => {
                    log::error!(
                        target: "sql::exec",
                        "❌ SQL execution failed | sql='{}' | params={} | error='{}'",
                        sql,
                        params.len(),
                        e
                    );
                    return Err(KalamDbError::ExecutionError(e.to_string()));
                },
            }
        };

        // Check permissions on the logical plan
        //FIXME: Check do we still need this?? now we have a permission check in each tableprovider
        //self.check_select_permissions(df.logical_plan(), exec_ctx)?;

        // Capture schema before collecting (needed for 0 row results)
        // DFSchema -> Arrow Schema via inner() method
        let schema: arrow::datatypes::SchemaRef =
            std::sync::Arc::new(df.schema().as_arrow().clone());

        // Execute and collect results (log execution errors)
        let batches = match df.collect().await {
            Ok(batches) => batches,
            Err(e) => {
                log::error!(
                    target: "sql::exec",
                    "❌ SQL execution failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    exec_ctx.user_role(),
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing query: {}", e)));
            },
        };

        // Calculate total row count
        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();
        tracing::Span::current().record("row_count", row_count);

        // Return batches with row count and schema (schema is needed when batches is empty)
        Ok(ExecutionResult::Rows {
            batches,
            row_count,
            schema: Some(schema),
        })
    }

    /// Execute DataFusion meta commands (EXPLAIN, SET, SHOW, etc.)
    ///
    /// These commands are passed directly to DataFusion without custom parsing.
    /// No plan caching is performed since these are diagnostic/config commands.
    /// Authorization is already checked in the classifier (admin only).
    #[tracing::instrument(name = "sql.meta_command", skip_all)]
    async fn execute_meta_command(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Create per-request SessionContext with user_id injected
        let session = exec_ctx.create_session_with_user();

        // Execute the command directly via DataFusion
        let df = match session.sql(sql).await {
            Ok(df) => df,
            Err(e) => {
                log::error!(
                    target: "sql::meta",
                    "❌ Meta command failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    exec_ctx.user_role(),
                    e
                );
                return Err(KalamDbError::ExecutionError(e.to_string()));
            },
        };

        // Capture schema before collecting
        let schema: arrow::datatypes::SchemaRef =
            std::sync::Arc::new(df.schema().as_arrow().clone());

        // Execute and collect results
        let batches = match df.collect().await {
            Ok(batches) => batches,
            Err(e) => {
                log::error!(
                    target: "sql::meta",
                    "❌ Meta command execution failed | sql='{}' | user='{}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing meta command: {}", e)));
            },
        };

        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();

        log::debug!(
            target: "sql::meta",
            "✅ Meta command completed | sql='{}' | rows={}",
            sql,
            row_count
        );

        Ok(ExecutionResult::Rows {
            batches,
            row_count,
            schema: Some(schema),
        })
    }

    /// Log SQL errors with appropriate level (warn for user errors, error for system errors)
    fn log_sql_error(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        e: datafusion::error::DataFusionError,
    ) -> KalamDbError {
        let error_msg = e.to_string().to_lowercase();
        let is_table_not_found = error_msg.contains("table") && error_msg.contains("not found")
            || error_msg.contains("relation") && error_msg.contains("does not exist")
            || error_msg.contains("unknown table");

        if is_table_not_found {
            log::warn!(
                target: "sql::plan",
                "⚠️  Table not found | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                sql,
                exec_ctx.user_id().as_str(),
                exec_ctx.user_role(),
                e
            );
        } else {
            log::error!(
                target: "sql::plan",
                "❌ SQL planning failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                sql,
                exec_ctx.user_id().as_str(),
                exec_ctx.user_role(),
                e
            );
        }
        KalamDbError::ExecutionError(e.to_string())
    }

    fn extract_rows_affected(batches: &[RecordBatch]) -> Result<usize, KalamDbError> {
        let mut total: usize = 0;

        for batch in batches {
            if batch.num_columns() == 0 || batch.num_rows() == 0 {
                continue;
            }

            let count_column = batch.column_by_name("count").unwrap_or_else(|| batch.column(0));

            for row_idx in 0..batch.num_rows() {
                let count_value =
                    arrow_value_to_scalar(count_column.as_ref(), row_idx).map_err(|e| {
                        KalamDbError::ExecutionError(format!(
                            "Failed to decode DML count value from result batch: {}",
                            e
                        ))
                    })?;
                total += Self::scalar_count_to_usize(count_value)?;
            }
        }

        Ok(total)
    }

    fn scalar_count_to_usize(value: ScalarValue) -> Result<usize, KalamDbError> {
        let invalid = |v: ScalarValue| {
            KalamDbError::ExecutionError(format!(
                "DML result does not contain a valid count value: {:?}",
                v
            ))
        };

        match value {
            ScalarValue::UInt64(Some(v)) => usize::try_from(v).map_err(|_| {
                KalamDbError::ExecutionError(format!(
                    "DML count {} exceeds platform usize range",
                    v
                ))
            }),
            ScalarValue::UInt32(Some(v)) => Ok(v as usize),
            ScalarValue::UInt16(Some(v)) => Ok(v as usize),
            ScalarValue::UInt8(Some(v)) => Ok(v as usize),
            ScalarValue::Int64(Some(v)) if v >= 0 => usize::try_from(v as u64).map_err(|_| {
                KalamDbError::ExecutionError(format!(
                    "DML count {} exceeds platform usize range",
                    v
                ))
            }),
            ScalarValue::Int32(Some(v)) if v >= 0 => Ok(v as usize),
            ScalarValue::Int16(Some(v)) if v >= 0 => Ok(v as usize),
            ScalarValue::Int8(Some(v)) if v >= 0 => Ok(v as usize),
            ScalarValue::Utf8(Some(v)) | ScalarValue::LargeUtf8(Some(v)) => {
                let parsed = v.parse::<u64>().map_err(|e| {
                    KalamDbError::ExecutionError(format!(
                        "Failed to parse DML count '{}' as number: {}",
                        v, e
                    ))
                })?;
                usize::try_from(parsed).map_err(|_| {
                    KalamDbError::ExecutionError(format!(
                        "DML count {} exceeds platform usize range",
                        parsed
                    ))
                })
            },
            other => Err(invalid(other)),
        }
    }

    /// Load existing tables from system.tables and register providers
    ///
    /// Called during server startup to restore table access after restart.
    /// Loads table definitions from the store and creates/registers:
    /// - UserTableShared instances for USER tables
    /// - SharedTableProvider instances for SHARED tables  
    /// - StreamTableProvider instances for STREAM tables
    ///
    /// # Returns
    /// Ok on success, error if table loading fails
    pub async fn load_existing_tables(&self) -> Result<(), KalamDbError> {
        let app_context = &self.app_context;
        // Delegate to unified SchemaRegistry initialization
        app_context.schema_registry().initialize_tables()
    }

    /// Expose the shared `AppContext` for upcoming migrations.
    /// TODO: Remove this since everyone has appcontext access now from the executor
    pub fn app_context(&self) -> &std::sync::Arc<crate::app_context::AppContext> {
        &self.app_context
    }
}
