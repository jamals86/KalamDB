//! SQL execution handler for the `/v1/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.
//! Authentication is handled automatically by the `AuthSession` extractor.

use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use kalamdb_auth::AuthSession;
use kalamdb_commons::models::UserId;
use kalamdb_core::providers::arrow_json_conversion::{
    json_value_to_scalar_strict, record_batch_to_json_rows, SerializationMode,
};
use kalamdb_core::sql::executor::models::ExecutionContext;
use kalamdb_core::sql::executor::{ExecutorMetadataAlias, ScalarValue, SqlExecutor};
use kalamdb_core::sql::ExecutionResult;
use std::sync::Arc;
use std::time::Instant;

use crate::models::{QueryRequest, QueryResult, SqlResponse};
use crate::rate_limiter::RateLimiter;

/// POST /v1/api/sql - Execute SQL statement(s)
///
/// Accepts a JSON payload with a `sql` field containing one or more SQL statements.
/// Multiple statements can be separated by semicolons and will be executed sequentially.
///
/// # Authentication
/// Requires authentication via Authorization header.
/// Authentication is handled automatically by the `AuthSession` extractor.
///
/// # Example Request
/// ```json
/// {
///   "sql": "SELECT * FROM users WHERE id = 1"
/// }
/// ```
///
/// # Example Response (Success)
/// ```json
/// {
///   "status": "success",
///   "results": [
///     {
///       "rows": [{"id": 1, "name": "Alice"}],
///       "row_count": 1,
///       "columns": ["id", "name"]
///     }
///   ],
///   "took": 15.0
/// }
/// ```
#[post("/sql")]
pub async fn execute_sql_v1(
    session: AuthSession,
    http_req: HttpRequest,
    req: web::Json<QueryRequest>,
    app_context: web::Data<Arc<kalamdb_core::app_context::AppContext>>,
    sql_executor: web::Data<Arc<SqlExecutor>>,
    rate_limiter: Option<web::Data<Arc<RateLimiter>>>,
) -> impl Responder {
    let start_time = Instant::now();

    // NOTE: Audit logging for password-based auth has been moved to the AuthSession extractor
    // (logs once on first authentication, not on every query). This improves query performance
    // by ~10-20% for high-frequency insert workloads.

    // Rate limiting: Check if user can execute query
    if let Some(ref limiter) = rate_limiter {
        if !limiter.check_query_rate(&session.user.user_id) {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            log::warn!(
                "Rate limit exceeded for user: {} (queries per second)",
                session.user.user_id.as_str()
            );
            return HttpResponse::TooManyRequests().json(SqlResponse::error(
                "RATE_LIMIT_EXCEEDED",
                "Too many queries per second. Please slow down.",
                took,
            ));
        }
    }

    // Extract request_id for ExecutionContext
    let request_id = http_req
        .headers()
        .get("X-Request-ID")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Parse parameters if provided
    let params = match &req.params {
        Some(json_params) => {
            let mut scalar_params = Vec::new();
            for (idx, json_val) in json_params.iter().enumerate() {
                match json_value_to_scalar_strict(json_val) {
                    Ok(scalar) => scalar_params.push(scalar),
                    Err(err) => {
                        let took = start_time.elapsed().as_secs_f64() * 1000.0;
                        return HttpResponse::BadRequest().json(SqlResponse::error(
                            "INVALID_PARAMETER",
                            &format!("Parameter ${} invalid: {}", idx + 1, err),
                            took,
                        ));
                    }
                }
            }
            scalar_params
        }
        None => Vec::new(),
    };

    // Parse SQL statements
    let statements = match kalamdb_sql::split_statements(&req.sql) {
        Ok(stmts) => stmts,
        Err(err) => {
            let took = start_time.elapsed().as_secs_f64() * 1000.0;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                "BATCH_PARSE_ERROR",
                &format!("Failed to parse SQL batch: {}", err),
                took,
            ));
        }
    };

    if statements.is_empty() {
        let took = start_time.elapsed().as_secs_f64() * 1000.0;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            "EMPTY_SQL",
            "No SQL statements provided",
            took,
        ));
    }

    // Reject multi-statement batches with parameters
    if !params.is_empty() && statements.len() > 1 {
        let took = start_time.elapsed().as_secs_f64() * 1000.0;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            "PARAMS_WITH_BATCH",
            "Parameters not supported with multi-statement batches",
            took,
        ));
    }

    // Execute statements
    let mut results = Vec::new();
    let mut total_inserted = 0usize;
    let mut total_updated = 0usize;
    let mut total_deleted = 0usize;

    // Get namespace_id from request (client-provided or None for default)
    let namespace_id = req.namespace_id.clone();
    
    // Get serialization mode from request (default: Simple)
    let serialization_mode = req.serialization_mode;

    for (idx, sql) in statements.iter().enumerate() {
        let stmt_start = Instant::now();
        match execute_single_statement(
            sql,
            app_context.get_ref(),
            sql_executor.get_ref(),
            &session,
            request_id.as_deref(),
            None,
            params.clone(),
            namespace_id.clone(),
            serialization_mode,
        )
        .await
        {
            Ok(result) => {
                // Calculate timing and row count
                let stmt_duration_secs = stmt_start.elapsed().as_secs_f64();
                let stmt_duration_ms = stmt_duration_secs * 1000.0;
                let row_count = result.rows.as_ref().map(|r| r.len()).unwrap_or(0);

                // Debug log for SQL execution (includes timing)
                log::debug!(
                    target: "sql::exec",
                    "✅ SQL executed | sql='{}' | user='{}' | role='{:?}' | rows={} | took={:.3}ms",
                    sql,
                    session.user.user_id.as_str(),
                    session.user.role,
                    row_count,
                    stmt_duration_ms
                );

                // Log slow query if threshold exceeded
                app_context.slow_query_logger().log_if_slow(
                    sql.to_string(),
                    stmt_duration_secs,
                    row_count,
                    session.user.user_id.clone(),
                    kalamdb_core::schema_registry::TableType::User,
                    None,
                );

                // Accumulate DML row counts for multi-statement batches
                if statements.len() > 1 {
                    if let Some(ref msg) = result.message {
                        if msg.contains("Inserted") {
                            total_inserted += result.row_count;
                            continue;
                        } else if msg.contains("Updated") {
                            total_updated += result.row_count;
                            continue;
                        } else if msg.contains("Deleted") {
                            total_deleted += result.row_count;
                            continue;
                        }
                    }
                }

                results.push(result);
            }
            Err(err) => {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return HttpResponse::BadRequest().json(SqlResponse::error_with_details(
                    "SQL_EXECUTION_ERROR",
                    &format!("Statement {} failed: {}", idx + 1, err),
                    sql,
                    took,
                ));
            }
        }
    }

    // Add accumulated DML results for multi-statement batches
    if statements.len() > 1 {
        if total_inserted > 0 {
            results.push(QueryResult::with_affected_rows(
                total_inserted,
                Some(format!("Inserted {} row(s)", total_inserted)),
            ));
        }
        if total_updated > 0 {
            results.push(QueryResult::with_affected_rows(
                total_updated,
                Some(format!("Updated {} row(s)", total_updated)),
            ));
        }
        if total_deleted > 0 {
            results.push(QueryResult::with_affected_rows(
                total_deleted,
                Some(format!("Deleted {} row(s)", total_deleted)),
            ));
        }
    }

    let took = start_time.elapsed().as_secs_f64() * 1000.0;
    HttpResponse::Ok().json(SqlResponse::success(results, took))
}

