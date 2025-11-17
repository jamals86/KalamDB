use serde::{Deserialize, Serialize};

/// Top-level benchmark result file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub meta: BenchmarkMeta,
    pub tests: Vec<TestResult>,
}

/// Metadata about the benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMeta {
    pub version: String,
    pub branch: String,
    pub timestamp: String, // ISO 8601 format
    pub machine: MachineInfo,
}

/// Machine information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineInfo {
    pub cpu: String,
    pub os: String,
    pub memory_total_mb: u64,
    pub memory_free_mb: u64,
    pub disk_type: String,
    pub disk_free_mb: u64,
}

/// Individual test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub id: String,
    pub group: TestGroup,
    pub subcategory: String,
    pub description: String,
    pub cli_total_time_ms: f64,
    pub api_roundtrip_ms: f64,
    pub sql_execution_ms: f64,
    pub requests: u64,
    pub avg_request_ms: f64,
    pub memory_before_mb: f64,
    pub memory_after_mb: f64,
    pub disk_before_mb: f64,
    pub disk_after_mb: f64,
    pub errors: Vec<String>,
}

/// Test group categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestGroup {
    UserTable,
    SharedTable,
    StreamTable,
    SystemTables,
    Concurrency,
}

impl TestResult {
    /// Create a new test result with default values
    pub fn new(id: &str, group: TestGroup, subcategory: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            group,
            subcategory: subcategory.to_string(),
            description: description.to_string(),
            cli_total_time_ms: 0.0,
            api_roundtrip_ms: 0.0,
            sql_execution_ms: 0.0,
            requests: 0,
            avg_request_ms: 0.0,
            memory_before_mb: 0.0,
            memory_after_mb: 0.0,
            disk_before_mb: 0.0,
            disk_after_mb: 0.0,
            errors: Vec::new(),
        }
    }

    /// Add an error to the result
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    /// Set timing information
    pub fn set_timings(&mut self, cli_ms: f64, api_ms: f64, sql_ms: f64) {
        self.cli_total_time_ms = cli_ms;
        self.api_roundtrip_ms = api_ms;
        self.sql_execution_ms = sql_ms;
    }

    /// Set memory information
    pub fn set_memory(&mut self, before_mb: f64, after_mb: f64) {
        self.memory_before_mb = before_mb;
        self.memory_after_mb = after_mb;
    }

    /// Set disk information
    pub fn set_disk(&mut self, before_mb: f64, after_mb: f64) {
        self.disk_before_mb = before_mb;
        self.disk_after_mb = after_mb;
    }

    /// Set request metrics
    pub fn set_requests(&mut self, count: u64, avg_ms: f64) {
        self.requests = count;
        self.avg_request_ms = avg_ms;
    }

    /// Validate the test result
    pub fn validate(&mut self) {
        if self.api_roundtrip_ms == 0.0 || self.avg_request_ms == 0.0 {
            self.add_error("request_time_zero".to_string());
        }
    }
}

impl BenchmarkMeta {
    /// Create metadata with current timestamp
    pub fn new(version: &str, branch: &str, machine: MachineInfo) -> Self {
        Self {
            version: version.to_string(),
            branch: branch.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            machine,
        }
    }
}

impl MachineInfo {
    /// Detect current machine information (best effort)
    pub fn detect() -> Self {
        Self {
            cpu: Self::detect_cpu(),
            os: Self::detect_os(),
            memory_total_mb: Self::detect_memory_total(),
            memory_free_mb: Self::detect_memory_free(),
            disk_type: "Unknown".to_string(),
            disk_free_mb: Self::detect_disk_free(),
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_cpu() -> String {
        std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "Unknown CPU".to_string())
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_cpu() -> String {
        use std::process::Command;

        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = Command::new("lscpu").output() {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    for line in s.lines() {
                        if line.starts_with("Model name:") {
                            return line
                                .split(':')
                                .nth(1)
                                .unwrap_or("Unknown")
                                .trim()
                                .to_string();
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = Command::new("sysctl")
                .arg("-n")
                .arg("machdep.cpu.brand_string")
                .output()
            {
                if let Ok(s) = String::from_utf8(output.stdout) {
                    return s.trim().to_string();
                }
            }
        }

        "Unknown CPU".to_string()
    }

    fn detect_os() -> String {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }

    #[cfg(target_os = "windows")]
    fn detect_memory_total() -> u64 {
        // Best effort - return a default value on Windows
        16384 // Default to 16GB
    }

    #[cfg(target_os = "linux")]
    fn detect_memory_total() -> u64 {
        use std::fs;
        if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb / 1024; // Convert to MB
                        }
                    }
                }
            }
        }
        16384 // Default
    }

    #[cfg(target_os = "macos")]
    fn detect_memory_total() -> u64 {
        use std::process::Command;
        if let Ok(output) = Command::new("sysctl").arg("-n").arg("hw.memsize").output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                if let Ok(bytes) = s.trim().parse::<u64>() {
                    return bytes / (1024 * 1024); // Convert to MB
                }
            }
        }
        16384 // Default
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn detect_memory_total() -> u64 {
        16384 // Default to 16GB
    }

    #[cfg(target_os = "windows")]
    fn detect_memory_free() -> u64 {
        // Best effort - return a default value on Windows
        8192 // Default to 8GB
    }

    #[cfg(target_os = "linux")]
    fn detect_memory_free() -> u64 {
        use std::fs;
        if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemAvailable:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb / 1024; // Convert to MB
                        }
                    }
                }
            }
        }
        8192 // Default
    }

    #[cfg(target_os = "macos")]
    fn detect_memory_free() -> u64 {
        use std::process::Command;
        if let Ok(output) = Command::new("vm_stat").output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                let mut pages_free = 0u64;
                for line in s.lines() {
                    if line.starts_with("Pages free:") {
                        if let Some(num_str) = line.split_whitespace().nth(2) {
                            if let Ok(num) = num_str.trim_end_matches('.').parse::<u64>() {
                                pages_free = num;
                                break;
                            }
                        }
                    }
                }
                // Page size is typically 4096 bytes
                return (pages_free * 4096) / (1024 * 1024); // Convert to MB
            }
        }
        8192 // Default
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn detect_memory_free() -> u64 {
        8192 // Default to 8GB
    }

    fn detect_disk_free() -> u64 {
        // Simplified - just return a default
        500000 // 500GB default
    }
}
