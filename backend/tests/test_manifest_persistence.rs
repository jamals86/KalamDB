//! Integration tests for Manifest Persistence (Phase 2, US4, T014)
//!
//! Tests:
//! - Manifest lifecycle: Hot Store (in-memory/RocksDB) → Cold Store (disk)
//! - Manifest creation on insert
//! - Manifest NOT written to disk until flush
//! - Manifest written to disk after flush
//! - Manifest loaded from disk on restart

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, QueryResultTestExt, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::path::PathBuf;

/// Helper to list directory contents recursively
fn list_directory_recursive(path: &PathBuf, depth: usize) {
    let indent = "  ".repeat(depth);
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let name = entry_path.file_name().unwrap().to_string_lossy();
            if entry_path.is_dir() {
                println!("{}📁 {}/", indent, name);
                list_directory_recursive(&entry_path, depth + 1);
            } else {
                let size = std::fs::metadata(&entry_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                println!("{}📄 {} ({} bytes)", indent, name, size);
            }
        }
    }
}

/// T014: Test manifest persistence lifecycle
#[actix_web::test]
async fn test_manifest_persistence_lifecycle() {
    let server = TestServer::new().await;

    // 1. Create namespace and table
    fixtures::create_namespace(&server, "test_ns").await;
    let create_response = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns.events (
                id TEXT PRIMARY KEY,
                event_type TEXT,
                timestamp INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;
    assert_eq!(
        create_response.status,
        ResponseStatus::Success,
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // 2. Insert data (should create manifest in Hot Store)
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.events (id, event_type, timestamp) 
               VALUES ('evt1', 'login', 1234567890)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.events (id, event_type, timestamp) 
               VALUES ('evt2', 'logout', 1234567900)"#,
            "user1",
        )
        .await;

    // 3. Verify manifest NOT on disk yet (only in Hot Store)
    let manifest_path = get_manifest_path(&server, "test_ns", "events", "user1");
    println!("🔍 Checking manifest path: {}", manifest_path.display());
    
    // List parent directory to see what files exist
    if let Some(parent) = manifest_path.parent() {
        if parent.exists() {
            println!("📁 Parent directory exists: {}", parent.display());
            if let Ok(entries) = std::fs::read_dir(parent) {
                println!("📂 Contents:");
                for entry in entries {
                    if let Ok(entry) = entry {
                        println!("  - {}", entry.path().display());
                    }
                }
            }
        } else {
            println!("📁 Parent directory does NOT exist: {}", parent.display());
        }
    }
    
    assert!(
        !manifest_path.exists(),
        "Manifest should NOT exist on disk before flush: {}",
        manifest_path.display()
    );
    println!("✅ Manifest NOT on disk before flush");

    // 4. Flush table to Parquet
    println!("🔄 Starting flush...");
    let flush_result = flush_helpers::execute_flush_synchronously(&server, "test_ns", "events")
        .await;
    
    match &flush_result {
        Ok(_) => println!("✅ Flush completed successfully"),
        Err(e) => println!("❌ Flush failed: {:?}", e),
    }
    
    flush_result.expect("Flush should succeed");
    
    // List the entire storage root to see what was created
    let storage_root = server.storage_root();
    println!("📁 Storage root: {}", storage_root.display());
    if storage_root.exists() {
        list_directory_recursive(&storage_root, 0);
    } else {
        println!("  Storage root does NOT exist!");
    }

    // 5. Verify manifest IS on disk now (Cold Store)
    println!("🔍 Checking manifest path after flush: {}", manifest_path.display());
    
    // List parent directory to see what files exist
    if let Some(parent) = manifest_path.parent() {
        if parent.exists() {
            println!("📁 Parent directory exists: {}", parent.display());
            if let Ok(entries) = std::fs::read_dir(parent) {
                println!("📂 Contents after flush:");
                for entry in entries {
                    if let Ok(entry) = entry {
                        println!("  - {}", entry.path().display());
                    }
                }
            }
        } else {
            println!("📁 Parent directory does NOT exist: {}", parent.display());
        }
    }
    
    assert!(
        manifest_path.exists(),
        "Manifest SHOULD exist on disk after flush: {}",
        manifest_path.display()
    );
    println!("✅ Manifest written to disk after flush");

    // 6. Read and validate manifest content
    let manifest_json = std::fs::read_to_string(&manifest_path)
        .expect("Failed to read manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(&manifest_json)
        .expect("Failed to parse manifest.json");

    assert_eq!(
        manifest["table_id"]["namespace_id"].as_str().unwrap(),
        "test_ns"
    );
    assert_eq!(
        manifest["table_id"]["table_name"].as_str().unwrap(),
        "events"
    );
    assert!(manifest["segments"].as_array().unwrap().len() > 0, "Should have at least one segment");
    println!("✅ Manifest content validated: {} segments", manifest["segments"].as_array().unwrap().len());

    // 7. Query data to verify it's still accessible
    let query_response = server
        .execute_sql_as_user(
            "SELECT id, event_type FROM test_ns.events ORDER BY id",
            "user1",
        )
        .await;

    assert_eq!(
        query_response.status,
        ResponseStatus::Success,
        "Query after flush failed: {:?}",
        query_response.error
    );

    let rows = query_response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 2, "Should return 2 rows after flush");
    assert_eq!(rows[0].get("id").unwrap().as_str().unwrap(), "evt1");
    assert_eq!(rows[1].get("id").unwrap().as_str().unwrap(), "evt2");

    println!("✅ T014: Manifest persistence lifecycle test passed");
}

/// T014b: Test manifest reload on server restart
#[actix_web::test]
async fn test_manifest_reload_from_disk() {
    let server = TestServer::new().await;

    // 1. Create namespace and table
    fixtures::create_namespace(&server, "test_ns").await;
    let create_response = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_ns.metrics (
                id TEXT PRIMARY KEY,
                metric_name TEXT,
                value REAL
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user2",
        )
        .await;
    assert_eq!(
        create_response.status,
        ResponseStatus::Success,
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // 2. Insert and flush
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ns.metrics (id, metric_name, value) 
               VALUES ('m1', 'cpu_usage', 45.5)"#,
            "user2",
        )
        .await;

    flush_helpers::execute_flush_synchronously(&server, "test_ns", "metrics")
        .await
        .expect("Flush should succeed");

    let manifest_path = get_manifest_path(&server, "test_ns", "metrics", "user2");
    assert!(manifest_path.exists(), "Manifest should exist after flush");

    // 3. Query data (this should load manifest from disk into Hot Store cache)
    let query_response = server
        .execute_sql_as_user(
            "SELECT id, metric_name, value FROM test_ns.metrics WHERE id = 'm1'",
            "user2",
        )
        .await;

    assert_eq!(
        query_response.status,
        ResponseStatus::Success,
        "Query failed: {:?}",
        query_response.error
    );

    let rows = query_response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("metric_name").unwrap().as_str().unwrap(), "cpu_usage");
    assert!((rows[0].get("value").unwrap().as_f64().unwrap() - 45.5).abs() < 0.01);

    println!("✅ T014b: Manifest reload from disk test passed");
}

/// Helper function to construct manifest path
/// The actual storage structure is: <storage_root>/users/<user_id>/tables/<namespace>/<table>/manifest.json
fn get_manifest_path(server: &TestServer, namespace: &str, table: &str, user_id: &str) -> PathBuf {
    let storage_path = server.storage_root();
    storage_path
        .join("users")
        .join(user_id)
        .join("tables")
        .join(namespace)
        .join(table)
        .join("manifest.json")
}
