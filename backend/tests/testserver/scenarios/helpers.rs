//! Common test helpers and assertion utilities for scenario tests.
//!
//! This module provides reusable helpers for:
//! - User isolation assertions
//! - Query result assertions
//! - Subscription lifecycle assertions
//! - Flush/storage artifact validation
//! - Parallel test utilities

use anyhow::Result;
use futures_util::StreamExt;
use kalam_link::models::{ChangeEvent, QueryResponse, ResponseStatus};
use kalam_link::SubscriptionManager;
use kalamdb_commons::Role;
use kalamdb_commons::models::UserRowId;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::{sleep, timeout, Instant};

/// Test harness path for HttpTestServer
#[path = "../../common/testserver/mod.rs"]
pub mod test_support;

pub use test_support::http_server::{
    with_http_test_server, with_http_test_server_timeout, HttpTestServer,
};

// =============================================================================
// QueryResponse Helpers (for link client)
// =============================================================================

/// Get count from a SELECT COUNT(*) AS cnt query response
pub fn get_count(resp: &QueryResponse) -> i64 {
    resp.get_i64("cnt").unwrap_or(0)
}

/// Get all rows as HashMaps from a QueryResponse
pub fn get_response_rows(resp: &QueryResponse) -> Vec<HashMap<String, JsonValue>> {
    resp.rows_as_maps()
}

/// Get string value from first row
pub fn get_first_string(resp: &QueryResponse, column: &str) -> Option<String> {
    resp.get_string(column)
}

/// Get i64 value from first row
pub fn get_first_i64(resp: &QueryResponse, column: &str) -> Option<i64> {
    resp.get_i64(column)
}

/// Create a user if they don't exist and return their user_id
/// Returns Ok(user_id) whether user was created or already existed
pub async fn ensure_user_exists(server: &HttpTestServer, username: &str, password: &str, role: &Role) -> Result<String> {
    // Try to create user using proper CREATE USER syntax
    let sql = format!(
        "CREATE USER '{}' WITH PASSWORD '{}' ROLE '{}'",
        username, password, role
    );
    let _resp = server.execute_sql(&sql).await;
    // Ignore errors - user might already exist or command might not be supported
    
    // Look up the user_id - use rows_as_maps for easier extraction
    let lookup_sql = format!("SELECT user_id FROM system.users WHERE username = '{}'", username);
    let resp = server.execute_sql(&lookup_sql).await?;
    
    // Use rows_as_maps which handles Arrow type unwrapping
    let rows = resp.rows_as_maps();
    
    if let Some(row) = rows.first() {
        if let Some(user_id_val) = row.get("user_id") {
            // Handle both direct strings and Arrow-wrapped strings
            let user_id_str = match user_id_val {
                JsonValue::String(s) => s.clone(),
                JsonValue::Object(map) if map.contains_key("Utf8") => {
                    map.get("Utf8").and_then(|v| v.as_str()).unwrap_or("").to_string()
                }
                _ => user_id_val.as_str().unwrap_or("").to_string(),
            };
            
            if !user_id_str.is_empty() {
                // Cache the user_id in the server for link_client to use
                server.cache_user_id(username, &user_id_str);
                return Ok(user_id_str);
            }
        }
    }
    
    // If we can't find the user_id, fail loudly instead of using a fallback
    anyhow::bail!(
        "Failed to get user_id for user '{}'. Query returned: {:?}",
        username,
        resp.rows_as_maps()
    )
}

/// Create multiple test users at once
pub async fn create_test_users(server: &HttpTestServer, users: &[(&str, &Role)]) -> Result<()> {
    for (username, role) in users {
        ensure_user_exists(server, username, "test123", role).await?;
    }
    Ok(())
}

/// Create a user and return a link client configured for that user
pub async fn create_user_and_client(server: &HttpTestServer, username: &str, role: &Role) -> Result<kalam_link::KalamLinkClient> {
    let user_id = ensure_user_exists(server, username, "test123", role).await?;
    Ok(server.link_client_with_id(&user_id, username, role))
}

/// Assert QueryResponse was successful
pub fn assert_query_success(resp: &QueryResponse, context: &str) {
    assert!(
        resp.success(),
        "{}: expected success, got error: {:?}",
        context,
        resp.error
    );
}

/// Assert QueryResponse has expected row count
pub fn assert_query_row_count(resp: &QueryResponse, expected: usize, context: &str) {
    assert_query_success(resp, context);
    let actual = resp.row_count();
    assert_eq!(
        actual, expected,
        "{}: expected {} rows, got {}",
        context, expected, actual
    );
}

// =============================================================================
// Query Result Assertions (QueryResponse)
// =============================================================================

