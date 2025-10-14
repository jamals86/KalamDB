use actix_web::{test, web, App};
use kalamdb_api::handlers::{messages::AppState, messages::create_message, query::query_messages};
use kalamdb_core::ids::SnowflakeGenerator;
use kalamdb_core::storage::RocksDbStore;
use serde_json::json;
use std::sync::Arc;

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
    let app_state = web::Data::new(AppState {
        store: store.clone(),
        id_generator: id_gen,
        max_message_size: 1_000_000,
    });

    App::new()
        .app_data(app_state)
        .app_data(web::Data::new(store))
        .route("/api/v1/messages", web::post().to(create_message))
        .route("/api/v1/query", web::post().to(query_messages))
}

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
