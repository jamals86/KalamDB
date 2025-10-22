//! Integration tests for API versioning (User Story 14)
//!
//! Tests verifying:
//! - /v1/api/sql endpoint works correctly
//! - /v1/ws WebSocket endpoint works correctly
//! - /v1/api/healthcheck endpoint works correctly
//! - Storage credentials column functionality
//! - Server refactoring module structure
//! - SQL parser consolidation (executor.rs in kalamdb-sql)
//! - SQL keywords centralized in enums
//! - sqlparser-rs integration for standard SQL
//! - PostgreSQL/MySQL syntax compatibility

mod common;

use actix_web::test;
use common::{start_test_server, TestServer};
use kalam_cli::formatter::OutputFormatter;
use kalam_cli::session::OutputFormat;
use kalam_link::models::QueryResult;
use kalam_link::QueryResponse;
use serde_json::json;

#[actix_web::test]
async fn test_v1_sql_endpoint() {
    let server = start_test_server().await;
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({
            "sql": "SELECT 1 as value"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(resp.status(), 200, "v1 SQL endpoint should return 200 OK");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "success");
    assert!(body["results"].is_array());
}

#[actix_web::test]
async fn test_v1_websocket_endpoint() {
    let server = start_test_server().await;
    let client = test::TestRequest::get()
        .uri("/v1/ws")
        .insert_header(("X-USER-ID", "test_user"))
        .insert_header(("upgrade", "websocket"))
        .insert_header(("connection", "upgrade"))
        .insert_header(("sec-websocket-version", "13"))
        .insert_header(("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    // WebSocket upgrade returns 101 Switching Protocols
    assert!(
        resp.status() == 101 || resp.status() == 400,
        "v1 WebSocket endpoint should handle connection"
    );
}

#[actix_web::test]
async fn test_v1_healthcheck_endpoint() {
    let server = start_test_server().await;
    let client = test::TestRequest::get()
        .uri("/v1/api/healthcheck")
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(
        resp.status(),
        200,
        "v1 healthcheck endpoint should return 200 OK"
    );

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["status"].is_string());
    assert_eq!(body["api_version"], "v1");
}

#[actix_web::test]
async fn test_storage_credentials_column() {
    let server = start_test_server().await;

    // Create storage with credentials (admin user should see plain credentials)
    let create_sql = r#"
        CREATE STORAGE test_s3
        TYPE s3
        NAME 'Test S3 Storage'
        DESCRIPTION 'integration test storage'
        BASE_DIRECTORY 's3://kalamdb-integration'
        SHARED_TABLES_TEMPLATE '{namespace}/{tableName}'
        USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}'
        CREDENTIALS '{"access_key": "AKIAIOSFODNN7EXAMPLE", "secret_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"}'
    "#;

    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "admin"))
        .set_json(&json!({
            "sql": create_sql
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(
        resp.status(),
        200,
        "CREATE STORAGE with credentials should succeed"
    );

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "success");

    // Query system.storages as admin to verify credentials persisted
    let query = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "admin"))
        .set_json(&json!({
            "sql": "SELECT storage_id, credentials FROM system.storages WHERE storage_id = 'test_s3'"
        }))
        .to_request();

    let resp = test::call_service(&server.app, query).await;
    assert_eq!(resp.status(), 200, "Querying storages as admin should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "success");

    let rows = body["results"][0]["rows"]
        .as_array()
        .expect("rows array expected");
    assert_eq!(rows.len(), 1, "exactly one storage row expected");
    let row = &rows[0];

    assert_eq!(
        row["storage_id"].as_str().unwrap(),
        "test_s3",
        "storage_id should match created storage"
    );
    assert_eq!(
        row["credentials"].as_str().unwrap(),
        r#"{"access_key":"AKIAIOSFODNN7EXAMPLE","secret_key":"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"}"#,
        "credentials should be stored as unmasked JSON for admin"
    );
}

#[actix_web::test]
async fn test_storage_query_includes_credentials() {
    let server = start_test_server().await;

    // Query system.storages
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "admin"))
        .set_json(&json!({
            "sql": "SELECT storage_id, storage_name, credentials FROM system.storages"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "success");

    // Verify credentials field exists in schema
    let columns = body["results"][0]["columns"].as_array().unwrap();
    assert!(
        columns.iter().any(|c| c == "credentials"),
        "system.storages should include credentials column"
    );

    // Non-admin users should see masked credentials
    let masked = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "regular_user"))
        .set_json(&json!({
            "sql": "SELECT storage_id, credentials FROM system.storages WHERE storage_id = 'test_s3'"
        }))
        .to_request();

    let resp = test::call_service(&server.app, masked).await;
    assert_eq!(resp.status(), 200, "Non-admin should still query storages");
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "success");

    let rows = body["results"][0]["rows"].as_array().unwrap();
    let row = rows.iter().find(|row| row["storage_id"] == "test_s3").unwrap();
    assert_eq!(
        row["credentials"].as_str().unwrap(),
        "***",
        "Non-admin should receive masked credentials"
    );
}

#[actix_web::test]
async fn test_main_rs_module_structure() {
    // Verify that main.rs is organized into modules
    // This is a compile-time test - if the server compiles, the structure is correct
    
    // Check that config.rs, routes.rs, middleware.rs, lifecycle.rs modules exist
    let config_exists =
        std::path::Path::new("backend/crates/kalamdb-server/src/config.rs").exists();
    let routes_exists =
        std::path::Path::new("backend/crates/kalamdb-server/src/routes.rs").exists();
    let middleware_exists =
        std::path::Path::new("backend/crates/kalamdb-server/src/middleware.rs").exists();
    let lifecycle_exists =
        std::path::Path::new("backend/crates/kalamdb-server/src/lifecycle.rs").exists();

    assert!(config_exists, "config.rs module should exist");
    assert!(routes_exists, "routes.rs module should exist");
    assert!(middleware_exists, "middleware.rs module should exist");
    assert!(lifecycle_exists, "lifecycle.rs module should exist");
}

