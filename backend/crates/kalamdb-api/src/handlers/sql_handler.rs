//! SQL execution handler for the `/v1/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use kalamdb_auth::{extract_auth_with_repo, AuthenticatedUser, UserRepository};
use kalamdb_commons::models::UserId;
use kalamdb_core::sql::ExecutionResult;
use kalamdb_core::sql::executor::{ExecutorMetadataAlias, SqlExecutor};
use kalamdb_core::sql::executor::models::{ExecutionContext, ScalarValue};
use log::warn;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;

use crate::models::{QueryResult, SqlRequest, SqlResponse};
use crate::rate_limiter::RateLimiter;

/// Convert JSON value to DataFusion ScalarValue
///
/// Supports common JSON types: null, bool, number (int/float), string
/// Date/timestamp parsing will be added when needed
fn json_to_scalar_value(value: &JsonValue) -> Result<ScalarValue, String> {
    match value {
        JsonValue::Null => Ok(ScalarValue::Utf8(None)),
        JsonValue::Bool(b) => Ok(ScalarValue::Boolean(Some(*b))),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(ScalarValue::Int64(Some(i)))
            } else if let Some(f) = n.as_f64() {
                Ok(ScalarValue::Float64(Some(f)))
            } else {
                Err(format!("Unsupported number format: {}", n))
            }
        }
        JsonValue::String(s) => Ok(ScalarValue::Utf8(Some(s.clone()))),
        JsonValue::Array(_) => Err("Array parameters not yet supported".to_string()),
        JsonValue::Object(_) => Err("Object parameters not yet supported".to_string()),
    }
}

/// POST /v1/api/sql - Execute SQL statement(s)
///
/// Accepts a JSON payload with a `sql` field containing one or more SQL statements.
/// Multiple statements can be separated by semicolons and will be executed sequentially.
///
/// # Authentication
/// Requires authentication via Authorization header (handled by middleware).
/// The authenticated user is available in request extensions.
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
///   "took_ms": 15
/// }
/// ```
///
/// # Example Response (Error)
/// ```json
/// {
///   "status": "error",
///   "results": [],
///   "took_ms": 5,
///   "error": {
///     "code": "INVALID_SQL",
///     "message": "Syntax error near 'SELCT'"
///   }
/// }
/// ```
#[post("/sql")]
pub async fn execute_sql_v1(
    http_req: HttpRequest,
    req: web::Json<SqlRequest>,
    app_context: web::Data<Arc<kalamdb_core::app_context::AppContext>>,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    rate_limiter: Option<web::Data<Arc<RateLimiter>>>,
) -> impl Responder {
    let start_time = Instant::now();

    // Extract and validate authentication from request using repo
    let auth_result = match extract_auth_with_repo(&http_req, user_repo.get_ref()).await {
        Ok(auth) => auth,
        Err(e) => {
            let took_ms = start_time.elapsed().as_millis() as u64;
            let (code, message) = match e {
                kalamdb_auth::AuthError::MissingAuthorization(msg) => {
                    ("MISSING_AUTHORIZATION", msg)
                }
                kalamdb_auth::AuthError::MalformedAuthorization(msg) => {
                    ("MALFORMED_AUTHORIZATION", msg)
                }
                kalamdb_auth::AuthError::InvalidCredentials(msg) => ("INVALID_CREDENTIALS", msg),
                kalamdb_auth::AuthError::RemoteAccessDenied(msg) => ("REMOTE_ACCESS_DENIED", msg),
                kalamdb_auth::AuthError::UserNotFound(msg) => ("USER_NOT_FOUND", msg),
                kalamdb_auth::AuthError::DatabaseError(msg) => ("DATABASE_ERROR", msg),
                _ => ("AUTHENTICATION_ERROR", format!("{:?}", e)),
            };
            return HttpResponse::Unauthorized().json(SqlResponse::error(code, &message, took_ms));
        }
    };

    // Rate limiting: Check if user can execute query
    if let Some(ref limiter) = rate_limiter {
        if !limiter.check_query_rate(&auth_result.user_id) {
            let took_ms = start_time.elapsed().as_millis() as u64;
            warn!(
                "Rate limit exceeded for user: {} (queries per second)",
                auth_result.user_id.as_str()
            );
            return HttpResponse::TooManyRequests().json(SqlResponse::error(
                "RATE_LIMIT_EXCEEDED",
                "Too many queries per second. Please slow down.",
                took_ms,
            ));
        }
    }

    // Parse parameters if provided (T040: JSON → ScalarValue deserialization)
    let params = match &req.params {
        Some(json_params) => {
            let mut scalar_params = Vec::new();
            for (idx, json_val) in json_params.iter().enumerate() {
                match json_to_scalar_value(json_val) {
                    Ok(scalar) => scalar_params.push(scalar),
                    Err(err) => {
                        let took_ms = start_time.elapsed().as_millis() as u64;
                        return HttpResponse::BadRequest().json(SqlResponse::error(
                            "INVALID_PARAMETER",
                            &format!("Parameter ${} invalid: {}", idx + 1, err),
                            took_ms,
                        ));
                    }
                }
            }
            scalar_params
        }
        None => Vec::new(),
    };

    let statements = match kalamdb_sql::split_statements(&req.sql) {
        Ok(stmts) => stmts,
        Err(err) => {
            let took_ms = start_time.elapsed().as_millis() as u64;
            return HttpResponse::BadRequest().json(SqlResponse::error(
                "BATCH_PARSE_ERROR",
                &format!("Failed to parse SQL batch: {}", err),
                took_ms,
            ));
        }
    };

    if statements.is_empty() {
        let took_ms = start_time.elapsed().as_millis() as u64;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            "EMPTY_SQL",
            "No SQL statements provided",
            took_ms,
        ));
    }

    // Reject multi-statement batches with parameters (simplifies implementation)
    if params.len() > 0 && statements.len() > 1 {
        let took_ms = start_time.elapsed().as_millis() as u64;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            "PARAMS_WITH_BATCH",
            "Parameters not supported with multi-statement batches",
            took_ms,
        ));
    }

    // Execute each statement sequentially
    let mut results = Vec::new();
    let sql_executor: Option<&Arc<SqlExecutor>> = http_req
        .app_data::<web::Data<Arc<SqlExecutor>>>()
        .map(|data| data.as_ref());

    // Phase 3 (T033): Extract request_id and ip_address for ExecutionContext
    let request_id = http_req
        .headers()
        .get("X-Request-ID")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    
    let resolved_ip = http_req
        .extensions()
        .get::<AuthenticatedUser>()
        .and_then(|user| user.connection_info.remote_addr.clone())
        .or_else(|| {
            http_req
                .headers()
                .get("X-Forwarded-For")
                .and_then(|h| h.to_str().ok())
                .and_then(|raw| raw.split(',').next().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            http_req
                .connection_info()
                .realip_remote_addr()
                .map(|s| s.to_string())
        });
    
    for (idx, sql) in statements.iter().enumerate() {
        match execute_single_statement(
            sql,
            app_context.get_ref(),
            sql_executor,
            &auth_result,
            request_id.as_deref(),
            resolved_ip.as_deref(),
            None,
            params.clone(), // Pass parameters to each statement
        )
        .await
        {
            Ok(result) => results.push(result),
            Err(err) => {
                let took_ms = start_time.elapsed().as_millis() as u64;
                return HttpResponse::BadRequest().json(SqlResponse::error_with_details(
                    "SQL_EXECUTION_ERROR",
                    &format!("Statement {} failed: {}", idx + 1, err),
                    sql,
                    took_ms,
                ));
            }
        }
    }

    let took_ms = start_time.elapsed().as_millis() as u64;
    HttpResponse::Ok().json(SqlResponse::success(results, took_ms))
}

