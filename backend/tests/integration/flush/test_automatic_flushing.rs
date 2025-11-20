//! Manual verification of flush behavior for tables with automatic flush policies.
//!
//! The production flush scheduler is not enabled in the test harness yet, so we trigger
//! flushes synchronously and validate that Parquet files are produced at the expected
//! storage paths. These tests ensure that flush metadata (ROW_THRESHOLD / FLUSH INTERVAL)
//! is honoured by the flush job implementation and that data lands on disk.

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_api::models::ResponseStatus;
use kalamdb_store::entity_store::EntityStore;

/// Create a user table with `FLUSH ROWS`, insert data, invoke a manual flush,
/// and verify that Parquet files are created on disk.
#[tokio::test]
async fn test_manual_flush_respects_row_threshold() {
    let server = TestServer::start_test_server().await;
    let namespace = "flush_row_threshold";
    let table_name = "messages";
    let user_id = "user_rt_001";

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            content TEXT,
            created_at TIMESTAMP
        ) FLUSH ROWS 25",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create table: {:?}",
        response
    );

    for i in 0..25 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, content, created_at) VALUES ({}, 'msg-{}', NOW())",
            namespace, table_name, i, i
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Insert failed: {:?}",
            response.error
        );
    }

    let flush_result = flush_helpers::execute_flush_synchronously(&server, namespace, table_name)
        .await
        .expect("manual flush should succeed");
    assert_eq!(flush_result.rows_flushed, 25, "expected to flush all rows");

    let parquet_files =
        flush_helpers::check_user_parquet_files(&server, namespace, table_name, user_id);
    flush_helpers::verify_parquet_files_exist(&parquet_files, 1, "manual flush rows threshold")
        .expect("parquet files should exist after flush");
}

/// Create a table with both `FLUSH ROWS` and `FLUSH INTERVAL` metadata. Trigger multiple
/// rounds of inserts and flushes, ensuring that each flush writes new Parquet files.
#[tokio::test]
async fn test_manual_flush_multiple_batches() {
    let server = TestServer::start_test_server().await;
    let namespace = "flush_multi_batch";
    let table_name = "events";
    let user_id = "user_mb_001";

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            event_id BIGINT,
            category TEXT,
            payload TEXT
        ) FLUSH ROWS 100 INTERVAL 30s",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create table: {:?}",
        response
    );

    for batch in 0..3 {
        for i in 0..10 {
            let event_id = batch * 100 + i;
            let insert_sql = format!(
                "INSERT INTO {}.{} (id, event_id, category, payload) VALUES ({}, {}, 'batch-{}', 'payload-{}')",
                namespace,
                table_name,
                event_id,
                event_id,
                batch,
                event_id
            );
            let response = server.execute_sql_as_user(&insert_sql, user_id).await;
            assert_eq!(
                response.status,
                ResponseStatus::Success,
                "Insert failed: {:?}",
                response.error
            );
        }

        {
            use kalamdb_commons::models::{
                NamespaceId as ModelNamespaceId, TableName as ModelTableName,
            };
            use kalamdb_store::entity_store::EntityStore;
            use kalamdb_tables::UserTableStoreExt;

            // Use the SAME backend as AppContext to ensure consistency
            let backend = server.app_context.storage_backend();
            let model_namespace = ModelNamespaceId::new(namespace);
            let model_table = ModelTableName::new(table_name);
            let store =
                kalamdb_tables::new_user_table_store(backend, &model_namespace, &model_table);
            let buffered_rows =
                EntityStore::scan_all(&store, None, None, None).expect("scan_all should succeed before flush");
            assert_eq!(
                buffered_rows.len(),
                10,
                "expected 10 buffered rows before flush",
            );
        }

        let flush_result =
            flush_helpers::execute_flush_synchronously(&server, namespace, table_name)
                .await
                .expect("flush should succeed");
        assert_eq!(
            flush_result.rows_flushed, 10,
            "each batch flush should move 10 rows: {:?}",
            flush_result
        );
    }

    let parquet_files =
        flush_helpers::check_user_parquet_files(&server, namespace, table_name, user_id);
    flush_helpers::verify_parquet_files_exist(&parquet_files, 1, "manual flush multiple batches")
        .expect("parquet files should exist after repeated flushes");
}
