//! Parameterized DML tests over the real HTTP SQL API.
//!
//! Validates:
//! - Parameter binding for INSERT/UPDATE/DELETE ($1, $2, ...)
//! - Parameter validation (max 50 params, 512KB each)
//! - Params not allowed with multi-statement batches

use super::test_support::http_server::HttpTestServer;
use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use serde_json::json;
use tokio::time::{sleep, Duration, Instant};

async fn create_user(server: &HttpTestServer, username: &str) -> anyhow::Result<String> {
    let password = "UserPass123!";
    let resp = server
        .execute_sql(&format!("CREATE USER '{}' WITH PASSWORD '{}' ROLE 'dba'", username, password))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE USER failed: {:?}", resp.error);
    Ok(HttpTestServer::basic_auth_header(&UserName::new(username), password))
}

async fn count_rows(
    server: &HttpTestServer,
    auth: &str,
    ns: &str,
    table: &str,
) -> anyhow::Result<i64> {
    let resp = server
        .execute_sql_with_auth(&format!("SELECT COUNT(*) AS cnt FROM {}.{}", ns, table), auth)
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
#[ntest::timeout(90000)] // 90 seconds - parameterized DML test
async fn test_parameterized_dml_over_http() {
    (async {
    let server = super::test_support::http_server::get_global_server().await;
    let suffix = std::process::id();
    let ns = format!("params_{}", suffix);
    let table = "items";

    let resp = server
        .execute_sql(&format!("CREATE NAMESPACE {}", ns))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE NAMESPACE failed");

    let auth = create_user(server, &format!("user_params_{}", suffix)).await?;

    let resp = server
        .execute_sql(&format!(
            "CREATE TABLE {}.{} (id BIGINT PRIMARY KEY, name TEXT, age INT) WITH (TYPE='SHARED', STORAGE_ID='local')",
            ns, table
        ))
        .await?;
    anyhow::ensure!(resp.status == ResponseStatus::Success, "CREATE TABLE failed: {:?}", resp.error);

    // Near-production servers may accept DDL before it is immediately queryable
    // (e.g. metadata propagation/registration). Wait briefly for visibility.
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let probe = server
                .execute_sql(&format!(
                    "SELECT COUNT(*) AS cnt FROM system.tables WHERE namespace_id = '{}' AND table_name = '{}'",
                    ns, table
                ))
                .await;

            if let Ok(resp) = &probe {
                if resp.status == ResponseStatus::Success {
                    let cnt = resp
                        .results
                        .first()
                        .and_then(|r| r.row_as_map(0))
                        .and_then(|m| m.get("cnt").cloned())
                        .and_then(|v| {
                            v.as_i64()
                                .or_else(|| v.as_u64().map(|u| u as i64))
                                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
                        })
                        .unwrap_or(0);
                    if cnt >= 1 {
                        break;
                    }
                }
            }

            if Instant::now() >= deadline {
                let listing = server
                    .execute_sql(&format!(
                        "SELECT namespace_id, table_name FROM system.tables WHERE table_name = '{}'",
                        table
                    ))
                    .await
                    .ok()
                    .and_then(|r| r.results.first().map(|qr| qr.rows_as_maps()))
                    .unwrap_or_default();

                anyhow::bail!(
                    "Table {}.{} not visible in system.tables after CREATE TABLE (last_probe={:?}, listing={:?})",
                    ns,
                    table,
                    probe
                    ,
                    listing
                );
            }

        sleep(Duration::from_millis(50)).await;
        }
    }

    // INSERT with parameters
    {
        let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!(
                            "INSERT INTO {}.{} (id, name, age) VALUES ($1, $2, $3)",
                            ns, table
                        ),
                        &auth,
                        vec![json!(1), json!("Alice"), json!(30)],
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success, "INSERT params failed: {:?}", resp.error);
                let cnt = count_rows(server, &auth, &ns, table).await?;
                anyhow::ensure!(cnt == 1, "expected 1 row after insert, got {}", cnt);
            }

            // UPDATE with parameters
            {
                let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!(
                            "UPDATE {}.{} SET name = $1, age = $2 WHERE id = $3",
                            ns, table
                        ),
                        &auth,
                        vec![json!("Alice Updated"), json!(31), json!(1)],
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success, "UPDATE params failed: {:?}", resp.error);

                let resp = server
                    .execute_sql_with_auth(
                        &format!("SELECT name, age FROM {}.{} WHERE id = 1", ns, table),
                        &auth,
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success, "SELECT failed: {:?}", resp.error);
                let row = resp
                    .results
                    .first()
                    .and_then(|r| r.row_as_map(0))
                    .ok_or_else(|| anyhow::anyhow!("Missing row"))?;
                anyhow::ensure!(row.get("name").and_then(|v| v.as_str()) == Some("Alice Updated"));
                anyhow::ensure!(row.get("age").and_then(|v| v.as_i64()) == Some(31));
            }

            // DELETE with parameters
            {
                let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!("DELETE FROM {}.{} WHERE id = $1", ns, table),
                        &auth,
                        vec![json!(1)],
                    )
                    .await?;
                anyhow::ensure!(resp.status == ResponseStatus::Success, "DELETE params failed: {:?}", resp.error);
                let cnt = count_rows(server, &auth, &ns, table).await?;
                anyhow::ensure!(cnt == 0, "expected 0 rows after delete, got {}", cnt);
            }

            // Parameter count validation (max 50)
            {
                let mut params = Vec::new();
                for i in 1..=51 {
                    params.push(json!(i));
                }

                let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!("INSERT INTO {}.{} (id, name, age) VALUES ($1, 'x', 0)", ns, table),
                        &auth,
                        params,
                    )
                    .await;
                
                // Should fail with parameter count error
                match resp {
                    Err(e) => {
                        let err_msg = e.to_string();
                        // Error might be wrapped, just ensure it failed
                        assert!(!err_msg.is_empty(), "Should have error message");
                    }
                    Ok(resp) if resp.status == ResponseStatus::Error => {
                        let msg = resp
                            .error
                            .as_ref()
                            .map(|e| e.message.as_str())
                            .unwrap_or("");
                        anyhow::ensure!(
                            msg.to_lowercase().contains("parameter") && msg.to_lowercase().contains("limit"),
                            "unexpected error message: {}",
                            msg
                        );
                    }
                    Ok(_) => anyhow::bail!("expected params count error"),
                }
            }

            // Parameter size validation (512KB)
            {
                let large_string = "a".repeat(600_000);
                let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!("INSERT INTO {}.{} (id, name, age) VALUES ($1, $2, 0)", ns, table),
                        &auth,
                        vec![json!(2), json!(large_string)],
                    )
                    .await;
                
                // Should fail with parameter size error
                match resp {
                    Err(e) => {
                        let err_msg = e.to_string();
                        // Error might be wrapped, just ensure it failed
                        assert!(!err_msg.is_empty(), "Should have error message");
                    }
                    Ok(resp) if resp.status == ResponseStatus::Error => {
                        let msg = resp
                            .error
                            .as_ref()
                            .map(|e| e.message.as_str())
                            .unwrap_or("");
                        anyhow::ensure!(
                            msg.to_lowercase().contains("size") || msg.to_lowercase().contains("512"),
                            "unexpected error message: {}",
                            msg
                        );
                    }
                    Ok(_) => anyhow::bail!("expected params size error"),
                }
            }

            // Multi-statement batches with params should be rejected
            {
                let resp = server
                    .execute_sql_with_auth_and_params(
                        &format!("INSERT INTO {}.{} (id, name, age) VALUES ($1, 'x', 0); SELECT 1", ns, table),
                        &auth,
                        vec![json!(123)],
                    )
                    .await;
                
                // Should fail - multi-statement batches with parameters are not allowed
                match resp {
                    Err(_e) => {
                        // Expected - operation should be rejected
                    }
                    Ok(resp) if resp.status == ResponseStatus::Error => {
                        // Also acceptable if server returns error response
                    }
                    Ok(_) => anyhow::bail!("expected params-with-batch error"),
        }
    }

    Ok(())
    })
        .await
        .expect("test_parameterized_dml_over_http");
}
