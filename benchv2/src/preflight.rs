use std::time::Instant;

use crate::client::KalamClient;
use crate::config::Config;

/// Result of a single pre-flight check.
#[derive(Debug)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckResult {
    fn pass(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }
    fn warn(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }
    fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }

    fn icon(&self) -> &str {
        match self.status {
            CheckStatus::Pass => "✅",
            CheckStatus::Warn => "⚠️ ",
            CheckStatus::Fail => "❌",
        }
    }

    fn color_code(&self) -> &str {
        match self.status {
            CheckStatus::Pass => "\x1b[32m",
            CheckStatus::Warn => "\x1b[33m",
            CheckStatus::Fail => "\x1b[31m",
        }
    }
}

/// Run all pre-flight checks. Returns true if no checks failed (warnings are OK).
pub async fn run_preflight_checks(client: &KalamClient, config: &Config) -> bool {
    println!("Pre-flight Checks");
    println!("─────────────────────────────────────────────────");

    let mut checks: Vec<CheckResult> = Vec::new();

    // 1. File descriptor limit (Unix only)
    checks.push(check_fd_limit());

    // 2. Server health
    checks.push(check_server_health(client, config).await);

    // 3. SQL connectivity
    checks.push(check_sql_connectivity(client).await);

    // 4. Admin permissions
    checks.push(check_admin_permissions(client, config).await);

    // 5. Clean state (no leftover bench namespaces)
    checks.push(check_clean_state(client).await);

    // 6. bcrypt cost (measure user creation speed as proxy)
    checks.push(check_bcrypt_cost(client).await);

    // Print results
    for check in &checks {
        println!(
            "  {} {}{:<24}\x1b[0m {}",
            check.icon(),
            check.color_code(),
            check.name,
            check.detail,
        );
    }
    println!();

    let failed = checks.iter().any(|c| c.status == CheckStatus::Fail);
    let warned = checks.iter().any(|c| c.status == CheckStatus::Warn);

    if failed {
        println!("\x1b[31m  ✖ Pre-flight checks FAILED. Fix the issues above before running benchmarks.\x1b[0m\n");
        return false;
    }
    if warned {
        println!("\x1b[33m  ⚡ Some warnings detected — benchmarks will proceed but results may be affected.\x1b[0m\n");
    } else {
        println!("\x1b[32m  ✔ All pre-flight checks passed.\x1b[0m\n");
    }

    true
}

/// Check the process file descriptor limit.
fn check_fd_limit() -> CheckResult {
    #[cfg(unix)]
    {
        use std::process::Command;
        // Try to read the soft limit via getrlimit
        let output = Command::new("sh")
            .arg("-c")
            .arg("ulimit -n")
            .output();
        match output {
            Ok(out) => {
                let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(n) = val.parse::<u64>() {
                    if n >= 8192 {
                        CheckResult::pass(
                            "File descriptors",
                            format!("{} (>= 8192)", n),
                        )
                    } else if n >= 1024 {
                        CheckResult::warn(
                            "File descriptors",
                            format!("{} — recommend >= 8192 (`ulimit -n 65536`)", n),
                        )
                    } else {
                        CheckResult::fail(
                            "File descriptors",
                            format!("{} — too low, run `ulimit -n 65536`", n),
                        )
                    }
                } else {
                    CheckResult::warn("File descriptors", format!("Could not parse: {}", val))
                }
            }
            Err(e) => CheckResult::warn("File descriptors", format!("Could not check: {}", e)),
        }
    }
    #[cfg(not(unix))]
    {
        CheckResult::pass("File descriptors", "N/A (non-Unix)")
    }
}

/// Check server health endpoint.
async fn check_server_health(client: &KalamClient, config: &Config) -> CheckResult {
    if client.health_check().await {
        CheckResult::pass("Server reachable", config.url.clone())
    } else {
        CheckResult::fail(
            "Server reachable",
            format!("Cannot reach {}", config.url),
        )
    }
}

