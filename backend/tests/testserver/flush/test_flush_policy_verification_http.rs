//! Flush policy + Parquet output verification over the real HTTP SQL API.
//!
//! Migrates coverage from:
//! - tests/integration/flush/test_manual_flush_verification.rs
//! - tests/integration/flush/test_automatic_flushing.rs
//! - tests/integration/flush/test_automatic_flushing_comprehensive.rs
//! - tests/integration/flush/test_flush_operations.rs

use super::test_support::flush::{
    count_parquet_files_for_table, flush_table_and_wait, wait_for_parquet_files_for_table,
    wait_for_parquet_files_for_user_table,
};
use super::test_support::http_server::HttpTestServer;
use super::test_support::jobs::{
    extract_cleanup_job_id, wait_for_job_completion, wait_for_path_absent,
};
use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use tokio::time::Duration;

async fn create_user(server: &HttpTestServer, username: &str) -> anyhow::Result<(String, String)> {
    let password = "UserPass123!";
    let resp = server
        .execute_sql(&format!(
            "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
            username, password
        ))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE USER failed: {:?}", resp.error);

    // Storage paths use internal `user_id` (not username), so fetch it for parquet assertions.
    let lookup = server
        .execute_sql(&format!(
            "SELECT user_id FROM system.users WHERE username = '{}' LIMIT 1",
            username
        ))
        .await?;
    anyhow::ensure!(
        lookup.status == ResponseStatus::Success,
        "system.users lookup failed: {:?}",
        lookup.error
    );

    let user_id = lookup
        .results
        .first()
        .map(|r| r.rows_as_maps())
        .unwrap_or_default()
        .first()
        .and_then(|r| r.get("user_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing user_id for {}", username))?
        .to_string();

    Ok((HttpTestServer::basic_auth_header(&UserName::new(username), password), user_id))
}