/// Execute a single SQL statement
async fn execute_single_statement(
    sql: &str,
    app_context: &Arc<kalamdb_core::app_context::AppContext>,
    sql_executor: &Arc<SqlExecutor>,
    session: &AuthSession,
    request_id: Option<&str>,
    metadata: Option<&ExecutorMetadataAlias>,
    params: Vec<ScalarValue>,
    namespace_id: Option<String>,
    serialization_mode: SerializationMode,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    use kalamdb_commons::NamespaceId;
    
    let base_session = app_context.base_session_context();
    let mut exec_ctx = ExecutionContext::new(
        session.user.user_id.clone(),
        session.user.role,
        Arc::clone(&base_session),
    );

    // Apply namespace if provided by client
    if let Some(ns) = namespace_id {
        exec_ctx = exec_ctx.with_namespace_id(NamespaceId::new(ns));
    }

    if let Some(rid) = request_id {
        exec_ctx = exec_ctx.with_request_id(rid.to_string());
    }
    if let Some(ip) = &session.connection_info.remote_addr {
        exec_ctx = exec_ctx.with_ip(ip.clone());
    }

    match sql_executor
        .execute_with_metadata(sql, &exec_ctx, metadata, params)
        .await
    {
        Ok(exec_result) => match exec_result {
            ExecutionResult::Success { message } => Ok(QueryResult::with_message(message)),
            ExecutionResult::Rows { batches, schema, .. } => {
                record_batch_to_query_result(batches, schema, Some(&session.user.user_id), serialization_mode)
            }
            ExecutionResult::Inserted { rows_affected } => Ok(QueryResult::with_affected_rows(
                rows_affected,
                Some(format!("Inserted {} row(s)", rows_affected)),
            )),
            ExecutionResult::Updated { rows_affected } => Ok(QueryResult::with_affected_rows(
                rows_affected,
                Some(format!("Updated {} row(s)", rows_affected)),
            )),
            ExecutionResult::Deleted { rows_affected } => Ok(QueryResult::with_affected_rows(
                rows_affected,
                Some(format!("Deleted {} row(s)", rows_affected)),
            )),
            ExecutionResult::Flushed {
                tables,
                bytes_written,
            } => Ok(QueryResult::with_affected_rows(
                tables.len(),
                Some(format!(
                    "Flushed {} table(s), {} bytes written",
                    tables.len(),
                    bytes_written
                )),
            )),
            ExecutionResult::Subscription {
                subscription_id,
                channel,
                select_query,
            } => {
                let sub_data = serde_json::json!({
                    "status": "active",
                    "ws_url": channel,
                    "subscription": {
                        "id": subscription_id,
                        "sql": select_query
                    },
                    "message": "WebSocket subscription created. Connect to ws_url to receive updates."
                });
                Ok(QueryResult::subscription(sub_data))
            }
            ExecutionResult::JobKilled { job_id, status } => Ok(QueryResult::with_message(
                format!("Job {} killed: {}", job_id, status),
            )),
        },
        Err(e) => Err(Box::new(e)),
    }
}

