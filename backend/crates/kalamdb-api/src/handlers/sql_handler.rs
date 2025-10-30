//! SQL execution handler for the `/v1/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use kalamdb_auth::{extract_auth, AuthenticatedUser};
use kalamdb_commons::models::UserId;
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::{ExecutionMetadata, ExecutionResult, SqlExecutor};
use log::warn;
use std::sync::Arc;
use std::time::Instant;

use crate::models::{QueryResult, SqlRequest, SqlResponse};
use crate::rate_limiter::RateLimiter;

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
    session_factory: web::Data<Arc<DataFusionSessionFactory>>,
    rate_limiter: Option<web::Data<Arc<RateLimiter>>>,
) -> impl Responder {
    let start_time = Instant::now();

    // Extract and validate authentication from request
    let sql_executor: Option<&Arc<SqlExecutor>> = http_req
        .app_data::<web::Data<Arc<SqlExecutor>>>()
        .map(|data| data.as_ref());

    let adapter = match sql_executor.and_then(|exec| exec.get_rocks_adapter()) {
        Some(adapter) => adapter,
        None => {
            let took_ms = start_time.elapsed().as_millis() as u64;
            return HttpResponse::InternalServerError().json(SqlResponse::error(
                "CONFIGURATION_ERROR",
                "Database adapter not configured",
                took_ms,
            ));
        }
    };

    let auth_result = match extract_auth(&http_req, &adapter).await {
        Ok(auth) => auth,
        Err(e) => {
            let took_ms = start_time.elapsed().as_millis() as u64;
            let (code, message) = match e {
                kalamdb_auth::AuthError::MissingAuthorization(msg) => ("MISSING_AUTHORIZATION", msg),
                kalamdb_auth::AuthError::MalformedAuthorization(msg) => ("MALFORMED_AUTHORIZATION", msg),
                kalamdb_auth::AuthError::InvalidCredentials(msg) => ("INVALID_CREDENTIALS", msg),
                kalamdb_auth::AuthError::RemoteAccessDenied(msg) => ("REMOTE_ACCESS_DENIED", msg),
                kalamdb_auth::AuthError::UserNotFound(msg) => ("USER_NOT_FOUND", msg),
                kalamdb_auth::AuthError::DatabaseError(msg) => ("DATABASE_ERROR", msg),
                _ => ("AUTHENTICATION_ERROR", format!("{:?}", e)),
            };
            return HttpResponse::Unauthorized().json(SqlResponse::error(code, &message, took_ms));
        }
    };

    let user_id = auth_result.user_id;

    // Rate limiting: Check if user can execute query
    if let Some(ref limiter) = rate_limiter {
        if !limiter.check_query_rate(&user_id) {
            let took_ms = start_time.elapsed().as_millis() as u64;
            warn!(
                "Rate limit exceeded for user: {} (queries per second)",
                user_id.as_str()
            );
            return HttpResponse::TooManyRequests().json(SqlResponse::error(
                "RATE_LIMIT_EXCEEDED",
                "Too many queries per second. Please slow down.",
                took_ms,
            ));
        }
    }

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

    // Execute each statement sequentially
    let mut results = Vec::new();
    let sql_executor: Option<&Arc<SqlExecutor>> = http_req
        .app_data::<web::Data<Arc<SqlExecutor>>>()
        .map(|data| data.as_ref());

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
    let execution_metadata = ExecutionMetadata {
        ip_address: resolved_ip,
    };

    for (idx, sql) in statements.iter().enumerate() {
        match execute_single_statement(
            sql,
            session_factory.get_ref(),
            sql_executor,
            Some(&user_id),
            Some(&execution_metadata),
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
/// Falls back to DataFusionSessionFactory for testing if SqlExecutor is not available
async fn execute_single_statement(
    sql: &str,
    session_factory: &Arc<DataFusionSessionFactory>,
    sql_executor: Option<&Arc<SqlExecutor>>,
    user_id: Option<&UserId>,
    metadata: Option<&ExecutionMetadata>,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // If sql_executor is available, use it (production path)
    if let Some(sql_executor) = sql_executor {
        // Execute through SqlExecutor (handles both custom DDL and DataFusion)
        match sql_executor
            .execute_with_metadata(sql, user_id, metadata)
            .await
        {
            Ok(result) => {
                // Convert ExecutionResult to QueryResult
                match result {
                    ExecutionResult::Success(message) => Ok(QueryResult::with_message(message)),
                    ExecutionResult::RecordBatch(batch) => {
                        record_batch_to_query_result(vec![batch], user_id)
                    }
                    ExecutionResult::RecordBatches(batches) => {
                        record_batch_to_query_result(batches, user_id)
                    }
                    ExecutionResult::Subscription(subscription_data) => {
                        // Return subscription metadata as a special query result
                        Ok(QueryResult::subscription(subscription_data))
                    }
                }
            }
            Err(e) => Err(Box::new(e)),
        }
    } else {
        // Fallback for testing: use DataFusion directly for simple queries
        let session = session_factory.create_session();
        let df = session.sql(sql).await?;
        let batches = df.collect().await?;
        record_batch_to_query_result(batches, None)
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
    use jsonwebtoken::Algorithm;

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
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .insert_header(("X-USER-ID", "test_user"))
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
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .insert_header(("X-USER-ID", "test_user"))
            .peer_addr("127.0.0.1:8080".parse().unwrap())
            .set_json(&req_body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

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
        };

        let req = test::TestRequest::post()
            .uri("/sql")
            .insert_header(("X-USER-ID", "test_user"))
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
