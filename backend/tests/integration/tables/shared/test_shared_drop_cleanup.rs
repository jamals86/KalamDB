//! Integration test (ignored): DROP SHARED TABLE removes RocksDB data and Parquet files
//!
//! Notes: Shared-table tests are ignored by default because some environments
//! require pre-created CFs. The logic here still exercises the drop cleanup path.

#[path = "../../common/mod.rs"]
mod common;

use common::flush_helpers::{check_shared_parquet_files, execute_shared_flush_synchronously};
use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::path::Path;

#[actix_web::test]
async fn test_drop_shared_table_deletes_partitions_and_parquet() {
    let server = TestServer::new().await;

    let namespace = "drop_shared";
    let table = "conversations";

    // Ensure namespace exists and table is clean
    fixtures::create_namespace(&server, namespace).await;
    let _ = server
        .execute_sql_as_user(&format!("DROP TABLE {}.{}", namespace, table), "system")
        .await;

    // Create shared table
    let create_sql = format!(
        r#"CREATE TABLE {}.{} (
            conversation_id TEXT PRIMARY KEY,
            title TEXT,
            participant_count INT
        ) WITH (TYPE='SHARED', STORAGE_ID='local')"#,
        namespace, table
    );
    let resp = server.execute_sql(&create_sql).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "CREATE TABLE failed: {:?}",
        resp.error
    );

    // Insert some rows
    for i in 0..3 {
        let ins = server
            .execute_sql(&format!(
                "INSERT INTO {}.{} (conversation_id, title, participant_count) VALUES ('conv{}','title{}',{})",
                namespace, table, i, i, i + 1
            ))
            .await;
        assert_eq!(ins.status, ResponseStatus::Success);
    }

    // Flush synchronously so Parquet files exist
    let flush_res = execute_shared_flush_synchronously(&server, namespace, table)
        .await
        .expect("Shared flush failed");
    assert!(
        flush_res.rows_flushed > 0,
        "Expected at least 1 row flushed"
    );

    // Verify Parquet exists pre-drop
    let files = check_shared_parquet_files(&server, namespace, table);
    assert!(
        !files.is_empty(),
        "Expected shared Parquet files before drop"
    );
    let shared_dir = files[0]
        .parent()
        .map(|p| p.to_path_buf())
        .expect("No parent dir for shared file");
    assert!(shared_dir.exists(), "Shared dir should exist pre-drop");

    // DROP the table
    let drop_resp = server
        .execute_sql(&format!("DROP TABLE {}.{}", namespace, table))
        .await;
    assert_eq!(
        drop_resp.status,
        ResponseStatus::Success,
        "DROP TABLE failed: {:?}",
        drop_resp.error
    );

    // Verify table metadata removed
    assert!(
        !server.table_exists(namespace, table).await,
        "Table metadata should be removed after drop"
    );

    // Verify Parquet directory removed
    assert!(
        !Path::new(&shared_dir).exists(),
        "Shared Parquet dir still exists after drop: {}",
        shared_dir.display()
    );
}