/// Convert Arrow RecordBatches to QueryResult
/// 
/// Uses the unified record_batch_to_json_rows function with configurable serialization mode.
/// - Simple mode: Plain JSON values (Int64/UInt64 as strings for precision)
/// - Typed mode: Values with type wrappers and formatted timestamps
fn record_batch_to_query_result(
    batches: Vec<arrow::record_batch::RecordBatch>,
    schema: Option<arrow::datatypes::SchemaRef>,
    user_id: Option<&UserId>,
    mode: SerializationMode,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // Get schema from first batch, or from explicitly provided schema for empty results
    let schema = if !batches.is_empty() {
        batches[0].schema()
    } else if let Some(s) = schema {
        s
    } else {
        // No batches and no schema - truly empty result
        return Ok(QueryResult::with_message(
            "Query executed successfully".to_string(),
        ));
    };

    let column_names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();

    let mut rows = Vec::new();
    for batch in &batches {
        let batch_rows = record_batch_to_json_rows(batch, mode)
            .map_err(|e| format!("Failed to convert batch to JSON: {}", e))?;
        rows.extend(batch_rows);
    }

    let mut result = QueryResult::with_rows(rows, column_names.clone());

    // Mask sensitive columns for non-admin users
    if let Some(rows) = result.rows.as_mut() {
        if !is_admin(user_id) {
            mask_sensitive_column(rows, &column_names, "credentials");
            mask_sensitive_column(rows, &column_names, "password_hash");
        }
    }

    Ok(result)
}

/// Mask a sensitive column with "***"
fn mask_sensitive_column(
    rows: &mut [std::collections::HashMap<String, serde_json::Value>],
    column_names: &[String],
    target_column: &str,
) {
    if let Some(col_idx) = column_names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(target_column))
    {
        let key = column_names[col_idx].clone();
        for row in rows.iter_mut() {
            if let Some(value) = row.get_mut(&key) {
                if !value.is_null() {
                    *value = serde_json::Value::String("***".to_string());
                }
            }
        }
    }
}

fn is_admin(user_id: Option<&UserId>) -> bool {
    match user_id {
        Some(id) => {
            let lower = id.as_str().to_lowercase();
            lower == "admin" || lower == "system"
        }
        None => false,
    }
}
