use anyhow::Result;
use kalam_link::models::ResponseStatus;
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration, Instant};

use super::http_server::HttpTestServer;

fn is_pending_job_status(status: &str) -> bool {
    matches!(status, "new" | "queued" | "running" | "retrying")
}

/// Wait until at least one matching flush job is completed and there are no pending flush jobs.
pub async fn wait_for_flush_jobs_settled(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);

    loop {
        let resp = server
            .execute_sql(
                "SELECT job_type, status, parameters FROM system.jobs WHERE job_type = 'flush'",
            )
            .await?;

        if resp.status != ResponseStatus::Success {
            if Instant::now() >= deadline {
                anyhow::bail!("Timed out waiting for system.jobs to be queryable");
            }
            sleep(Duration::from_millis(50)).await;
            continue;
        }

        let rows = resp.results.first().map(|r| r.rows_as_maps()).unwrap_or_default();

        let matching: Vec<_> = rows
            .iter()
            .filter(|r| {
                r.get("parameters")
                    .and_then(|v| v.as_str())
                    .map(|s| s.contains(ns) && s.contains(table))
                    .unwrap_or(false)
            })
            .collect();

        let has_completed = matching.iter().any(|r| {
            r.get("status")
                .and_then(|v| v.as_str())
                .map(|s| s == "completed")
                .unwrap_or(false)
        });

        let has_pending = matching.iter().any(|r| {
            r.get("status")
                .and_then(|v| v.as_str())
                .map(is_pending_job_status)
                .unwrap_or(false)
        });

        if has_completed && !has_pending {
            return Ok(());
        }

        if Instant::now() >= deadline {
            anyhow::bail!(
                "Timed out waiting for flush jobs to settle for {}.{} (matching={:?})",
                ns,
                table,
                matching
            );
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Execute `FLUSH TABLE` and wait until it settles. Treat idempotency conflicts as success.
pub async fn flush_table_and_wait(server: &HttpTestServer, ns: &str, table: &str) -> Result<()> {
    let sql = format!("FLUSH TABLE {}.{}", ns, table);
    let resp = server.execute_sql(&sql).await?;

    if resp.status == ResponseStatus::Success {
        return wait_for_flush_jobs_settled(server, ns, table).await;
    }

    let is_idempotent_conflict = resp
        .error
        .as_ref()
        .map(|e| e.message.contains("Idempotent conflict"))
        .unwrap_or(false);

    if is_idempotent_conflict {
        return wait_for_flush_jobs_settled(server, ns, table).await;
    }

    anyhow::bail!("FLUSH TABLE failed: {:?}", resp.error);
}

/// Recursively find parquet files under `root`.
pub fn find_parquet_files(root: &Path) -> Vec<PathBuf> {
    fn recurse(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                recurse(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    recurse(root, &mut out);
    out
}

/// Count parquet files matching a namespace/table substring filter.
pub fn count_parquet_files_for_table(storage_root: &Path, ns: &str, table: &str) -> usize {
    find_parquet_files(storage_root)
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            s.contains(ns) && s.contains(table)
        })
        .count()
}

/// Wait until at least `min_files` parquet files exist for a given table.
pub async fn wait_for_parquet_files_for_table(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
    min_files: usize,
    timeout: Duration,
) -> Result<Vec<PathBuf>> {
    let storage_root = server.storage_root();
    let deadline = Instant::now() + timeout;

    loop {
        let candidates = find_parquet_files(&storage_root);
        let matches: Vec<_> = candidates
            .into_iter()
            .filter(|p| {
                let s = p.to_string_lossy();
                s.contains(ns) && s.contains(table)
            })
            .collect();

        if matches.len() >= min_files {
            return Ok(matches);
        }

        if Instant::now() >= deadline {
            anyhow::bail!(
                "Timed out waiting for {} parquet files for {}.{} under {} (found={})",
                min_files,
                ns,
                table,
                storage_root.display(),
                matches.len()
            );
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Wait until at least `min_files` parquet files exist for a USER table for a specific user.
pub async fn wait_for_parquet_files_for_user_table(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
    user_id: &str,
    min_files: usize,
    timeout: Duration,
) -> Result<Vec<PathBuf>> {
    let storage_root = server.storage_root();
    let deadline = Instant::now() + timeout;

    loop {
        let candidates = find_parquet_files(&storage_root);
        let matches: Vec<_> = candidates
            .into_iter()
            .filter(|p| {
                let s = p.to_string_lossy();
                s.contains(ns) && s.contains(table) && s.contains(user_id)
            })
            .collect();

        if matches.len() >= min_files {
            return Ok(matches);
        }

        if Instant::now() >= deadline {
            anyhow::bail!(
                "Timed out waiting for {} parquet files for user {} in {}.{} under {} (found={})",
                min_files,
                user_id,
                ns,
                table,
                storage_root.display(),
                matches.len()
            );
        }

        sleep(Duration::from_millis(50)).await;
    }
}