/// Check SQL connectivity with SELECT 1.
async fn check_sql_connectivity(client: &KalamClient) -> CheckResult {
    match client.sql_ok("SELECT 1").await {
        Ok(_) => CheckResult::pass("SQL connectivity", "SELECT 1 returned OK"),
        Err(e) => CheckResult::fail("SQL connectivity", format!("Failed: {}", e)),
    }
}

/// Check admin permissions by creating and dropping a test namespace.
async fn check_admin_permissions(client: &KalamClient, config: &Config) -> CheckResult {
    let test_ns = format!("{}_preflight_check", config.namespace);
    match client
        .sql_ok(&format!("CREATE NAMESPACE IF NOT EXISTS {}", test_ns))
        .await
    {
        Ok(_) => {
            let _ = client
                .sql(&format!("DROP NAMESPACE IF EXISTS {}", test_ns))
                .await;
            CheckResult::pass("Admin permissions", "Can create/drop namespaces")
        }
        Err(e) => CheckResult::fail("Admin permissions", format!("Cannot create namespace: {}", e)),
    }
}

/// Check for leftover benchmark namespaces.
async fn check_clean_state(client: &KalamClient) -> CheckResult {
    match client.sql_ok("SHOW NAMESPACES").await {
        Ok(resp) => {
            let mut stale = Vec::new();
            if let Some(result) = resp.results.first() {
                if let Some(rows) = &result.rows {
                    for row in rows {
                        if let Some(serde_json::Value::String(ns)) = row.first() {
                            if ns.starts_with("bench_") {
                                stale.push(ns.clone());
                            }
                        }
                    }
                }
            }
            if stale.is_empty() {
                CheckResult::pass("Clean state", "No leftover bench_* namespaces")
            } else {
                CheckResult::warn(
                    "Clean state",
                    format!(
                        "Found {} stale bench_* namespace(s) — they won't interfere (unique run IDs)",
                        stale.len()
                    ),
                )
            }
        }
        Err(e) => CheckResult::warn("Clean state", format!("Could not check: {}", e)),
    }
}

/// Measure user creation speed as a proxy for bcrypt cost.
/// If creating a user takes > 50ms, bcrypt cost is likely too high for benchmarks.
async fn check_bcrypt_cost(client: &KalamClient) -> CheckResult {
    let test_user = "bench_preflight_bcrypt_test";
    let test_pass = "TestPass123!";

    let start = Instant::now();
    let create_result = client
        .sql(&format!(
            "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
            test_user, test_pass
        ))
        .await;
    let elapsed = start.elapsed();

    // Always try to clean up
    let _ = client
        .sql(&format!("DROP USER IF EXISTS '{}'", test_user))
        .await;

    match create_result {
        Ok(resp) => {
            let ms = elapsed.as_millis();
            if resp.status == "success" || resp.error.as_ref().map_or(false, |e| e.message.contains("already exists")) {
                if ms < 50 {
                    CheckResult::pass(
                        "bcrypt cost",
                        format!("User creation took {}ms (cost is low ✓)", ms),
                    )
                } else if ms < 200 {
                    CheckResult::warn(
                        "bcrypt cost",
                        format!(
                            "User creation took {}ms — set bcrypt_cost = 4 in server.toml for benchmarks",
                            ms
                        ),
                    )
                } else {
                    CheckResult::fail(
                        "bcrypt cost",
                        format!(
                            "User creation took {}ms — bcrypt_cost is too high, set to 4 in server.toml",
                            ms
                        ),
                    )
                }
            } else {
                let msg = resp.error.map(|e| e.message).unwrap_or_default();
                CheckResult::warn("bcrypt cost", format!("Could not test: {}", msg))
            }
        }
        Err(e) => CheckResult::warn("bcrypt cost", format!("Could not test: {}", e)),
    }
}
