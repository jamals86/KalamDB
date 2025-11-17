//! Comprehensive manual verification for flush jobs.
//!
//! These tests exercise multi-user flushing and the SQL `FLUSH TABLE` command,
//! ensuring that Parquet files are produced and that job metadata is persisted.

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_commons::models::JobStatus;

/// Insert data for multiple users, flush, and ensure each user gets dedicated Parquet files.
#[tokio::test]
async fn test_manual_flush_multi_user_partitions() {
    let server = TestServer::start_test_server().await;
    let namespace = "flush_multi_user";
    let table_name = "inbox";
    let users = ["alice", "bob"];

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            message_id BIGINT,
            body TEXT
        ) FLUSH ROWS 20",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, "alice").await;
    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );

    for user in &users {
        for i in 0..15 {
            let insert_sql = format!(
                "INSERT INTO {}.{} (id, message_id, body) VALUES ({}, {}, '{}-msg-{}')",
                namespace, table_name, i, i, user, i
            );
            let response = server.execute_sql_as_user(&insert_sql, user).await;
            assert_eq!(
                response.status, "success",
                "Insert failed: {:?}",
                response.error
            );
        }
    }

    let flush_result = flush_helpers::execute_flush_synchronously(&server, namespace, table_name)
        .await
        .expect("manual flush should succeed");
    assert_eq!(flush_result.rows_flushed, users.len() * 15);
    assert_eq!(flush_result.parquet_files.len(), users.len());

    for user in &users {
        let parquet_files =
            flush_helpers::check_user_parquet_files(&server, namespace, table_name, user);
        flush_helpers::verify_parquet_files_exist(
            &parquet_files,
            1,
            &format!("multi-user flush ({})", user),
        )
        .expect("Parquet files should exist for each user");
    }
}

/// Use the SQL `FLUSH TABLE` command and verify job status plus Parquet output.
#[tokio::test]
async fn test_flush_table_sql_job_and_files() {
    let server = TestServer::start_test_server().await;
    let namespace = "flush_sql";
    let table_name = "audit_log";
    let user_id = "auditor";

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            entry TEXT
        ) FLUSH ROWS 50",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status, "success",
        "Failed to create table: {:?}",
        response.error
    );

    for i in 0..30 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, entry) VALUES ({}, 'entry-{}')",
            namespace, table_name, i, i
        );
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        assert_eq!(
            response.status, "success",
            "Insert failed: {:?}",
            response.error
        );
    }

    let flush_sql = format!("FLUSH TABLE {}.{}", namespace, table_name);
    let flush_response = server.execute_sql_as_user(&flush_sql, "system").await;
    assert_eq!(
        flush_response.status, "success",
        "FLUSH TABLE command failed: {:?}",
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
    assert!(
        job_result.to_lowercase().contains("flushed"),
        "job result should indicate flushed rows"
    );

    // Double-check job record status
    let job_query = format!("SELECT status FROM system.jobs WHERE job_id = '{}'", job_id);
    let job_response = server.execute_sql(&job_query).await;
    assert_eq!(
        job_response.status, "success",
        "Failed to query system.jobs"
    );
    if let Some(rows) = job_response.results.first().and_then(|r| r.rows.as_ref()) {
        let status = rows[0].get("status").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(status, JobStatus::Completed.as_str());
    }

    let parquet_files = flush_helpers::wait_for_parquet_files(
        &server,
        namespace,
        table_name,
        user_id,
        std::time::Duration::from_secs(5),
        1,
    )
    .await
    .expect("Parquet files should appear after FLUSH TABLE");
    flush_helpers::verify_parquet_files_exist(&parquet_files, 1, &job_result)
        .expect("Parquet files should exist after FLUSH TABLE");
}
