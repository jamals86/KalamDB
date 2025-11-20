//! Manual flush verification through the SQL surface.

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use kalamdb_api::models::ResponseStatus;

/// Issue `FLUSH TABLE`, wait for the job to complete, and ensure Parquet files appear.
#[actix_web::test]
async fn test_flush_table_command_creates_parquet() {
    let server = TestServer::new().await;
    let namespace = "manual_flush_sql";
    let table_name = "logs";
    let user_id = "operator";

    fixtures::create_namespace(&server, namespace).await;

    let create_sql = format!(
        "CREATE USER TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            message TEXT,
            created_at TIMESTAMP
        ) FLUSH ROWS 20",
        namespace, table_name
    );
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create table: {:?}",
        response
    );

    for i in 0..20 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (id, message, created_at) VALUES ({}, 'entry-{}', NOW())",
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
        flush_helpers::extract_job_id(job_id_message).expect("FLUSH TABLE should return job id");

    let job_result = flush_helpers::wait_for_flush_job_completion(
        &server,
        &job_id,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("flush job should complete");

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
        .expect("Parquet files should be readable");
}
