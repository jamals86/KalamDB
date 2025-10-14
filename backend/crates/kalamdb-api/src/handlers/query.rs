// Query handler - SQL-only API
use actix_web::{web, HttpResponse, Responder};
use kalamdb_core::sql::{SqlParser, SqlResult};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;

/// Shared application state for SQL executor
pub struct AppState {
    pub sql_executor: Arc<kalamdb_core::sql::SqlExecutor>,
}

/// Request body for SQL query endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct SqlQueryRequest {
    /// The SQL statement to execute
    pub sql: String,
}

/// Response for INSERT queries
#[derive(Debug, Serialize)]
pub struct InsertResponse {
    /// Number of rows affected
    #[serde(rename = "rowsAffected")]
    pub rows_affected: usize,
    /// ID of the inserted message
    #[serde(rename = "insertedId")]
    pub inserted_id: i64,
}

/// Response for SELECT queries
#[derive(Debug, Serialize)]
pub struct SelectResponse {
    /// Column names
    pub columns: Vec<String>,
    /// Row data
    pub rows: Vec<Vec<JsonValue>>,
    /// Number of rows returned
    #[serde(rename = "rowCount")]
    pub row_count: usize,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Handler for POST /api/v1/query endpoint (SQL-only)
/// 
/// Accepts SQL INSERT and SELECT statements and executes them against the message store.
/// 
/// # Examples
/// 
/// INSERT:
/// ```json
/// {
///   "sql": "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello')"
/// }
/// ```
/// 
/// SELECT:
/// ```json
/// {
///   "sql": "SELECT * FROM messages WHERE conversation_id = 'conv_123' LIMIT 50"
/// }
/// ```
pub async fn query_messages(
    req: web::Json<SqlQueryRequest>,
    state: web::Data<AppState>,
) -> impl Responder {
    let start_time = Instant::now();
    let sql = req.sql.trim();
    
    // Log the incoming SQL query
    debug!("Received SQL query: {}", sql);
    
    // Validate SQL is not empty
    if sql.is_empty() {
        error!("Empty SQL query received");
        return HttpResponse::BadRequest().json(ErrorResponse {
            error: "Invalid query".to_string(),
            message: Some("SQL query cannot be empty".to_string()),
        });
    }
    
    // Parse SQL statement
    let statement = match SqlParser::parse(sql) {
        Ok(stmt) => stmt,
        Err(e) => {
            error!("SQL parsing failed: {}", e);
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "Invalid SQL syntax".to_string(),
                message: Some(e.to_string()),
            });
        }
    };
    
    // Execute the statement
    match state.sql_executor.execute(statement) {
        Ok(result) => {
            let elapsed = start_time.elapsed();
            
            match result {
                SqlResult::Insert { rows_affected, inserted_id } => {
                    info!(
                        "INSERT completed: inserted_id={}, took {:?}",
                        inserted_id, elapsed
                    );
                    
                    HttpResponse::Ok().json(InsertResponse {
                        rows_affected,
                        inserted_id,
                    })
                }
                SqlResult::Select { columns, rows, row_count } => {
                    info!(
                        "SELECT completed: returned {} rows, took {:?}",
                        row_count, elapsed
                    );
                    
                    HttpResponse::Ok().json(SelectResponse {
                        columns,
                        rows,
                        row_count,
                    })
                }
            }
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            error!("SQL execution failed after {:?}: {}", elapsed, e);
            
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Query execution failed".to_string(),
                message: Some(e.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;
    use kalamdb_core::ids::SnowflakeGenerator;
    use kalamdb_core::sql::SqlExecutor;
    use kalamdb_core::storage::RocksDbStore;
    use serde_json::json;
    use std::sync::Arc;

    #[actix_web::test]
    async fn test_sql_insert_handler() {
        let test_dir = format!("./data/test_sql_insert_handler_{}", std::process::id());
        let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = Arc::new(SqlExecutor::new(store.clone(), id_gen, 1_000_000));

        let state = web::Data::new(AppState {
            sql_executor: executor,
        });

        let req_body = SqlQueryRequest {
            sql: "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello')".to_string(),
        };

        let req = test::TestRequest::post()
            .set_json(&req_body)
            .to_http_request();

        let resp = query_messages(web::Json(req_body), state).await.respond_to(&req);

        assert!(resp.status().is_success());

        // Cleanup
        drop(store);
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[actix_web::test]
    async fn test_sql_select_handler() {
        let test_dir = format!("./data/test_sql_select_handler_{}", std::process::id());
        let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = Arc::new(SqlExecutor::new(store.clone(), id_gen, 1_000_000));

        let state = web::Data::new(AppState {
            sql_executor: executor.clone(),
        });

        // Insert a message first
        let insert_req = SqlQueryRequest {
            sql: "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_test', 'bob', 'Test')".to_string(),
        };
        let req = test::TestRequest::post().to_http_request();
        let _resp = query_messages(web::Json(insert_req), state.clone()).await.respond_to(&req);

        // Now select it
        let select_req = SqlQueryRequest {
            sql: "SELECT * FROM messages WHERE conversation_id = 'conv_test'".to_string(),
        };
        let req = test::TestRequest::post().to_http_request();
        let resp = query_messages(web::Json(select_req), state).await.respond_to(&req);

        assert!(resp.status().is_success());

        // Cleanup
        drop(store);
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[actix_web::test]
    async fn test_sql_invalid_syntax() {
        let test_dir = format!("./data/test_sql_invalid_handler_{}", std::process::id());
        let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = Arc::new(SqlExecutor::new(store.clone(), id_gen, 1_000_000));

        let state = web::Data::new(AppState {
            sql_executor: executor,
        });

        let req_body = SqlQueryRequest {
            sql: "INVALID SQL".to_string(),
        };

        let req = test::TestRequest::post().to_http_request();
        let resp = query_messages(web::Json(req_body), state).await.respond_to(&req);

        assert_eq!(resp.status(), 400);

        // Cleanup
        drop(store);
        let _ = std::fs::remove_dir_all(&test_dir);
    }
}