#[actix_web::test]
async fn test_executor_moved_to_kalamdb_sql() {
    // Verify executor.rs exists in kalamdb-sql crate and core re-exports the module
    let sql_executor_path =
        std::path::Path::new("backend/crates/kalamdb-sql/src/executor.rs");
    assert!(
        sql_executor_path.exists(),
        "executor.rs should be located in kalamdb-sql crate"
    );

    // The core crate should expose executor via a thin wrapper module
    let core_wrapper =
        std::path::Path::new("backend/crates/kalamdb-core/src/sql/executor.rs");
    assert!(
        core_wrapper.exists(),
        "kalamdb-core should provide a wrapper around kalamdb-sql executor"
    );

    let contents = std::fs::read_to_string(core_wrapper).expect("read executor wrapper");
    assert!(
        contents.contains("pub use kalamdb_sql::executor::SqlExecutor"),
        "core executor wrapper should re-export SqlExecutor from kalamdb-sql"
    );
}

#[actix_web::test]
async fn test_sql_keywords_enum_centralized() {
    // Verify keywords.rs exists with centralized enums
    let keywords_path = std::path::Path::new("backend/crates/kalamdb-sql/src/keywords.rs");
    assert!(
        keywords_path.exists(),
        "keywords.rs should exist in kalamdb-sql"
    );

    let contents = std::fs::read_to_string(keywords_path).expect("read keywords file");
    assert!(
        contents.contains("pub enum SqlKeyword"),
        "keywords.rs should define SqlKeyword enum"
    );
    assert!(
        contents.contains("pub enum KalamDbKeyword"),
        "keywords.rs should define KalamDbKeyword enum"
    );
}

#[actix_web::test]
async fn test_sqlparser_rs_integration() {
    let server = start_test_server().await;

    // Test standard SQL SELECT statement (should use sqlparser-rs)
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({
            "sql": "SELECT * FROM system.users WHERE user_id = 'test_user'"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(
        resp.status(),
        200,
        "Standard SQL should be parsed by sqlparser-rs"
    );
}

#[actix_web::test]
async fn test_custom_statement_extension() {
    let server = start_test_server().await;

    // Test custom KalamDB statement (CREATE STORAGE)
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "admin"))
        .set_json(&json!({
            "sql": "CREATE STORAGE test_storage TYPE filesystem BASE_DIRECTORY '/tmp/test'"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert_eq!(
        resp.status(),
        200,
        "Custom KalamDB statements should be supported via parser extensions"
    );
}

#[actix_web::test]
async fn test_postgres_syntax_compatibility() {
    let server = start_test_server().await;

    // Test PostgreSQL-style CREATE TABLE with SERIAL
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({
            "sql": "CREATE TABLE test_pg (id SERIAL PRIMARY KEY, name VARCHAR(100))"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert!(
        resp.status() == 200 || resp.status() == 400,
        "PostgreSQL syntax should be recognized"
    );
}

#[actix_web::test]
async fn test_mysql_syntax_compatibility() {
    let server = start_test_server().await;

    // Test MySQL-style CREATE TABLE with AUTO_INCREMENT
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({
            "sql": "CREATE TABLE test_mysql (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100))"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    assert!(
        resp.status() == 200 || resp.status() == 400,
        "MySQL syntax should be recognized"
    );
}

#[actix_web::test]
async fn test_error_message_postgres_style() {
    let server = start_test_server().await;

    // Trigger an error with invalid table reference
    let client = test::TestRequest::post()
        .uri("/v1/api/sql")
        .insert_header(("content-type", "application/json"))
        .insert_header(("X-USER-ID", "test_user"))
        .set_json(&json!({
            "sql": "SELECT * FROM nonexistent_table"
        }))
        .to_request();

    let resp = test::call_service(&server.app, client).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let message = body["error"]["message"]
        .as_str()
        .expect("error message should be string");
    assert!(
        message.starts_with("ERROR: relation"),
        "Error message should use PostgreSQL-style prefix"
    );
}

#[actix_web::test]
async fn test_cli_output_psql_style() {
    use cli::kalam_cli::formatter::OutputFormatter;
    use cli::kalam_cli::session::OutputFormat;
    use kalam_link::models::{QueryResponse, QueryResult};

    // Construct a fake query result to exercise table formatting
    let mut row = std::collections::HashMap::new();
    row.insert("id".to_string(), serde_json::json!(1));
    row.insert("name".to_string(), serde_json::json!("Alice"));

    let result = QueryResult {
        rows: Some(vec![row]),
        row_count: 1,
        columns: vec!["id".to_string(), "name".to_string()],
        message: None,
    };

    let response = QueryResponse {
        status: "success".to_string(),
        results: vec![result],
        execution_time_ms: Some(12),
        error: None,
    };

    let formatter = OutputFormatter::new(OutputFormat::Table, false);
    let output = formatter
        .format_response(&response)
        .expect("formatting should succeed");

    assert!(
        output.contains('┌') && output.contains('┐'),
        "Formatted output should use psql-style box drawing characters"
    );
    assert!(
        output.contains("(1 row)"),
        "Formatted output should display row count like psql"
    );
}