/// Assert that a SQL response was successful
pub fn assert_success(resp: &QueryResponse, context: &str) {
    assert_eq!(
        resp.status,
        ResponseStatus::Success,
        "{}: expected success, got {:?}",
        context,
        resp.error
    );
}

/// Assert that a SQL response failed with an expected error message substring
pub fn assert_error_contains(resp: &QueryResponse, expected: &str, context: &str) {
    assert_eq!(
        resp.status,
        ResponseStatus::Error,
        "{}: expected error, got success",
        context
    );
    let msg = resp
        .error
        .as_ref()
        .map(|e| e.message.as_str())
        .unwrap_or("");
    assert!(
        msg.to_lowercase().contains(&expected.to_lowercase()),
        "{}: expected error containing '{}', got '{}'",
        context,
        expected,
        msg
    );
}

/// Assert exact row count in a SQL response
pub fn assert_row_count(resp: &QueryResponse, expected: usize, context: &str) {
    assert_success(resp, context);
    let actual = resp.row_count();
    assert_eq!(
        actual, expected,
        "{}: expected {} rows, got {}",
        context, expected, actual
    );
}

/// Assert minimum row count in a SQL response
pub fn assert_min_row_count(resp: &QueryResponse, min: usize, context: &str) {
    assert_success(resp, context);
    let actual = resp.row_count();
    assert!(
        actual >= min,
        "{}: expected at least {} rows, got {}",
        context,
        min,
        actual
    );
}

/// Get all rows as vectors of JSON values from a QueryResponse
pub fn get_rows(resp: &QueryResponse) -> Vec<Vec<serde_json::Value>> {
    resp.rows()
}

/// Get column index by name from schema
pub fn get_column_index(resp: &QueryResponse, column_name: &str) -> Option<usize> {
    resp.column_index(column_name)
}

/// Extract a string column value from a row
pub fn get_string_value(row: &[serde_json::Value], idx: usize) -> Option<String> {
    row.get(idx).and_then(|v| {
        v.as_str()
            .map(|s| s.to_string())
            .or_else(|| v.get("Utf8").and_then(|v2| v2.as_str().map(|s| s.to_string())))
    })
}

/// Extract an i64 column value from a row
pub fn get_i64_value(row: &[serde_json::Value], idx: usize) -> Option<i64> {
    row.get(idx).and_then(|v| v.as_i64())
}

/// Extract an i64 value from a JSON value, handling both Number and String types
pub fn json_to_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

// =============================================================================
// User Isolation Assertions
// =============================================================================