/// Execute a single SQL statement
/// Uses SqlExecutor for all SQL (custom DDL and standard DataFusion SQL)
async fn execute_single_statement(
    sql: &str,
    app_context: &Arc<kalamdb_core::app_context::AppContext>,
    sql_executor: Option<&Arc<SqlExecutor>>,
    auth: &kalamdb_auth::AuthenticatedRequest,
    request_id: Option<&str>,
    ip_address: Option<&str>,
    metadata: Option<&ExecutorMetadataAlias>,
    params: Vec<ScalarValue>, // T040: Accept parameters
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // Phase 3 (T033): Construct ExecutionContext with user identity, request tracking, and base SessionContext
    // Get base SessionContext from AppContext (tables already registered)
    // ExecutionContext will extract SessionState and inject user_id for per-user filtering
    let base_session = app_context.base_session_context();
    let mut exec_ctx = ExecutionContext::new(auth.user_id.clone(), auth.role, Arc::clone(&base_session));
    
    // Add request_id and ip_address if available
    if let Some(rid) = request_id {
        exec_ctx = exec_ctx.with_request_id(rid.to_string());
    }
    if let Some(ip) = ip_address {
        exec_ctx = exec_ctx.with_ip(ip.to_string());
    }
    
    // If sql_executor is available, use it (production path)
    if let Some(sql_executor) = sql_executor {
        // Execute through SqlExecutor (handles both custom DDL and DataFusion)
        // Note: session parameter is not used anymore - exec_ctx creates per-request session
        match sql_executor
            .execute_with_metadata(sql, &exec_ctx, metadata, params) // Pass parameters
            .await
        {
            Ok(result) => {
                // Convert ExecutionResult to QueryResult
                // Phase 3 (T036-T038): Use new ExecutionResult struct variants with row counts
                match result {
                    ExecutionResult::Success { message } => Ok(QueryResult::with_message(message)),
                    ExecutionResult::Rows { batches, row_count: _ } => {
                        record_batch_to_query_result(batches, Some(&auth.user_id))
                    }
                    ExecutionResult::Inserted { rows_affected } => {
                        Ok(QueryResult::with_affected_rows(
                            rows_affected,
                            Some(format!("Inserted {} row(s)", rows_affected)),
                        ))
                    }
                    ExecutionResult::Updated { rows_affected } => {
                        Ok(QueryResult::with_affected_rows(
                            rows_affected,
                            Some(format!("Updated {} row(s)", rows_affected)),
                        ))
                    }
                    ExecutionResult::Deleted { rows_affected } => {
                        Ok(QueryResult::with_affected_rows(
                            rows_affected,
                            Some(format!("Deleted {} row(s)", rows_affected)),
                        ))
                    }
                    ExecutionResult::Flushed { tables, bytes_written } => {
                        Ok(QueryResult::with_affected_rows(
                            tables.len(),
                            Some(format!(
                                "Flushed {} table(s), {} bytes written",
                                tables.len(),
                                bytes_written
                            )),
                        ))
                    }
                    ExecutionResult::Subscription { subscription_id, channel } => {
                        // Create subscription result as JSON
                        let sub_data = serde_json::json!({
                            "subscription_id": subscription_id,
                            "channel": channel,
                            "status": "active"
                        });
                        Ok(QueryResult::subscription(sub_data))
                    }
                    ExecutionResult::JobKilled { job_id, status } => {
                        Ok(QueryResult::with_message(format!("Job {} killed: {}", job_id, status)))
                    }
                }
            }
            Err(e) => Err(Box::new(e)),
        }
    }
    else {
        // // Fallback for testing: use shared session from AppContext (avoid memory leak)
        // let session = app_context.session();
        // let df = session.sql(sql).await?;
        // let batches = df.collect().await?;
        // record_batch_to_query_result(batches, None)

        //Throw error if SqlExecutor is not available
        Err("SqlExecutor not available".into())
    }
}

