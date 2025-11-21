//! Integration tests focused on flush job behaviour for different table types.

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::sync::Arc;

/// Manual flush on a user table should create Parquet files under the configured storage path.
#[actix_web::test]
async fn test_user_table_manual_flush_creates_parquet() {
    let server = TestServer::new().await;
    let namespace = "flush_ops_user";
    let table_name = "telemetry";
    let user_id = "user_ops_001";

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            metric TEXT,
            value DOUBLE
        ) FLUSH ROWS 5",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create table: {:?}",
        response
    );

    server
        .sql_executor
        .load_existing_tables()
        .await
        .expect("failed to register tables");

    for i in 0..5 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, metric, value) VALUES ({}, 'cpu', {})",
            namespace,
            table_name,
            i,
            0.5 + (i as f64)
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Insert failed: {:?}",
            response.error
        );
    }

    let count_sql = format!("SELECT COUNT(*) AS cnt FROM {}.{}", namespace, table_name);
    let count_response = server.execute_sql_as_user(&count_sql, user_id).await;
    assert_eq!(
        count_response.status,
        ResponseStatus::Success,
        "Count query failed: {:?}",
        count_response.error
    );
    let row_count = count_response
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("cnt"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    assert_eq!(row_count, 5, "Expected 5 rows after inserts");

    {
        use kalamdb_commons::models::{
            NamespaceId as ModelNamespaceId, TableName as ModelTableName,
        };
        use kalamdb_tables::UserTableStoreExt;

        // Use the SAME backend as AppContext to ensure consistency
        let backend = server.app_context.storage_backend();
        let model_namespace = ModelNamespaceId::new(namespace);
        let model_table = ModelTableName::new(table_name);
        let store = kalamdb_tables::new_user_table_store(backend, &model_namespace, &model_table);
        use kalamdb_store::entity_store::EntityStore;
        let buffered_rows =
            EntityStore::scan_all(&store, None, None, None).expect("scan_all() should succeed");
        assert_eq!(
            buffered_rows.len(),
            5,
            "Expected buffered rows before flush"
        );
    }

    let before = flush_helpers::check_user_parquet_files(&server, namespace, table_name, user_id);
    assert!(
        before.is_empty(),
        "Parquet files should not exist before flush"
    );

    let flush_sql = format!("FLUSH TABLE {}.{}", namespace, table_name);
    let flush_response = server.execute_sql_as_user(&flush_sql, "system").await;
    assert_eq!(
        flush_response.status,
        ResponseStatus::Success,
        "FLUSH TABLE failed: {:?}",
        flush_response.error
    );

    let job_id_message = flush_response
        .results
        .first()
        .and_then(|r| r.message.as_deref())
        .unwrap_or("");
    let job_id =
        flush_helpers::extract_job_id(job_id_message).expect("FLUSH TABLE should return a job id");

    let job_result = flush_helpers::wait_for_flush_job_completion(
        &server,
        &job_id,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("flush job should complete");
    // Accept both legacy and new result formats. Examples:
    // - "Parquet files written: 1 (5 rows)"
    // - "Flushed <ns>.<table> successfully (5 rows, 1 files)"
    let jr_lower = job_result.to_lowercase();
    let has_files_phrase = jr_lower.contains("parquet files");
    let looks_successful = jr_lower.contains("successfully")
        && jr_lower.contains("rows")
        && (jr_lower.contains("file") || jr_lower.contains("files"));
    assert!(
        has_files_phrase || looks_successful,
        "Unexpected flush job result: {}",
        job_result
    );
    assert!(
        !jr_lower.contains("parquet files: 0") && !jr_lower.contains("0 files"),
        "Flush did not produce Parquet files: {}",
        job_result
    );

    let parquet_files = flush_helpers::wait_for_parquet_files(
        &server,
        namespace,
        table_name,
        user_id,
        std::time::Duration::from_secs(5),
        1,
    )
    .await
    .expect("Parquet files should appear after flush");
    flush_helpers::verify_parquet_files_exist(&parquet_files, 1, &job_result)
        .expect("Parquet files should exist after flush");
}

/// Manual flush on a shared table should also generate Parquet output.
#[actix_web::test]
async fn test_shared_table_manual_flush_creates_parquet() {
    let server = TestServer::new().await;
    let namespace = "flush_ops_shared";
    let table_name = "audit_events";

    fixtures::create_namespace(&server, namespace).await;
    let create_resp = fixtures::create_shared_table(&server, namespace, table_name).await;

    server
        .sql_executor
        .load_existing_tables()
        .await
        .expect("failed to register tables");
    assert_eq!(
        create_resp.status,
        ResponseStatus::Success,
        "Failed to create shared table: {:?}",
        create_resp.error
    );

    for i in 0..8 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (conversation_id, title, status, participant_count, created_at) \
             VALUES ('conv-{}', 'title-{}', 'open', {}, NOW())",
            namespace,
            table_name,
            i,
            i,
            2 + (i % 3)
        );
        let response = server.execute_sql_as_user(&insert_sql, "system").await;
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Insert failed: {:?}",
            response.error
        );
    }

    let before = flush_helpers::check_shared_parquet_files(&server, namespace, table_name);
    assert!(
        before.is_empty(),
        "No Parquet files should exist before flush"
    );

    let flush_result =
        flush_helpers::execute_shared_flush_synchronously(&server, namespace, table_name)
            .await
            .expect("shared table flush should succeed");
    assert_eq!(flush_result.rows_flushed, 8);
    assert!(
        !flush_result.parquet_files.is_empty(),
        "Shared flush returned no files: {:?}",
        flush_result
    );

    let after = flush_helpers::check_shared_parquet_files(&server, namespace, table_name);
    flush_helpers::verify_parquet_files_exist(&after, 1, "shared table manual flush")
        .expect("Parquet files should exist after shared table flush");
}
