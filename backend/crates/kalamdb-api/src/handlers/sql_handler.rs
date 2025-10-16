//! SQL execution handler for the `/api/sql` REST API endpoint
//!
//! This module provides HTTP handlers for executing SQL statements via the REST API.

use actix_web::{post, web, HttpResponse, Responder};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use std::sync::Arc;
use std::time::Instant;

use crate::models::{QueryResult, SqlRequest, SqlResponse};

/// POST /api/sql - Execute SQL statement(s)
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
#[post("/api/sql")]
pub async fn execute_sql(
    req: web::Json<SqlRequest>,
    session_factory: web::Data<Arc<DataFusionSessionFactory>>,
) -> impl Responder {
    let start_time = Instant::now();
    
    // Split SQL by semicolons to handle multiple statements
    let statements: Vec<&str> = req.sql
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
    
    for (idx, sql) in statements.iter().enumerate() {
        match execute_single_statement(sql, &session_factory).await {
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

/// Execute a single SQL statement using DataFusion
async fn execute_single_statement(
    sql: &str,
    session_factory: &Arc<DataFusionSessionFactory>,
) -> Result<QueryResult, Box<dyn std::error::Error>> {
    // Create a DataFusion session (TODO: pass user context)
    let ctx = session_factory.create_session();
    
    // Execute the SQL query
    let df = ctx.sql(sql).await?;
    
    // Collect results into Arrow batches
    let batches = df.collect().await?;
    
    if batches.is_empty() {
        // DDL statements or queries with no results
        return Ok(QueryResult::with_message(format!("Query executed successfully")));
    }
    
    // Convert Arrow batches to JSON rows
    let schema = batches[0].schema();
    let column_names: Vec<String> = schema
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    
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
                let json_row: std::collections::HashMap<String, serde_json::Value> = serde_json::from_str(line)?;
                rows.push(json_row);
            }
        }
    }
    
    Ok(QueryResult::with_rows(rows, column_names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    
    #[actix_rt::test]
    async fn test_execute_sql_endpoint() {
        // Create a test session factory
        let session_factory = Arc::new(DataFusionSessionFactory::new().unwrap());
        
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .service(execute_sql)
        ).await;
        
        let req_body = SqlRequest {
            sql: "SELECT 1 as id, 'Alice' as name".to_string(),
        };
        
        let req = test::TestRequest::post()
            .uri("/api/sql")
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
        
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .service(execute_sql)
        ).await;
        
        let req_body = SqlRequest {
            sql: "".to_string(),
        };
        
        let req = test::TestRequest::post()
            .uri("/api/sql")
            .set_json(&req_body)
            .to_request();
        
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }
    
    #[actix_rt::test]
    async fn test_execute_sql_multiple_statements() {
        let session_factory = Arc::new(DataFusionSessionFactory::new().unwrap());
        
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(session_factory))
                .service(execute_sql)
        ).await;
        
        let req_body = SqlRequest {
            sql: "SELECT 1 as id; SELECT 2 as id".to_string(),
        };
        
        let req = test::TestRequest::post()
            .uri("/api/sql")
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