/// Assert that user isolation is maintained: user_a cannot see user_b's data
pub async fn assert_user_isolation(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
    user_a: &str,
    user_b: &str,
) -> Result<()> {
    let client_a = server.link_client(user_a);
    let client_b = server.link_client(user_b);

    // Query as user A
    let sql = format!("SELECT * FROM {}.{}", ns, table);
    let resp_a = client_a.execute_query(&sql, None, None).await?;

    // Query as user B
    let resp_b = client_b.execute_query(&sql, None, None).await?;

    // Extract user_id values from each response
    let get_user_ids = |resp: &kalam_link::models::QueryResponse| -> HashSet<String> {
        resp.rows_as_maps()
            .iter()
            .filter_map(|row: &HashMap<String, JsonValue>| {
                row.get("user_id")
                    .or_else(|| row.get("_user_id"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
            .collect()
    };

    let user_ids_a = get_user_ids(&resp_a);
    let user_ids_b = get_user_ids(&resp_b);

    // User A should only see their own data
    for uid in &user_ids_a {
        assert!(
            uid == user_a || uid.is_empty(),
            "User {} saw data from user {}",
            user_a,
            uid
        );
    }

    // User B should only see their own data
    for uid in &user_ids_b {
        assert!(
            uid == user_b || uid.is_empty(),
            "User {} saw data from user {}",
            user_b,
            uid
        );
    }

    Ok(())
}

// =============================================================================
// Subscription Assertions
// =============================================================================

/// Wait for subscription ACK event
pub async fn wait_for_ack(
    subscription: &mut SubscriptionManager,
    timeout_duration: Duration,
) -> Result<(String, usize)> {
    let deadline = Instant::now() + timeout_duration;

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), subscription.next()).await {
            Ok(Some(Ok(ChangeEvent::Ack {
                subscription_id,
                total_rows,
                ..
            }))) => {
                return Ok((subscription_id, total_rows as usize));
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscription error: {:?}", e)),
            Ok(None) => return Err(anyhow::anyhow!("Subscription stream ended")),
            Err(_) => continue, // Timeout on this iteration, try again
        }
    }

    Err(anyhow::anyhow!("Timed out waiting for ACK"))
}

/// Drain initial data batches from subscription and return total row count
pub async fn drain_initial_data(
    subscription: &mut SubscriptionManager,
    timeout_duration: Duration,
) -> Result<usize> {
    let deadline = Instant::now() + timeout_duration;
    let mut total_rows = 0;

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), subscription.next()).await {
            Ok(Some(Ok(ChangeEvent::Ack { .. }))) => continue,
            Ok(Some(Ok(ChangeEvent::InitialDataBatch {
                rows,
                batch_control,
                ..
            }))) => {
                total_rows += rows.len();
                // Check if this is the last batch
                if !batch_control.has_more {
                    break;
                }
            }
            Ok(Some(Ok(_))) => break, // Non-initial event means initial data is done
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscription error: {:?}", e)),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    Ok(total_rows)
}

/// Wait for N insert events and return the total rows inserted
pub async fn wait_for_inserts(
    subscription: &mut SubscriptionManager,
    expected_count: usize,
    timeout_duration: Duration,
) -> Result<Vec<serde_json::Value>> {
    let deadline = Instant::now() + timeout_duration;
    let mut all_rows = Vec::new();

    while Instant::now() < deadline && all_rows.len() < expected_count {
        match timeout(Duration::from_millis(100), subscription.next()).await {
            Ok(Some(Ok(ChangeEvent::Insert { rows, .. }))) => {
                all_rows.extend(rows);
            }
            Ok(Some(Ok(ChangeEvent::Ack { .. }))) | Ok(Some(Ok(ChangeEvent::InitialDataBatch { .. }))) => {
                continue;
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscription error: {:?}", e)),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    if all_rows.len() < expected_count {
        return Err(anyhow::anyhow!(
            "Timed out waiting for {} inserts, got {}",
            expected_count,
            all_rows.len()
        ));
    }

    Ok(all_rows)
}

/// Wait for update events
pub async fn wait_for_updates(
    subscription: &mut SubscriptionManager,
    expected_count: usize,
    timeout_duration: Duration,
) -> Result<Vec<serde_json::Value>> {
    let deadline = Instant::now() + timeout_duration;
    let mut all_rows = Vec::new();

    while Instant::now() < deadline && all_rows.len() < expected_count {
        match timeout(Duration::from_millis(100), subscription.next()).await {
            Ok(Some(Ok(ChangeEvent::Update { rows, .. }))) => {
                all_rows.extend(rows);
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscription error: {:?}", e)),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    if all_rows.len() < expected_count {
        return Err(anyhow::anyhow!(
            "Timed out waiting for {} updates, got {}",
            expected_count,
            all_rows.len()
        ));
    }

    Ok(all_rows)
}

/// Wait for delete events
pub async fn wait_for_deletes(
    subscription: &mut SubscriptionManager,
    expected_count: usize,
    timeout_duration: Duration,
) -> Result<Vec<serde_json::Value>> {
    let deadline = Instant::now() + timeout_duration;
    let mut all_rows = Vec::new();

    while Instant::now() < deadline && all_rows.len() < expected_count {
        match timeout(Duration::from_millis(100), subscription.next()).await {
            Ok(Some(Ok(ChangeEvent::Delete { old_rows, .. }))) => {
                all_rows.extend(old_rows);
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => return Err(anyhow::anyhow!("Subscription error: {:?}", e)),
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    if all_rows.len() < expected_count {
        return Err(anyhow::anyhow!(
            "Timed out waiting for {} deletes, got {}",
            expected_count,
            all_rows.len()
        ));
    }

    Ok(all_rows)
}

/// Collect all change events for a duration
pub async fn collect_events(
    subscription: &mut SubscriptionManager,
    duration: Duration,
) -> Vec<ChangeEvent> {
    let deadline = Instant::now() + duration;
    let mut events = Vec::new();

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(50), subscription.next()).await {
            Ok(Some(Ok(event))) => events.push(event),
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => continue,
        }
    }

    events
}

// =============================================================================
// Flush & Storage Assertions
// =============================================================================

/// Wait for flush job to complete
pub async fn wait_for_flush_complete(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
    timeout_duration: Duration,
) -> Result<()> {
    test_support::flush::wait_for_flush_jobs_settled(server, ns, table).await
}

/// Trigger flush and wait for completion
pub async fn flush_and_wait(
    server: &HttpTestServer,
    ns: &str,
    table: &str,
) -> Result<()> {
    test_support::flush::flush_table_and_wait(server, ns, table).await
}

/// Assert that manifest.json exists for a table
pub fn assert_manifest_exists(storage_root: &Path, ns: &str, table: &str) -> Result<()> {
    let manifests = find_files_recursive(storage_root, "manifest.json");
    let matching: Vec<_> = manifests
        .iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            s.contains(ns) && s.contains(table)
        })
        .collect();

    if matching.is_empty() {
        return Err(anyhow::anyhow!(
            "No manifest.json found for {}.{} under {}",
            ns,
            table,
            storage_root.display()
        ));
    }

    Ok(())
}

/// Assert that parquet files exist and are non-empty for a table
pub fn assert_parquet_exists_and_nonempty(
    storage_root: &Path,
    ns: &str,
    table: &str,
) -> Result<()> {
    let parquets = test_support::flush::find_parquet_files(storage_root);
    let matching: Vec<_> = parquets
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            s.contains(ns) && s.contains(table)
        })
        .collect();

    if matching.is_empty() {
        return Err(anyhow::anyhow!(
            "No parquet files found for {}.{} under {}",
            ns,
            table,
            storage_root.display()
        ));
    }

    for p in &matching {
        let size = std::fs::metadata(p)?.len();
        if size == 0 {
            return Err(anyhow::anyhow!("Parquet file {} is empty", p.display()));
        }
    }

    Ok(())
}

/// Find files with a specific name recursively
pub fn find_files_recursive(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(find_files_recursive(&path, filename));
            } else if path.file_name().map(|n| n == filename).unwrap_or(false) {
                results.push(path);
            }
        }
    }
    results
}

