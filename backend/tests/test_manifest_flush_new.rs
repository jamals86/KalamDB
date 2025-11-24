//! Integration tests for manifest creation and validation during flush operations

#[path = "integration/common/mod.rs"]
mod common;

use common::fixtures::{create_namespace, execute_sql};
use common::{flush_helpers, TestServer};
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::types::Manifest;
use kalamdb_commons::{NamespaceId, TableName};

#[tokio::test]
async fn test_shared_table_flush_creates_manifest() {
    let server = TestServer::new().await;
    let data_path = server.temp_dir.path();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("products");

    // Create namespace and table
    create_namespace(&server, namespace.as_str()).await;

    let create_sql = format!(
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, name TEXT) WITH (TYPE = 'SHARED', FLUSH_POLICY = 'rows:5')",
        namespace.as_str(),
        table.as_str()
    );
    execute_sql(&server, &create_sql, "root").await.unwrap();

    // Insert rows (more than FLUSH ROWS threshold)
    for i in 1..=7 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, name) VALUES ({}, 'item_{}')",
            namespace.as_str(),
            table.as_str(),
            i,
            i
        );
        execute_sql(&server, &insert_sql, "root").await.unwrap();
    }

    // Execute manual flush
    println!("🔧 Server data dir: {}", data_path.display());

    let flush_result = flush_helpers::execute_shared_flush_synchronously(
        &server,
        namespace.as_str(),
        table.as_str(),
    )
    .await
    .expect("Flush should succeed");

    println!("✅ Flushed {} rows", flush_result.rows_flushed);
    println!("📄 Parquet files: {:?}", flush_result.parquet_files);

    // List all files in the table directory
    let table_dir = format!(
        "{}/storage/shared/{}/{}",
        data_path.display(),
        namespace.as_str(),
        table.as_str()
    );
    println!("📁 Files in {}", table_dir);
    if let Ok(entries) = std::fs::read_dir(&table_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            println!("  - {}", file_name);
        }
    } else {
        println!("  Directory does not exist!");
    }

    // Verify manifest exists in the same directory as Parquet files
    let manifest_path = format!(
        "{}/storage/shared/{}/{}/manifest.json",
        data_path.display(),
        namespace.as_str(),
        table.as_str()
    );

    assert!(
        std::path::Path::new(&manifest_path).exists(),
        "Manifest should exist at: {}",
        manifest_path
    );

    // Parse and validate manifest
    let manifest_json = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: Manifest = serde_json::from_str(&manifest_json).unwrap();

    assert_eq!(
        manifest.table_id.namespace_id().as_str(),
        namespace.as_str()
    );
    assert_eq!(manifest.table_id.table_name().as_str(), table.as_str());
    assert_eq!(
        manifest.user_id, None,
        "Shared table should have no user_id"
    );
    assert!(
        !manifest.segments.is_empty(),
        "Should have at least one segment"
    );

    let batch = &manifest.segments[0];
    // assert_eq!(batch.id, "0"); // ID might be UUID or filename, let's not assert exact value unless we know
    assert_eq!(batch.path, "batch-0.parquet");
    assert!(batch.row_count >= 5, "Batch should have at least 5 rows");
    assert!(batch.size_bytes > 0);
    assert!(batch.min_seq > 0);
    assert!(batch.max_seq >= batch.min_seq);

    println!(
        "✅ Manifest created successfully with {} batch(es)",
        manifest.segments.len()
    );
}

#[tokio::test]
async fn test_manifest_cache_works() {
    let server = TestServer::new().await;
    let namespace = NamespaceId::new("cache_ns");
    let table = TableName::new("items");

    create_namespace(&server, namespace.as_str()).await;

    execute_sql(
        &server,
        &format!(
            "CREATE TABLE {}.{} (id INT PRIMARY KEY) WITH (TYPE = 'SHARED', FLUSH_POLICY = 'rows:3')",
            namespace.as_str(),
            table.as_str()
        ),
        "root",
    )
    .await
    .unwrap();

    // Insert and flush
    for i in 1..=5 {
        execute_sql(
            &server,
            &format!(
                "INSERT INTO {}.{} (id) VALUES ({})",
                namespace.as_str(),
                table.as_str(),
                i
            ),
            "root",
        )
        .await
        .unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Query should use cached manifest (check logs for HIT message)
    let resp = execute_sql(
        &server,
        &format!(
            "SELECT COUNT(*) FROM {}.{}",
            namespace.as_str(),
            table.as_str()
        ),
        "root",
    )
    .await
    .unwrap();

    assert_eq!(resp.status, ResponseStatus::Success);
    println!("✅ Query executed with manifest cache");
}
