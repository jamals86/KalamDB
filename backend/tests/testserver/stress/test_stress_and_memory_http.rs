//! Stress / concurrency smoke tests over the real HTTP SQL API.
//!
//! These are intentionally short (seconds, not minutes) and run through the real
//! HTTP surface to cover business logic without flaking CI.

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use futures_util::future::try_join_all;
use kalamdb_api::models::ResponseStatus;
use test_support::http_server::{with_http_test_server_timeout, HttpTestServer};
use test_support::query_result_ext::QueryResultTestExt;
use tokio::time::Duration;

async fn create_user(server: &test_support::http_server::HttpTestServer, user: &str) -> anyhow::Result<String> {
    let password = "UserPass123!";
    let resp = server
        .execute_sql(&format!(
            "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
            user, password
        ))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE USER failed: {:?}", resp.error);
    Ok(HttpTestServer::basic_auth_header(user, password))
}

async fn count_rows(
    server: &test_support::http_server::HttpTestServer,
    auth: &str,
    ns: &str,
    table: &str,
) -> anyhow::Result<i64> {
    let resp = server
        .execute_sql_with_auth(
            &format!("SELECT COUNT(*) AS cnt FROM {}.{}", ns, table),
            auth,
        )
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "COUNT failed: {:?}", resp.error);

    let row = resp
        .results
        .first()
        .and_then(|r| r.row_as_map(0))
        .ok_or_else(|| anyhow::anyhow!("Missing COUNT row"))?;

    row.get("cnt")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|u| u as i64))
                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
        .ok_or_else(|| anyhow::anyhow!("COUNT value not an integer: {:?}", row.get("cnt")))
}

#[tokio::test]
async fn test_stress_smoke_over_http() {
    with_http_test_server_timeout(Duration::from_secs(45), |server| {
        Box::pin(async move {
            let suffix = std::process::id();
            let ns = format!("stress_{}", suffix);

            let resp = server
                .execute_sql(&format!("CREATE NAMESPACE {}", ns))
                .await?;
            anyhow::ensure!(
                resp.status == ResponseStatus::Success,
                "CREATE NAMESPACE failed: {:?}",
                resp.error
            );

            let user = format!("stress_user_{}", suffix);
            let auth = create_user(server, &user).await?;

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "CREATE TABLE {}.stress_data (id INT PRIMARY KEY, value TEXT) WITH (TYPE='USER', STORAGE_ID='local')",
                        ns
                    ),
                    &auth,
                )
                .await?;
            anyhow::ensure!(
                resp.status == ResponseStatus::Success,
                "CREATE TABLE failed: {:?}",
                resp.error
            );

            // Concurrent writers (small, deterministic).
            let writer_futures = (0..5).map(|writer| {
                let ns = ns.clone();
                let auth = auth.clone();
                async move {
                    for j in 0..10 {
                        let id = writer * 100 + j;
                        let sql = format!(
                            "INSERT INTO {}.stress_data (id, value) VALUES ({}, 'w{}-{}')",
                            ns, id, writer, j
                        );
                        let resp = server.execute_sql_with_auth(&sql, &auth).await?;
                        anyhow::ensure!(
                            resp.status == ResponseStatus::Success,
                            "insert failed: {:?}",
                            resp.error
                        );
                    }
                    anyhow::Ok(())
                }
            });
            try_join_all(writer_futures).await?;

            let cnt = count_rows(server, &auth, &ns, "stress_data").await?;
            anyhow::ensure!(cnt == 50, "expected 50 rows, got {}", cnt);

            // Basic cleanup: DROP TABLE should succeed.
            // Note: DROP TABLE currently requires admin privileges.
            let resp = server.execute_sql(&format!("DROP TABLE {}.stress_data", ns)).await?;
            anyhow::ensure!(
                resp.status == ResponseStatus::Success,
                "DROP TABLE failed: {:?}",
                resp.error
            );

            Ok(())
        })
    })
    .await
    .expect("with_http_test_server_timeout");
}
