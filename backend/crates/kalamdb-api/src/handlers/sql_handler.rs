//! SQL execution handler for the `/v1/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.

use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use kalamdb_core::catalog::UserId;
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::{ExecutionResult, SqlExecutor};
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
///   "execution_time_ms": 15
/// }
/// ```
///
/// # Example Response (Error)
/// ```json
/// {
///   "status": "error",
///   "results": [],
///   "execution_time_ms": 5,
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
    rate_limiter: web::Data<Arc<RateLimiter>>,
) -> impl Responder {
    let start_time = Instant::now();

    // Extract user_id from X-USER-ID header (optional)
    let user_id: Option<UserId> = http_req
        .headers()
        .get("X-USER-ID")
        .and_then(|h| h.to_str().ok())
        .map(UserId::from);

    // Rate limiting: Check if user can execute query
    if let Some(ref uid) = user_id {
        if !rate_limiter.check_query_rate(uid) {
            let execution_time_ms = start_time.elapsed().as_millis() as u64;
            warn!(
                "Rate limit exceeded for user: {} (queries per second)",
                uid.as_ref()
            );
            return HttpResponse::TooManyRequests().json(SqlResponse::error(
                "RATE_LIMIT_EXCEEDED",
                "Too many queries per second. Please slow down.",
                execution_time_ms,
            ));
        }
    }

    // Split SQL by semicolons to handle multiple statements
    let statements: Vec<&str> = req
        .sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if statements.is_empty() {
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        return HttpResponse::BadRequest().json(SqlResponse::error(
            "EMPTY_SQL",
            "No SQL statements provided",
            execution_time_ms,
        ));
    }

    // Execute each statement sequentially
    let mut results = Vec::new();
    let sql_executor: Option<&Arc<SqlExecutor>> = http_req
        .app_data::<web::Data<Arc<SqlExecutor>>>()
        .map(|data| data.as_ref());

    for (idx, sql) in statements.iter().enumerate() {
        match execute_single_statement(
            sql,
            session_factory.get_ref(),
            sql_executor,
            user_id.as_ref(),
        )
        .await
        {
            Ok(result) => results.push(result),
            Err(err) => {
                let execution_time_ms = start_time.elapsed().as_millis() as u64;
                return HttpResponse::BadRequest().json(SqlResponse::error_with_details(
                    "SQL_EXECUTION_ERROR",
                    &format!("Error executing statement {}: {}", idx + 1, err),
                    sql,
                    execution_time_ms,
                ));
            }
        }
    }

    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    HttpResponse::Ok().json(SqlResponse::success(results, execution_time_ms))
}

/// Execute a single SQL statement
/// Uses SqlExecutor for all SQL (custom DDL and standard DataFusion SQL)
/// Falls back to DataFusionSessionFactory for testing if SqlExecutor is not available
async fn execute_single_statement(
    sql: &str,
    session_factory: &Arc<DataFusionSessionFactory>,
    sql_executor: Option<&Arc<SqlExecutor>>,
    user_id: Option<&UserId>,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // If sql_executor is available, use it (production path)
    if let Some(sql_executor) = sql_executor {
        // Execute through SqlExecutor (handles both custom DDL and DataFusion)
        match sql_executor.execute(sql, user_id).await {
            Ok(result) => {
                // Convert ExecutionResult to QueryResult
                match result {
                    ExecutionResult::Success(message) => {
                        Ok(QueryResult::with_message(message))
                    }
                    ExecutionResult::RecordBatch(batch) => {
                        record_batch_to_query_result(vec![batch])
                    }
                    ExecutionResult::RecordBatches(batches) => {
                        record_batch_to_query_result(batches)
                    }
                }
            }
            Err(e) => {
                Err(Box::new(e))
            }
        }
    } else {
        // Fallback for testing: use DataFusion directly for simple queries
        let session = session_factory.create_session();
        let df = session.sql(sql).await?;
        let batches = df.collect().await?;
        record_batch_to_query_result(batches)
    }
}

/// Convert Arrow RecordBatches to QueryResult
fn record_batch_to_query_result(
    batches: Vec<arrow::record_batch::RecordBatch>,
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

    Ok(QueryResult::with_rows(rows, column_names))
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
