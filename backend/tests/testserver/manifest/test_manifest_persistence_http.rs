//! Manifest persistence behavior over the real HTTP SQL API.

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use test_support::http_server::HttpTestServer;
use tokio::time::{sleep, Duration, Instant};

fn find_manifest_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn recurse(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                recurse(&path, out);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("manifest.json") {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    recurse(root, &mut out);
    out
}

async fn wait_for_flush_job_completed(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let resp = server
            .execute_sql("SELECT job_type, status, parameters FROM system.jobs WHERE job_type = 'flush'")
            .await?;

        if resp.status == ResponseStatus::Success {
            let rows = resp.results[0].rows_as_maps();
            let maybe_job = rows.iter().find(|r| {
                r.get("parameters")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains(ns) && s.contains(table))
                    .unwrap_or(false)
            });
            if let Some(job) = maybe_job {
                let status = job.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status == "completed" {
                    return Ok(());
                }
            }
        }

        if Instant::now() >= deadline {
            anyhow::bail!("Timed out waiting for flush job to complete for {}.{}", ns, table);
        }
        sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn test_user_table_manifest_persistence_over_http() {
    let server = test_support::http_server::get_global_server().await;
    // Case 1: Manifest written only after flush
    {
            let ns = format!("test_manifest_persist_{}", std::process::id());
            let table = "events";
            let user = "user1";
            let password = "UserPass123!";

            let resp = server
                .execute_sql(&format!(
                    "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
                    user, password
                ))
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success, "resp.error={:?}", resp.error);

            let user_auth = HttpTestServer::basic_auth_header(&UserName::new(user), password);

            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            assert_eq!(resp.status, ResponseStatus::Success, "resp.error={:?}", resp.error);

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "CREATE TABLE {}.{} (id TEXT PRIMARY KEY, event_type TEXT, ts INT) WITH (TYPE = 'USER', STORAGE_ID = 'local')",
                        ns, table
                    ),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "INSERT INTO {}.{} (id, event_type, ts) VALUES ('evt1', 'login', 1)",
                        ns, table
                    ),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let storage_root = server.storage_root();
            let manifests_before = find_manifest_files(&storage_root)
                .into_iter()
                .filter(|p| {
                    let s = p.to_string_lossy();
                    // Check for namespace, table, and user directory (u_<hash>)
                    s.contains(&ns) && s.contains(table) && p.components().any(|c| {
                        c.as_os_str().to_string_lossy().starts_with("u_")
                    })
                })
                .collect::<Vec<_>>();
            assert!(
                manifests_before.is_empty(),
                "Manifest should NOT exist on disk before flush for {}.{} (found: {:?})",
                ns,
                table,
                manifests_before
            );

            let resp = server
                .execute_sql(&format!("FLUSH TABLE {}.{}", ns, table))
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            wait_for_flush_job_completed(server, &ns, table).await?;

            let deadline = Instant::now() + Duration::from_secs(5);
            let _manifest_path = loop {
                let candidates = find_manifest_files(&storage_root);
                if let Some(path) = candidates.iter().find(|p| {
                    let s = p.to_string_lossy();
                    // Check for namespace, table, and user directory (u_<hash>)
                    s.contains(&ns) && s.contains(table) && p.components().any(|c| {
                        c.as_os_str().to_string_lossy().starts_with("u_")
                    })
                }) {
                    break path.to_path_buf();
                }
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "Expected manifest.json for {}.{} under {} (found: {:?})",
                        ns,
                        table,
                        storage_root.display(),
                        candidates
                    );
                }
                sleep(Duration::from_millis(50)).await;
            };

            let resp = server
                .execute_sql_with_auth(
                    &format!("SELECT id, event_type FROM {}.{} ORDER BY id", ns, table),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let rows = resp.results[0].rows_as_maps();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get("id").unwrap().as_str().unwrap(), "evt1");
        }

        // Case 2: Manifest exists after flush and query can read data
        {
            let ns = format!("test_manifest_reload_{}", std::process::id());
            let table = "metrics";
            let user = "user2";
            let password = "UserPass123!";

            let resp = server
                .execute_sql(&format!(
                    "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
                    user, password
                ))
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success, "resp.error={:?}", resp.error);

            let user_auth = HttpTestServer::basic_auth_header(&UserName::new(user), password);

            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            assert_eq!(resp.status, ResponseStatus::Success, "resp.error={:?}", resp.error);

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "CREATE TABLE {}.{} (id TEXT PRIMARY KEY, metric_name TEXT, value DOUBLE) WITH (TYPE = 'USER', STORAGE_ID = 'local')",
                        ns, table
                    ),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success, "resp.error={:?}", resp.error);

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "INSERT INTO {}.{} (id, metric_name, value) VALUES ('m1', 'cpu_usage', 45.5)",
                        ns, table
                    ),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let resp = server
                .execute_sql(&format!("FLUSH TABLE {}.{}", ns, table))
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            wait_for_flush_job_completed(server, &ns, table).await?;

            let storage_root = server.storage_root();
            let deadline = Instant::now() + Duration::from_secs(5);
            let _manifest_path = loop {
                let candidates = find_manifest_files(&storage_root);
                if let Some(path) = candidates.iter().find(|p| {
                    let s = p.to_string_lossy();
                    // Check for namespace, table, and user directory (u_<hash>)
                    s.contains(&ns) && s.contains(table) && p.components().any(|c| {
                        c.as_os_str().to_string_lossy().starts_with("u_")
                    })
                }) {
                    break path.to_path_buf();
                }
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "Expected manifest.json for {}.{} under {} (found: {:?})",
                        ns,
                        table,
                        storage_root.display(),
                        candidates
                    );
                }
                sleep(Duration::from_millis(50)).await;
            };

            let resp = server
                .execute_sql_with_auth(
                    &format!(
                        "SELECT id, metric_name, value FROM {}.{} WHERE id = 'm1'",
                        ns, table
                    ),
                    &user_auth,
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let rows = resp.results[0].rows_as_maps();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].get("metric_name").unwrap().as_str().unwrap(),
                "cpu_usage"
            );
        }

        Ok(())
        })
    }
}