/// Convert Arrow RecordBatches to QueryResult
fn record_batch_to_query_result(
    batches: Vec<arrow::record_batch::RecordBatch>,
    user_id: Option<&UserId>,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    if batches.is_empty() {
        return Ok(QueryResult::with_message(
            "Query executed successfully".to_string(),
        ));
    }

    // Convert Arrow batches to JSON rows
    let schema = batches[0].schema();
    let column_names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();

    let mut rows = Vec::new();

    for batch in &batches {
        // Convert each batch to JSON using Arrow's JSON writer
        let mut buf = Vec::new();
        let mut writer = arrow::json::LineDelimitedWriter::new(&mut buf);
        writer.write(batch)?;
        writer.finish()?;

        // Parse the JSON lines
        let json_str = String::from_utf8(buf)?;
        for line in json_str.lines() {
            if !line.is_empty() {
                let json_row: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(line)?;
                rows.push(json_row);
            }
        }
    }

    let mut result = QueryResult::with_rows(rows, column_names.clone());

    if let Some(rows) = result.rows.as_mut() {
        if !is_admin(user_id) {
            if let Some(credentials_col) = column_names
                .iter()
                .position(|name| name.eq_ignore_ascii_case("credentials"))
            {
                let key = column_names[credentials_col].clone();
                for row in rows.iter_mut() {
                    if let Some(value) = row.get_mut(&key) {
                        if !value.is_null() {
                            *value = serde_json::Value::String("***".to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limiter::RateLimiter;
    use actix_web::{test, App};
    use kalamdb_core::sql::DataFusionSessionFactory;

    // NOTE: These unit tests are disabled because they require full KalamDB setup
    // including RocksDB, SqlExecutor, and authentication middleware.
    // SQL execution is thoroughly tested in integration tests (backend/tests/).

    #[ignore]
    #[actix_rt::test]
    async fn test_execute_sql_endpoint() {
        // Create a test session factory
        let session_factory = Arc::new(DataFusionSessionFactory::new().unwrap());
        let rate_limiter = Arc::new(RateLimiter::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .app_data(web::Data::new(rate_limiter))
                .service(execute_sql_v1),
        )
        .await;

        let req_body = SqlRequest {
            sql: "SELECT 1 as id, 'Alice' as name".to_string(),
            params: None,
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .peer_addr("127.0.0.1:8080".parse().unwrap())
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Parse the response body to verify structure
        let body = test::read_body(resp).await;
        let response: SqlResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.results.len(), 1);
    }

    #[ignore]
    #[actix_rt::test]
    async fn test_execute_sql_empty_query() {
        let session_factory = Arc::new(DataFusionSessionFactory::new().unwrap());
        let rate_limiter = Arc::new(RateLimiter::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .app_data(web::Data::new(rate_limiter))
                .service(execute_sql_v1),
        )
        .await;

        let req_body = SqlRequest {
            sql: "".to_string(),
            params: None,
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .peer_addr("127.0.0.1:8080".parse().unwrap())
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[ignore]
    #[actix_rt::test]
    async fn test_execute_sql_multiple_statements() {
        let session_factory = Arc::new(DataFusionSessionFactory::new().unwrap());
        let rate_limiter = Arc::new(RateLimiter::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .app_data(web::Data::new(rate_limiter))
                .service(execute_sql_v1),
        )
        .await;

        let req_body = SqlRequest {
            sql: "SELECT 1 as id; SELECT 2 as id".to_string(),
            params: None,
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .peer_addr("127.0.0.1:8080".parse().unwrap())
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Parse response and verify we got 2 result sets
        let body = test::read_body(resp).await;
        let response: SqlResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.results.len(), 2);
    }
}