/// Assert no duplicates by primary key in query results
pub fn assert_no_duplicates(resp: &QueryResponse, pk_column: &str) -> Result<()> {
    assert_success(resp, "query for duplicate check");

    let pk_idx = get_column_index(resp, pk_column)
        .ok_or_else(|| anyhow::anyhow!("Column {} not found", pk_column))?;

    let mut seen = HashSet::new();
    for row in get_rows(resp) {
        if let Some(pk) = row.get(pk_idx) {
            let pk_str = pk.to_string();
            if !seen.insert(pk_str.clone()) {
                return Err(anyhow::anyhow!("Duplicate primary key: {}", pk_str));
            }
        }
    }

    Ok(())
}

// =============================================================================
// Job Assertions
// =============================================================================

/// Wait for a specific job to reach a terminal state
pub async fn wait_for_job_terminal(
    server: &HttpTestServer,
    job_id: &str,
    timeout_duration: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout_duration;

    while Instant::now() < deadline {
        let sql = format!(
            "SELECT status FROM system.jobs WHERE job_id = '{}'",
            job_id
        );
        let resp = server.execute_sql(&sql).await?;

        if resp.status == ResponseStatus::Success {
            if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
                if let Some(row) = rows.first() {
                    if let Some(status) = row.first().and_then(|v| v.as_str()) {
                        match status {
                            "completed" | "failed" | "cancelled" => {
                                return Ok(status.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    Err(anyhow::anyhow!(
        "Timed out waiting for job {} to reach terminal state",
        job_id
    ))
}

/// Assert job completed successfully
pub async fn assert_job_completed(
    server: &HttpTestServer,
    job_id: &str,
    timeout_duration: Duration,
) -> Result<()> {
    let status = wait_for_job_terminal(server, job_id, timeout_duration).await?;
    if status != "completed" {
        return Err(anyhow::anyhow!(
            "Expected job {} to be completed, got {}",
            job_id,
            status
        ));
    }
    Ok(())
}

/// Assert job failed
pub async fn assert_job_failed(
    server: &HttpTestServer,
    job_id: &str,
    timeout_duration: Duration,
) -> Result<()> {
    let status = wait_for_job_terminal(server, job_id, timeout_duration).await?;
    if status != "failed" {
        return Err(anyhow::anyhow!(
            "Expected job {} to be failed, got {}",
            job_id,
            status
        ));
    }
    Ok(())
}

// =============================================================================
// Parallel Test Utilities
// =============================================================================

/// Run a function for multiple users in parallel
pub async fn run_parallel_users<F, Fut>(
    user_count: usize,
    user_prefix: &str,
    f: F,
) -> Vec<Result<()>>
where
    F: Fn(String, usize) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let handles: Vec<_> = (0..user_count)
        .map(|i| {
            let user_id = format!("{}_{}", user_prefix, i);
            let f = f.clone();
            tokio::spawn(async move { f(user_id, i).await })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!("Task panicked: {:?}", e))));
    }
    results
}

/// Generate unique namespace name for test isolation
pub fn unique_ns(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}_{}", prefix, std::process::id(), id)
}

/// Generate unique table name
pub fn unique_table(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::SeqCst))
}
