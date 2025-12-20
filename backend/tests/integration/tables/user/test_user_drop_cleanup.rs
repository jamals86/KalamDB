//! Integration test: DROP USER TABLE removes RocksDB data and Parquet files
//!
//! - Creates a user table with data for multiple users
//! - Flushes to Parquet and verifies per-user folders/files exist
//! - Drops the table and verifies:
//!   - Table partition is removed from RocksDB
//!   - All per-user Parquet folders under the table are deleted

#[path = "../../common/mod.rs"]
mod common;

use common::flush_helpers::{check_user_parquet_files, execute_flush_synchronously};
use common::{fixtures, TestServer, wait_for_cleanup_job_completion, extract_cleanup_job_id, wait_for_path_absent};
use kalamdb_api::models::ResponseStatus;
use tokio::time::{sleep, Duration};

#[actix_web::test]
async fn test_drop_user_table_deletes_partitions_and_parquet() {
    let server = TestServer::new().await;

    let namespace = "drop_user";
    let table = "notes";
    let user1 = "userA";
    let user2 = "userB";

    // Ensure namespace exists and table is clean
    fixtures::create_namespace(&server, namespace).await;
    let _ = server
        .execute_sql_as_user(&format!("DROP TABLE {}.{}", namespace, table), "system")
        .await;

    // Create user table
    let create_sql = format!(
        r#"CREATE TABLE {}.{} (
            id TEXT PRIMARY KEY,
            content TEXT,
            priority INT
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local'
        )"#,
        namespace, table
    );
    let resp = server.execute_sql_as_user(&create_sql, user1).await;
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "CREATE TABLE failed: {:?}",
        resp.error
    );

    // Insert data for two users
    let ins1 = server
        .execute_sql_as_user(
            &format!(
                "INSERT INTO {}.{} (id, content, priority) VALUES ('a1','hello A1',1)",
                namespace, table
            ),
            user1,
        )
        .await;
    assert_eq!(ins1.status, ResponseStatus::Success);

    let ins2 = server
        .execute_sql_as_user(
            &format!(
                "INSERT INTO {}.{} (id, content, priority) VALUES ('b1','hello B1',2)",
                namespace, table
            ),
            user2,
        )
        .await;
    assert_eq!(ins2.status, ResponseStatus::Success);

    // Flush synchronously so Parquet files exist for each user
    let flush_res = execute_flush_synchronously(&server, namespace, table)
        .await
        .expect("Flush failed");
    // Sanity check: at least some rows were flushed
    assert!(
        flush_res.rows_flushed > 0,
        "Expected at least 1 row flushed"
    );

    // Verify Parquet files exist for both users before DROP
    let files_user1 = check_user_parquet_files(&server, namespace, table, user1);
    let files_user2 = check_user_parquet_files(&server, namespace, table, user2);
    assert!(
        !files_user1.is_empty(),
        "Expected Parquet for {} before drop",
        user1
    );
    assert!(
        !files_user2.is_empty(),
        "Expected Parquet for {} before drop",
        user2
    );

    // Capture directories to verify deletion after DROP
    let dir_user1 = files_user1[0]
        .parent()
        .map(|p| p.to_path_buf())
        .expect("No parent dir for user1 file");
    let dir_user2 = files_user2[0]
        .parent()
        .map(|p| p.to_path_buf())
        .expect("No parent dir for user2 file");
    assert!(dir_user1.exists(), "User1 dir should exist pre-drop");
    assert!(dir_user2.exists(), "User2 dir should exist pre-drop");

    // DROP the table
    let drop_resp = server
        .execute_sql_as_user(&format!("DROP TABLE {}.{}", namespace, table), "system")
        .await;
    assert_eq!(
        drop_resp.status,
        ResponseStatus::Success,
        "DROP TABLE failed: {:?}",
        drop_resp.error
    );

    // Extract cleanup job ID from response and wait for it to complete
    let result_message = drop_resp
        .results.first()
        .and_then(|r| r.message.as_ref())
        .expect("DROP TABLE should return result message");

    if let Some(job_id) = extract_cleanup_job_id(result_message) {
        println!("Waiting for cleanup job {} to complete...", job_id);
        wait_for_cleanup_job_completion(&server, &job_id, Duration::from_secs(10))
            .await
            .expect("Cleanup job should complete successfully");
        println!("Cleanup job {} completed", job_id);
    } else {
        // Fallback: wait a bit for async cleanup if job ID not found
        println!("Could not extract cleanup job ID from: {}", result_message);
        sleep(Duration::from_millis(200)).await;
    }

    // Verify table metadata removed
    assert!(
        !server.table_exists(namespace, table).await,
        "Table metadata should be removed after drop"
    );

    // Verify per-user Parquet directories are removed (allow a brief delay after job completion)
    assert!(
        wait_for_path_absent(&dir_user1, Duration::from_secs(2)).await,
        "User1 Parquet dir still exists after drop: {}",
        dir_user1.display()
    );
    assert!(
        wait_for_path_absent(&dir_user2, Duration::from_secs(2)).await,
        "User2 Parquet dir still exists after drop: {}",
        dir_user2.display()
    );
}