#[tokio::test]
#[ntest::timeout(180000)] // 3 minutes max for comprehensive flush policy test
async fn test_flush_policy_and_parquet_output_over_http() {
    (async {
    let server = super::test_support::http_server::get_global_server().await;
    let suffix = std::process::id();
    let ns = format!("flush_policy_{}", suffix);

    let resp = server
        .execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success);

    let user_a = format!("alice_{}", suffix);
    let user_b = format!("bob_{}", suffix);
    let (auth_a, user_a_id) = create_user(server, &user_a).await?;
    let (auth_b, user_b_id) = create_user(server, &user_b).await?;

            // -----------------------------------------------------------------
            // USER table: manual flush creates parquet + respects row threshold
            // -----------------------------------------------------------------
            {
                let table = "messages";
                let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, content TEXT) WITH (TYPE='USER', STORAGE_ID='local', FLUSH_POLICY='rows:25')",
                            ns, table
                        ),
                        &auth_a,
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE TABLE failed: {:?}", resp.error);

                for i in 0..25 {
                    let resp = server
                        .execute_sql_with_auth(
                            &format!("INSERT INTO {}.{} (id, content) VALUES ({}, 'msg-{}')", ns, table, i, i),
                            &auth_a,
                        )
                        .await?;
                    anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
                }

                flush_table_and_wait(server, &ns, table).await?;
                let _ = wait_for_parquet_files_for_user_table(server, &ns, table, &user_a_id, 1, Duration::from_secs(10)).await?;
            }

            // -----------------------------------------------------------------
            // USER table: multiple flush batches produce additional parquet
            // -----------------------------------------------------------------
            {
                let table = "events";
                let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, payload TEXT) WITH (TYPE='USER', STORAGE_ID='local', FLUSH_POLICY='rows:100,interval:30')",
                            ns, table
                        ),
                        &auth_a,
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success);

                let storage_root = server.storage_root();

                for batch in 0..3 {
                    for i in 0..10 {
                        let id = batch * 100 + i;
                        let resp = server
                            .execute_sql_with_auth(
                                &format!("INSERT INTO {}.{} (id, payload) VALUES ({}, 'p-{}')", ns, table, id, id),
                                &auth_a,
                            )
                            .await?;
                        anyhow::ensure!(resp.status == ResponseStatus::Success);
                    }

                    let before = count_parquet_files_for_table(&storage_root, &ns, table);
                    flush_table_and_wait(server, &ns, table).await?;
                    let _ = wait_for_parquet_files_for_table(
                        server,
                        &ns,
                        table,
                        before.saturating_add(1),
                        Duration::from_secs(10),
                    )
                    .await?;
                }
            }

            // -----------------------------------------------------------------
            // USER table: multi-user partitions produce per-user parquet output
            // -----------------------------------------------------------------
            {
                let table = "inbox";
                let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, body TEXT) WITH (TYPE='USER', STORAGE_ID='local', FLUSH_POLICY='rows:20')",
                            ns, table
                        ),
                        &auth_a,
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success);

                for i in 0..15 {
                    let resp = server
                        .execute_sql_with_auth(
                            &format!("INSERT INTO {}.{} (id, body) VALUES ({}, '{}-msg-{}')", ns, table, i, user_a, i),
                            &auth_a,
                        )
                        .await?;
                    anyhow::ensure!(resp.status == ResponseStatus::Success);
                }
                for i in 100..115 {
                    let resp = server
                        .execute_sql_with_auth(
                            &format!("INSERT INTO {}.{} (id, body) VALUES ({}, '{}-msg-{}')", ns, table, i, user_b, i),
                            &auth_b,
                        )
                        .await?;
                    anyhow::ensure!(resp.status == ResponseStatus::Success);
                }

                flush_table_and_wait(server, &ns, table).await?;

                let _ = wait_for_parquet_files_for_user_table(server, &ns, table, &user_a_id, 1, Duration::from_secs(10)).await?;
                let _ = wait_for_parquet_files_for_user_table(server, &ns, table, &user_b_id, 1, Duration::from_secs(10)).await?;
            }

            // -----------------------------------------------------------------
            // SHARED table: manual flush creates parquet
            // -----------------------------------------------------------------
            {
                let table = "audit_events";
                let resp = server
                    .execute_sql(&format!(
                        "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, entry TEXT) WITH (TYPE='SHARED', STORAGE_ID='local', FLUSH_POLICY='rows:5')",
                        ns, table
                    ))
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success);

                for i in 0..8 {
                    let resp = server
                        .execute_sql(&format!(
                            "INSERT INTO {}.{} (id, entry) VALUES ({}, 'entry-{}')",
                            ns, table, i, i
                        ))
                        .await?;
                    anyhow::ensure!(resp.status == ResponseStatus::Success);
                }

                flush_table_and_wait(server, &ns, table).await?;
                let _ = wait_for_parquet_files_for_table(server, &ns, table, 1, Duration::from_secs(10)).await?;
            }

            // -----------------------------------------------------------------
            // DROP TABLE: wait for cleanup job + parquet removal (smoke)
            // -----------------------------------------------------------------
            {
                let table = "drop_cleanup";
                let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, body TEXT) WITH (TYPE='USER', STORAGE_ID='local', FLUSH_POLICY='rows:2')",
                            ns, table
                        ),
                        &auth_a,
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success);

                for i in 0..2 {
                    let resp = server
                        .execute_sql_with_auth(
                            &format!("INSERT INTO {}.{} (id, body) VALUES ({}, 'x')", ns, table, i),
                            &auth_a,
                        )
                        .await?;
                    anyhow::ensure!(resp.status == ResponseStatus::Success);
                }

                flush_table_and_wait(server, &ns, table).await?;
                let parquet = wait_for_parquet_files_for_user_table(server, &ns, table, &user_a_id, 1, Duration::from_secs(10)).await?;
                let parquet_dir = parquet
                    .first()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .ok_or_else(|| anyhow::anyhow!("missing parquet parent dir"))?;

                let drop_resp = server
                    .execute_sql(&format!("DROP TABLE {}.{}", ns, table))
                    .await?;
                anyhow::ensure!(drop_resp.status == ResponseStatus::Success);

                let msg = drop_resp
                    .results
                    .first()
                    .and_then(|r| r.message.as_deref())
                    .unwrap_or("");

                if let Some(job_id) = extract_cleanup_job_id(msg) {
                    let _ = wait_for_job_completion(server, &job_id, Duration::from_secs(15)).await?;
                }

                // Allow async filestore cleanup to finish.
                anyhow::ensure!(
                    wait_for_path_absent(&parquet_dir, Duration::from_secs(5)).await,
                    "expected parquet dir removed after drop: {}",
                    parquet_dir.display()
                );
            }

    Ok(())
    })
        .await
        .expect("test_flush_policy_and_parquet_output_over_http");
}
