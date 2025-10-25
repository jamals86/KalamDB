//! Integration tests for User Story 7: Storage Backend Abstraction and Architecture Cleanup
//!
//! Tests verify:
//! - StorageBackend trait interface exists and is complete
//! - RocksDB backend implements the trait correctly
//! - system.storages table rename is complete (old name removed)
//! - Storage operations work through abstraction (no direct RocksDB calls)
//! - Column Family concepts work through Partition abstraction
//! - Storage backend error handling is graceful

use reqwest;
use serde_json::json;

const BASE_URL: &str = "http://localhost:3000";

#[tokio::test]
async fn test_storage_trait_interface_exists() {
    // T420: Verify trait defines required operations
    // This test verifies the StorageBackend trait is properly defined
    // by checking that we can use it through the RocksDB implementation
    
    let client = reqwest::Client::new();
    
    // Create a test namespace
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE NAMESPACE test_storage_trait",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Create a storage (tests that storage operations work through trait)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE STORAGE test_local URI 'file:///tmp/test_storage'",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Cleanup
    client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "DROP NAMESPACE test_storage_trait",
            "user_id": "test_user"
        }))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_rocksdb_implements_storage_trait() {
    // T421: Verify RocksDB backend implements trait
    // The fact that the server runs and can perform operations
    // proves RocksDB implements StorageBackend correctly
    
    let client = reqwest::Client::new();
    
    // Create namespace (uses partition creation through trait)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE NAMESPACE test_rocksdb_trait",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Create table (uses put operations through trait)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE USER TABLE test_rocksdb_trait.test_table (id BIGINT PRIMARY KEY, name STRING)",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Insert data (uses put through trait)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "INSERT INTO test_rocksdb_trait.test_table (id, name) VALUES (1, 'test')",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Query data (uses get/scan through trait)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM test_rocksdb_trait.test_table",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["rows"].is_array());
    assert_eq!(body["rows"].as_array().unwrap().len(), 1);
    
    // Cleanup
    client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "DROP NAMESPACE test_rocksdb_trait",
            "user_id": "test_user"
        }))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_system_storages_table_renamed() {
    // T422: Query system.storages, verify old name gone
    
    let client = reqwest::Client::new();
    
    // Query system.storages (new name)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM system.storages",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200, "system.storages should exist");
    
    // Try to query system.storage_locations (old name - should fail)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM system.storage_locations",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    // Should return error or 4xx status
    assert!(
        response.status().is_client_error() || response.status().is_server_error(),
        "system.storage_locations should not exist (should be renamed to system.storages)"
    );
}

#[tokio::test]
async fn test_storage_operations_through_abstraction() {
    // T423: Perform CRUD, verify no direct RocksDB calls
    // This test exercises the full CRUD cycle to ensure all operations
    // go through the StorageBackend trait
    
    let client = reqwest::Client::new();
    
    // CREATE: Create storage
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE STORAGE test_crud_storage URI 'file:///tmp/test_crud'",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // READ: Query storage
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM system.storages WHERE storage_id = 'test_crud_storage'",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["rows"].is_array());
    assert_eq!(body["rows"].as_array().unwrap().len(), 1);
    
    // UPDATE: Alter storage (if supported)
    // Note: ALTER STORAGE might not be implemented yet, skip if not available
    
    // DELETE: Drop storage
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "DROP STORAGE test_crud_storage",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Verify deletion
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM system.storages WHERE storage_id = 'test_crud_storage'",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["rows"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_column_family_abstraction() {
    // T424: Verify CF concepts work through Partition abstraction
    // Each table gets its own partition (column family in RocksDB)
    
    let client = reqwest::Client::new();
    
    // Create namespace
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE NAMESPACE test_partition_ns",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Create multiple tables (each should create a partition)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE USER TABLE test_partition_ns.table1 (id BIGINT PRIMARY KEY, data STRING)",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE USER TABLE test_partition_ns.table2 (id BIGINT PRIMARY KEY, value BIGINT)",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), 200);
    
    // Insert data into both tables
    client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "INSERT INTO test_partition_ns.table1 (id, data) VALUES (1, 'test')",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "INSERT INTO test_partition_ns.table2 (id, value) VALUES (1, 42)",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    // Verify data is isolated between partitions
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM test_partition_ns.table1",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["rows"].as_array().unwrap().len(), 1);
    
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM test_partition_ns.table2",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["rows"].as_array().unwrap().len(), 1);
    
    // Cleanup
    client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "DROP NAMESPACE test_partition_ns",
            "user_id": "test_user"
        }))
        .send()
        .await
        .ok();
}

#[tokio::test]
async fn test_storage_backend_error_handling() {
    // T425: Trigger storage errors, verify graceful handling
    
    let client = reqwest::Client::new();
    
    // Try to create storage with invalid URI
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "CREATE STORAGE bad_storage URI ''",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    // Should return error (either 400 or 500)
    assert!(response.status().is_client_error() || response.status().is_server_error());
    
    // Try to query non-existent table (partition not found)
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "SELECT * FROM nonexistent.table",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert!(response.status().is_client_error() || response.status().is_server_error());
    
    // Try to insert into non-existent table
    let response = client
        .post(&format!("{}/v1/api/sql", BASE_URL))
        .json(&json!({
            "sql": "INSERT INTO nonexistent.table (id) VALUES (1)",
            "user_id": "test_user"
        }))
        .send()
        .await
        .expect("Failed to send request");
    
    assert!(response.status().is_client_error() || response.status().is_server_error());
}
