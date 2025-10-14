use actix_web::{test, web, App};
use kalamdb_api::handlers::query::query_messages;
use kalamdb_core::ids::SnowflakeGenerator;
use kalamdb_core::sql::SqlExecutor;
use kalamdb_core::storage::RocksDbStore;
use serde_json::json;
use std::sync::Arc;

// Helper struct for app state (SQL-only version)
pub struct AppState {
    pub sql_executor: Arc<SqlExecutor>,
}

// Helper function to create app with proper state
fn create_test_app(store: Arc<RocksDbStore>) -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let id_gen = Arc::new(SnowflakeGenerator::new(1));
    let sql_executor = Arc::new(SqlExecutor::new(store, id_gen, 1_000_000));
    let app_state = web::Data::new(AppState { sql_executor });

    App::new()
        .app_data(app_state)
        .route("/api/v1/query", web::post().to(query_messages))
}

// T018: API contract test for POST /api/v1/query with SQL INSERT
#[actix_web::test]
async fn test_sql_insert_basic() {
    println!("Test starting...");
    let test_dir = format!("./data/test_sql_insert_basic_{}", std::process::id());
    println!("Test dir: {}", test_dir);
    
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));
    println!("Store created");

    let app = test::init_service(create_test_app(store.clone())).await;
    println!("App initialized");

    // Insert message using SQL
    let insert_query = json!({
        "sql": "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello, world!')"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&insert_query)
        .to_request();
    println!("Request created");
    
    let resp = test::call_service(&app, req).await;
    println!("Response received");

    let status = resp.status();
    let body_bytes = test::read_body(resp).await;
    let body_str = String::from_utf8_lossy(&body_bytes);
    println!("Status: {}", status);
    println!("Body: {}", body_str);
    
    assert!(status.is_success(), "Insert should succeed. Status: {}, Body: {}", status, body_str);

    let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("Failed to parse JSON");
    assert!(body["insertedId"].is_number(), "Should return insertedId");
    assert_eq!(body["rowsAffected"], 1, "Should affect 1 row");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_insert_with_metadata() {
    let test_dir = format!("./data/test_sql_insert_metadata_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert message with metadata using SQL
    let insert_query = json!({
        "sql": r#"INSERT INTO messages (conversation_id, from, content, metadata) VALUES ('conv_123', 'bob', 'Test message', '{"role":"assistant","model":"gpt-4"}')"#
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&insert_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success(), "Insert with metadata should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["insertedId"].is_number());

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_insert_missing_required_field() {
    let test_dir = format!("./data/test_sql_insert_missing_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert message missing required 'content' field
    let insert_query = json!({
        "sql": "INSERT INTO messages (conversation_id, from) VALUES ('conv_123', 'alice')"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&insert_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400, "Should fail with 400 Bad Request");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_insert_invalid_table() {
    let test_dir = format!("./data/test_sql_insert_invalid_table_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Try to insert into non-existent table
    let insert_query = json!({
        "sql": "INSERT INTO users (name) VALUES ('alice')"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&insert_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400, "Should fail with 400 Bad Request for invalid table");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_select_basic() {
    let test_dir = format!("./data/test_sql_select_basic_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert a test message first
    let insert_query = json!({
        "sql": "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_test', 'alice', 'Hello')"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&insert_query)
        .to_request();
    let _resp = test::call_service(&app, req).await;

    // Query messages using SQL SELECT
    let select_query = json!({
        "sql": "SELECT * FROM messages WHERE conversation_id = 'conv_test'"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&select_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success(), "SELECT should succeed");

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["columns"].is_array(), "Should have columns");
    assert!(body["rows"].is_array(), "Should have rows");
    assert_eq!(body["rowCount"], 1, "Should return 1 row");
    
    let rows = body["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_select_with_limit() {
    let test_dir = format!("./data/test_sql_select_limit_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert multiple messages
    for i in 0..10 {
        let insert_query = json!({
            "sql": format!("INSERT INTO messages (conversation_id, from, content) VALUES ('conv_test', 'alice', 'Message {}')", i)
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/query")
            .set_json(&insert_query)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // Query with LIMIT
    let select_query = json!({
        "sql": "SELECT * FROM messages WHERE conversation_id = 'conv_test' LIMIT 5"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&select_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["rowCount"], 5, "Should return exactly 5 rows");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_select_with_order_by() {
    let test_dir = format!("./data/test_sql_select_order_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert messages
    for i in 0..5 {
        let insert_query = json!({
            "sql": format!("INSERT INTO messages (conversation_id, from, content) VALUES ('conv_test', 'alice', 'Message {}')", i)
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/query")
            .set_json(&insert_query)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // Query with ORDER BY DESC
    let select_query = json!({
        "sql": "SELECT * FROM messages WHERE conversation_id = 'conv_test' ORDER BY timestamp DESC"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&select_query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["rowCount"], 5);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_sql_invalid_syntax() {
    let test_dir = format!("./data/test_sql_invalid_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Invalid SQL syntax
    let query = json!({
        "sql": "INVALID SQL STATEMENT"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400, "Should fail with 400 for invalid SQL");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

// Legacy test - keeping structure but adapting for SQL
#[actix_web::test]
async fn test_query_endpoint_basic() {
    let test_dir = format!("./data/test_api_query_basic_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert test message
    let msg1 = json!({
        "conversation_id": "conv_test",
        "from": "alice",
        "content": "Hello",
        "metadata": {}
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/messages")
        .set_json(&msg1)
        .to_request();
    let _resp = test::call_service(&app, req).await;

    // Query messages
    let query = json!({
        "conversation_id": "conv_test"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["messages"].is_array());
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["conversation_id"], "conv_test");
    assert_eq!(body["messages"][0]["from"], "alice");
    assert_eq!(body["messages"][0]["content"], "Hello");

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_with_limit() {
    let test_dir = format!("./data/test_api_query_limit_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert multiple messages
    for i in 0..10 {
        let msg = json!({
            "conversation_id": "conv_test",
            "from": "alice",
            "content": format!("Message {}", i),
            "metadata": {}
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/messages")
            .set_json(&msg)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // Query with limit
    let query = json!({
        "conversation_id": "conv_test",
        "limit": 5
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["messages"].as_array().unwrap().len(), 5);
    assert_eq!(body["count"], 5);
    assert_eq!(body["has_more"], true);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_with_since_msg_id() {
    let test_dir = format!("./data/test_api_query_since_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert messages and capture first message ID
    let msg1 = json!({
        "conversation_id": "conv_test",
        "from": "alice",
        "content": "First message",
        "metadata": {}
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/messages")
        .set_json(&msg1)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let first_msg_id = body["msg_id"].as_i64().unwrap();

    // Insert more messages
    for i in 0..3 {
        let msg = json!({
            "conversation_id": "conv_test",
            "from": "alice",
            "content": format!("Message {}", i),
            "metadata": {}
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/messages")
            .set_json(&msg)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // Query messages after first message
    let query = json!({
        "conversation_id": "conv_test",
        "since_msg_id": first_msg_id
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    
    // Verify all returned message IDs are greater than first_msg_id
    for msg in messages {
        assert!(msg["msg_id"].as_i64().unwrap() > first_msg_id);
    }

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_descending_order() {
    let test_dir = format!("./data/test_api_query_desc_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert messages
    for i in 0..5 {
        let msg = json!({
            "conversation_id": "conv_test",
            "from": "alice",
            "content": format!("Message {}", i),
            "metadata": {}
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/messages")
            .set_json(&msg)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // Query in descending order
    let query = json!({
        "conversation_id": "conv_test",
        "order": "desc"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    let messages = body["messages"].as_array().unwrap();
    
    // Verify messages are in descending order by msg_id
    for i in 0..messages.len().saturating_sub(1) {
        let current_id = messages[i]["msg_id"].as_i64().unwrap();
        let next_id = messages[i + 1]["msg_id"].as_i64().unwrap();
        assert!(current_id > next_id);
    }

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_invalid_limit() {
    let test_dir = format!("./data/test_api_query_invalid_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Query with limit = 0
    let query = json!({
        "limit": 0
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_invalid_order() {
    let test_dir = format!("./data/test_api_query_invalid_order_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Query with invalid order
    let query = json!({
        "order": "invalid"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_empty_result() {
    let test_dir = format!("./data/test_api_query_empty_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Query non-existent conversation
    let query = json!({
        "conversation_id": "nonexistent"
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["messages"].as_array().unwrap().len(), 0);
    assert_eq!(body["count"], 0);
    assert_eq!(body["has_more"], false);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[actix_web::test]
async fn test_query_endpoint_pagination() {
    let test_dir = format!("./data/test_api_query_pagination_{}", std::process::id());
    let store = Arc::new(RocksDbStore::open(&test_dir).expect("Failed to create store"));

    let app = test::init_service(create_test_app(store.clone())).await;

    // Insert 10 messages
    for i in 0..10 {
        let msg = json!({
            "conversation_id": "conv_test",
            "from": "alice",
            "content": format!("Message {}", i),
            "metadata": {}
        });

        let req = test::TestRequest::post()
            .uri("/api/v1/messages")
            .set_json(&msg)
            .to_request();
        let _resp = test::call_service(&app, req).await;
    }

    // First page
    let query = json!({
        "conversation_id": "conv_test",
        "limit": 3
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    
    assert_eq!(body["count"], 3);
    assert_eq!(body["has_more"], true);
    let last_msg_id = body["messages"][2]["msg_id"].as_i64().unwrap();

    // Second page
    let query = json!({
        "conversation_id": "conv_test",
        "since_msg_id": last_msg_id,
        "limit": 3
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/query")
        .set_json(&query)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    
    assert_eq!(body["count"], 3);
    assert_eq!(body["has_more"], true);

    // Cleanup
    drop(store);
    let _ = std::fs::remove_dir_all(&test_dir);
}
