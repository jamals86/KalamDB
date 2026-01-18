//! Quickstart end-to-end smoke over the real HTTP SQL API.

use super::test_support::http_server::HttpTestServer;
use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;

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
#[ntest::timeout(60000)] // 60 seconds - comprehensive quickstart test
async fn test_quickstart_workflow_over_http() -> anyhow::Result<()> {
    let server = super::test_support::http_server::get_global_server().await;
    let suffix = std::process::id();
    let ns = format!("qs_{}", suffix);
    let user = format!("user_qs_{}", suffix);
    let auth = create_user(server, &user).await?;

    let resp = server.execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns)).await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success);

    // USER table: messages
    {
        let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.messages (id BIGINT PRIMARY KEY, content TEXT, created_at BIGINT) WITH (TYPE='USER', STORAGE_ID='local')",
                            ns
                        ),
                        &auth,
                    )
                    .await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "CREATE USER table failed: {:?}",
            resp.error
        );

        for i in 0..5 {
            let resp = server
                        .execute_sql_with_auth(
                            &format!(
                                "INSERT INTO {}.messages (id, content, created_at) VALUES ({}, 'msg-{}', {})",
                                ns,
                                i,
                                i,
                                1000 + i
                            ),
                            &auth,
                        )
                        .await?;
            anyhow::ensure!(resp.status == ResponseStatus::Success);
        }

        let resp = server
            .execute_sql_with_auth(&format!("SELECT COUNT(*) AS cnt FROM {}.messages", ns), &auth)
            .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);
        let cnt = resp
            .results
            .first()
            .and_then(|r| r.row_as_map(0))
            .and_then(|row| row.get("cnt").cloned())
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
            .unwrap_or(0);
        anyhow::ensure!(cnt == 5, "expected 5 rows, got {}", cnt);

        let resp = server
            .execute_sql_with_auth(
                &format!("UPDATE {}.messages SET content = 'updated' WHERE id = 0", ns),
                &auth,
            )
            .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);

        let resp = server
            .execute_sql_with_auth(&format!("DELETE FROM {}.messages WHERE id = 1", ns), &auth)
            .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);
    }

    // SHARED table: config
    {
        let resp = server
                    .execute_sql(&format!(
                        "CREATE TABLE {}.config (name TEXT PRIMARY KEY, value TEXT) WITH (TYPE='SHARED', STORAGE_ID='local')",
                        ns
                    ))
                    .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);

        let resp = server
            .execute_sql(&format!(
                "INSERT INTO {}.config (name, value) VALUES ('max_connections', '100')",
                ns
            ))
            .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);

        let resp = server
            .execute_sql(&format!("SELECT value FROM {}.config WHERE name='max_connections'", ns))
            .await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success);
        let row = resp
            .results
            .first()
            .and_then(|r| r.row_as_map(0))
            .ok_or_else(|| anyhow::anyhow!("Missing config row"))?;
        anyhow::ensure!(row.get("value").and_then(|v| v.as_str()) == Some("100"));
    }

    // Additional USER table: events
    // Note: STREAM table type is not yet implemented.
    {
        let resp = server
                    .execute_sql_with_auth(
                        &format!(
                            "CREATE TABLE {}.events (id BIGINT PRIMARY KEY, kind TEXT, created_at BIGINT) WITH (TYPE='USER', STORAGE_ID='local')",
                            ns
                        ),
                        &auth,
                    )
                    .await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "CREATE events table failed: {:?}",
            resp.error
        );

        let resp = server
            .execute_sql_with_auth(
                &format!(
                    "INSERT INTO {}.events (id, kind, created_at) VALUES (1, 'start', 1000)",
                    ns
                ),
                &auth,
            )
            .await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "INSERT events row failed: {:?}",
            resp.error
        );

        let resp = server
            .execute_sql_with_auth(&format!("SELECT COUNT(*) AS cnt FROM {}.events", ns), &auth)
            .await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "SELECT events count failed: {:?}",
            resp.error
        );
    }

    // system tables basic query
    {
        let resp = server
            .execute_sql(&format!(
                "SELECT table_name FROM system.tables WHERE namespace_id='{}'",
                ns
            ))
            .await?;
        eprintln!("system.tables response: {:?}", resp);
        if resp.status != ResponseStatus::Success {
            eprintln!("system.tables query failed: {:?}", resp.error);
        }
        anyhow::ensure!(resp.status == ResponseStatus::Success);
        anyhow::ensure!(!resp.rows_as_maps().is_empty());
    }
    Ok(())
}
