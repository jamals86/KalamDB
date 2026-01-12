//! System tables smoke coverage over the real HTTP SQL API.

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use test_support::flush::flush_table_and_wait;
use test_support::http_server::{with_http_test_server_timeout, HttpTestServer};
use tokio::time::Duration;

async fn create_user(server: &HttpTestServer, username: &str) -> anyhow::Result<String> {
    let password = "UserPass123!";
    let resp = server
        .execute_sql(&format!(
            "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
            username, password
        ))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE USER failed: {:?}", resp.error);
    Ok(HttpTestServer::basic_auth_header(&UserName::new(username), password))
}

#[tokio::test]
async fn test_system_tables_queryable_over_http() {
    with_http_test_server_timeout(Duration::from_secs(45), |server| {
        Box::pin(async move {
            let suffix = std::process::id();
            let ns = format!("sys_{}", suffix);
            let table_user = "ut";
            let table_shared = "st";

            let resp = server
                .execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);

            let user = format!("sys_user_{}", suffix);
            let auth = create_user(server, &user).await?;

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "CREATE TABLE {}.{} (id INT PRIMARY KEY, v TEXT) WITH (TYPE='USER', STORAGE_ID='local', FLUSH_POLICY='rows:5')",
                        ns, table_user
                    ),
                    &auth,
                )
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);

            let resp = server
                .execute_sql(&format!(
                    "CREATE TABLE {}.{} (id INT PRIMARY KEY, v TEXT) WITH (TYPE='SHARED', STORAGE_ID='local', FLUSH_POLICY='rows:5')",
                    ns, table_shared
                ))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);

            let resp = server
                .execute_sql_with_auth(&format!("INSERT INTO {}.{} (id, v) VALUES (1, 'x')", ns, table_user), &auth)
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);

            let resp = server
                .execute_sql(&format!("INSERT INTO {}.{} (id, v) VALUES (1, 'y')", ns, table_shared))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);

            flush_table_and_wait(server, &ns, table_user).await?;

            // system.namespaces
            let resp = server
                .execute_sql(&format!("SELECT namespace_id FROM system.namespaces WHERE namespace_id = '{}'", ns))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
            anyhow::ensure!(!resp.results[0].rows_as_maps().is_empty());

            // system.tables
            let resp = server
                .execute_sql(&format!(
                    "SELECT table_name, table_type FROM system.tables WHERE namespace_id = '{}' ORDER BY table_name",
                    ns
                ))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
            let rows = resp.results[0].rows_as_maps();
            anyhow::ensure!(rows.len() >= 2);

            // system.users
            let resp = server
                .execute_sql(&format!("SELECT username, role FROM system.users WHERE username = '{}'", user))
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
            anyhow::ensure!(!resp.results[0].rows_as_maps().is_empty());

            // system.storages
            let resp = server.execute_sql("SELECT storage_id, storage_type FROM system.storages").await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
            let rows = resp.results[0].rows_as_maps();
            anyhow::ensure!(rows.iter().any(|r| r.get("storage_id").and_then(|v| v.as_str()) == Some("local")));

            // system.jobs (flush should have created at least one record)
            let resp = server
                .execute_sql("SELECT job_type, status FROM system.jobs WHERE job_type = 'flush'")
                .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
            anyhow::ensure!(!resp.results[0].rows_as_maps().is_empty());

            Ok(())
        })
    })
    .await
    .expect("with_http_test_server_timeout");
}
