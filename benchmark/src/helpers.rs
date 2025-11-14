use crate::models::{BenchmarkMeta, BenchmarkResult, MachineInfo, TestResult};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::Mutex;

// Global lock for benchmark file writes
static BENCHMARK_LOCK: Mutex<()> = Mutex::new(());

/// Get the benchmark results directory
pub fn get_results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("view").join("results")
}

/// Ensure the results directory exists
pub fn ensure_results_dir() -> std::io::Result<PathBuf> {
    let dir = get_results_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Get git commit hash (short)
fn get_git_commit_hash() -> String {
    std::process::Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Get version from Cargo.toml
fn get_version() -> String {
    use std::fs;
    
    let cargo_toml_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml_path) {
        for line in content.lines() {
            if line.trim().starts_with("version = ") {
                if let Some(version) = line.split('=').nth(1) {
                    return version.trim().trim_matches('"').to_string();
                }
            }
        }
    }
    "unknown".to_string()
}

/// Get current git branch
fn get_git_branch() -> String {
    std::process::Command::new("git")
        .args(&["branch", "--show-current"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Generate benchmark filename based on version, branch, and commit
/// Format: bench-<version>--<branch>-<commit>.json
pub fn generate_benchmark_filename() -> String {
    let version = get_version();
    let branch = get_git_branch();
    let commit = get_git_commit_hash();
    format!("bench-{}--{}-{}.json", version, branch, commit)
}

/// Write benchmark result to JSON file
pub fn write_benchmark_result(result: &BenchmarkResult) -> anyhow::Result<PathBuf> {
    let dir = ensure_results_dir()?;
    let filename = generate_benchmark_filename();
    let path = dir.join(&filename);
    
    let json = serde_json::to_string_pretty(result)?;
    let mut file = File::create(&path)?;
    file.write_all(json.as_bytes())?;
    
    Ok(path)
}

/// Append test result to the current benchmark run file (thread-safe)
pub fn append_test_result(test_result: TestResult) -> anyhow::Result<PathBuf> {
    // Acquire lock to prevent concurrent writes
    let _lock = BENCHMARK_LOCK.lock().unwrap();
    
    let dir = ensure_results_dir()?;
    let filename = generate_benchmark_filename();
    let path = dir.join(&filename);
    
    let version = get_version();
    let branch = get_git_branch();
    
    let mut benchmark = if path.exists() {
        let json = fs::read_to_string(&path)?;
        serde_json::from_str::<BenchmarkResult>(&json)?
    } else {
        BenchmarkResult {
            meta: BenchmarkMeta::new(&version, &branch, MachineInfo::detect()),
            tests: Vec::new(),
        }
    };
    
    benchmark.tests.push(test_result);
    
    let json = serde_json::to_string_pretty(&benchmark)?;
    let mut file = File::create(&path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?; // Ensure data is written to disk
    
    Ok(path)
}

/// Measure current process memory usage in MB
#[cfg(target_os = "windows")]
pub fn measure_memory_mb() -> f64 {
    use std::mem;
    
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    
    unsafe {
        let mut counters: ProcessMemoryCounters = mem::zeroed();
        counters.cb = mem::size_of::<ProcessMemoryCounters>() as u32;
        
        let result = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            counters.cb,
        );
        
        if result != 0 {
            (counters.working_set_size as f64) / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

#[cfg(target_os = "linux")]
pub fn measure_memory_mb() -> f64 {
    use std::fs;
    
    if let Ok(content) = fs::read_to_string("/proc/self/statm") {
        // statm format: size resident shared text lib data dt
        // We use resident (RSS) which is in pages
        if let Some(rss_str) = content.split_whitespace().nth(1) {
            if let Ok(pages) = rss_str.parse::<u64>() {
                // Page size is typically 4096 bytes
                let page_size = 4096;
                return (pages * page_size) as f64 / (1024.0 * 1024.0);
            }
        }
    }
    0.0
}

#[cfg(target_os = "macos")]
pub fn measure_memory_mb() -> f64 {
    use std::mem;
    
    #[repr(C)]
    struct TaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: TimeValue,
        system_time: TimeValue,
        policy: i32,
        suspend_count: i32,
    }
    
    #[repr(C)]
    struct TimeValue {
        seconds: i32,
        microseconds: i32,
    }
    
    unsafe {
        let mut info: TaskBasicInfo = mem::zeroed();
        let mut count = (mem::size_of::<TaskBasicInfo>() / mem::size_of::<u32>()) as u32;
        
        // Note: This is a simplified version. Full implementation would need mach kernel APIs
        // For now, return a placeholder
        0.0
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn measure_memory_mb() -> f64 {
    0.0 // Unsupported platform
}

/// Measure disk usage of a directory in MB
pub fn measure_disk_mb<P: AsRef<Path>>(path: P) -> f64 {
    fn dir_size(path: &Path) -> u64 {
        let mut total = 0u64;
        
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        total += dir_size(&entry.path());
                    } else {
                        total += metadata.len();
                    }
                }
            }
        }
        
        total
    }
    
    let bytes = dir_size(path.as_ref());
    bytes as f64 / (1024.0 * 1024.0)
}

/// Execute CLI command and capture timing
pub struct CliExecution {
    pub output: String,
    pub cli_total_ms: f64,
    pub server_time_ms: f64,
    pub overhead_ms: f64,
}

/// Execute SQL via CLI and measure timing
pub fn execute_cli_timed(
    username: &str,
    password: &str,
    sql: &str,
) -> anyhow::Result<CliExecution> {
    use std::process::Command;
    
    // Use environment variable or default to "kalam" binary name
    let kalam_bin = std::env::var("CARGO_BIN_EXE_kalam")
        .unwrap_or_else(|_| "kalam".to_string());
    
    let start = Instant::now();
    let mut command = Command::new(&kalam_bin);
    command
        .arg("-u")
        .arg("http://localhost:8080")
        .arg("--username")
        .arg(username)
        .arg("--password")
        .arg(password)
        .arg("--command")
        .arg(sql);

    if let Ok(timeout_value) = std::env::var("KALAM_CLI_TIMEOUT_SECS") {
        if timeout_value.parse::<u64>().is_ok() {
            command.arg("--timeout").arg(timeout_value);
        }
    }

    let output = command.output()?;
    let cli_total_ms = start.elapsed().as_millis() as f64;
    
    if !output.status.success() {
        anyhow::bail!(
            "CLI command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout).to_string();
    
    // Extract server time from output (looks for "Took: XXX.XXX ms")
    let server_time_ms = output_str
        .lines()
        .find(|l| l.starts_with("Took:"))
        .and_then(|line| {
            line.split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    
    let overhead_ms = cli_total_ms - server_time_ms;
    
    Ok(CliExecution {
        output: output_str,
        cli_total_ms,
        server_time_ms,
        overhead_ms,
    })
}

/// Execute SQL as root user
pub fn execute_cli_timed_root(sql: &str) -> anyhow::Result<CliExecution> {
    execute_cli_timed("root", "", sql)
}

/// Wait for a flush job to complete
pub fn wait_for_flush_completion(job_id: &str, timeout: Duration) -> anyhow::Result<()> {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(200);
    
    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("Timeout waiting for flush job {} to complete", job_id);
        }
        
        let query = format!(
            "SELECT status FROM system.jobs WHERE job_id = '{}'",
            job_id
        );
        
        let result = execute_cli_timed_root(&query)?;
        
        if result.output.to_lowercase().contains("completed") {
            return Ok(());
        }
        
        if result.output.to_lowercase().contains("failed") {
            anyhow::bail!("Flush job {} failed", job_id);
        }
        
        std::thread::sleep(poll_interval);
    }
}

/// Parse job ID from FLUSH TABLE output
pub fn parse_job_id_from_flush(output: &str) -> anyhow::Result<String> {
    if let Some(idx) = output.find("Job ID: ") {
        let after = &output[idx + "Job ID: ".len()..];
        let first_line = after.lines().next().unwrap_or(after);
        let id_token = first_line
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing job id token"))?;
        return Ok(id_token.trim().to_string());
    }
    
    anyhow::bail!("Failed to parse job ID from FLUSH output: {}", output)
}

/// Setup benchmark namespace and tables
pub fn setup_benchmark_tables() -> anyhow::Result<()> {
    // Create namespaces
    for ns in &["bench_user", "bench_shared", "bench_stream"] {
        let sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", ns);
        execute_cli_timed_root(&sql)?;
        std::thread::sleep(Duration::from_millis(100));
    }
    
    // Create user table
    let user_table_sql = r#"
        CREATE USER TABLE IF NOT EXISTS bench_user.items (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            value TEXT,
            timestamp TIMESTAMP DEFAULT NOW()
        ) FLUSH ROWS 100000
    "#;
    execute_cli_timed_root(user_table_sql)?;
    std::thread::sleep(Duration::from_millis(100));
    
    // Create shared table
    let shared_table_sql = r#"
        CREATE SHARED TABLE IF NOT EXISTS bench_shared.items (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            value TEXT,
            timestamp TIMESTAMP DEFAULT NOW()
        ) FLUSH ROWS 100000
    "#;
    execute_cli_timed_root(shared_table_sql)?;
    std::thread::sleep(Duration::from_millis(100));
    
    // Create stream table
    let stream_table_sql = r#"
        CREATE STREAM TABLE IF NOT EXISTS bench_stream.events (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            value TEXT,
            timestamp TIMESTAMP DEFAULT NOW()
        ) TTL 10
    "#;
    execute_cli_timed_root(stream_table_sql)?;
    std::thread::sleep(Duration::from_millis(100));
    
    Ok(())
}

/// Cleanup benchmark tables
pub fn cleanup_benchmark_tables() -> anyhow::Result<()> {
    let tables = [
        "bench_user.items",
        "bench_shared.items",
        "bench_stream.events",
    ];
    
    for table in &tables {
        let sql = format!("DROP TABLE IF EXISTS {}", table);
        let _ = execute_cli_timed_root(&sql);
        std::thread::sleep(Duration::from_millis(50));
    }
    
    Ok(())
}

/// Generate benchmark data value
pub fn generate_value(index: usize) -> String {
    format!("benchmark_value_{}", index)
}
