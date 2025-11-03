#![allow(deprecated)]
//! Stress test utilities for concurrent load generation and monitoring
//!
//! This module provides reusable utilities for stress testing KalamDB:
//! - Concurrent writer thread spawning with configurable insert rate
//! - WebSocket subscription spawning with connection monitoring
//! - Memory monitoring with periodic measurement
//! - CPU usage measurement
//!
//! **Usage Example**:
//! ```rust
//! use stress_utils::{ConcurrentWriters, MemoryMonitor};
//!
//! let writers = ConcurrentWriters::new(10, 1000); // 10 writers, 1000 inserts/sec total
//! let monitor = MemoryMonitor::new(Duration::from_secs(30));
//!
//! writers.start().await;
//! monitor.start();
//!
//! tokio::time::sleep(Duration::from_secs(300)).await;
//!
//! let stats = monitor.stop();
//! writers.stop().await;
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

/// Configuration for concurrent writers
pub struct WriterConfig {
    /// Number of concurrent writer threads
    pub writer_count: usize,
    /// Target insert rate (inserts per second, distributed across all writers)
    pub target_rate: usize,
    /// Namespace to write to
    pub namespace: String,
    /// Table name to write to
    pub table_name: String,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            writer_count: 10,
            target_rate: 1000,
            namespace: "default".to_string(),
            table_name: "stress_test".to_string(),
        }
    }
}

/// Concurrent writers for stress testing
pub struct ConcurrentWriters {
    config: WriterConfig,
    running: Arc<AtomicBool>,
    insert_count: Arc<AtomicU64>,
    handles: Vec<JoinHandle<()>>,
}

impl ConcurrentWriters {
    /// Create new concurrent writers
    pub fn new(config: WriterConfig) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            insert_count: Arc::new(AtomicU64::new(0)),
            handles: Vec::new(),
        }
    }

    /// Start concurrent writers
    ///
    /// Spawns writer threads that insert data at the configured rate.
    /// Rate is distributed evenly across all writers.
    pub async fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);

        let inserts_per_writer = self.config.target_rate / self.config.writer_count;
        let interval = Duration::from_millis(1000 / inserts_per_writer as u64);

        for _i in 0..self.config.writer_count {
            let running = Arc::clone(&self.running);
            let insert_count = Arc::clone(&self.insert_count);
            let _namespace = self.config.namespace.clone();
            let _table_name = self.config.table_name.clone();

            let handle = tokio::spawn(async move {
                while running.load(Ordering::SeqCst) {
                    // TODO: T230 - Implement actual insert logic
                    // For now, just simulate inserts

                    insert_count.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(interval).await;
                }
            });

            self.handles.push(handle);
        }
    }

    /// Stop all writers
    pub async fn stop(&mut self) -> WriterStats {
        self.running.store(false, Ordering::SeqCst);

        // Wait for all handles to complete
        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }

        WriterStats {
            total_inserts: self.insert_count.load(Ordering::SeqCst),
        }
    }
}

/// Statistics from writer run
pub struct WriterStats {
    pub total_inserts: u64,
}

/// Memory monitoring with periodic measurements
pub struct MemoryMonitor {
    interval: Duration,
    running: Arc<AtomicBool>,
    measurements: Arc<tokio::sync::Mutex<Vec<MemoryMeasurement>>>,
    handle: Option<JoinHandle<()>>,
}

/// Single memory measurement
#[derive(Debug, Clone)]
pub struct MemoryMeasurement {
    pub rss_bytes: usize,
}

impl MemoryMonitor {
    /// Create new memory monitor
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            running: Arc::new(AtomicBool::new(false)),
            measurements: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            handle: None,
        }
    }

    /// Start monitoring memory
    pub fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let measurements = Arc::clone(&self.measurements);
        let interval = self.interval;

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                // TODO: T232 - Implement actual memory measurement
                // For now, use a placeholder
                let measurement = MemoryMeasurement {
                    rss_bytes: get_process_memory(),
                };

                measurements.lock().await.push(measurement);
                tokio::time::sleep(interval).await;
            }
        });

        self.handle = Some(handle);
    }

    /// Stop monitoring and return all measurements
    pub async fn stop(&mut self) -> Vec<MemoryMeasurement> {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        let measurements = self.measurements.lock().await;
        measurements.clone()
    }

    /// Get memory growth percentage from first to last measurement
    pub async fn memory_growth_percentage(&self) -> Option<f64> {
        let measurements = self.measurements.lock().await;

        if measurements.len() < 2 {
            return None;
        }

        let first = measurements.first().unwrap().rss_bytes as f64;
        let last = measurements.last().unwrap().rss_bytes as f64;

        Some(((last - first) / first) * 100.0)
    }
}

/// Get current process memory (RSS in bytes)
///
/// Platform-specific implementation:
/// - Windows: Uses `GetProcessMemoryInfo` from psapi.dll
/// - Linux: Parses `/proc/self/status`
/// - macOS: Uses `task_info`
fn get_process_memory() -> usize {
    #[cfg(target_os = "windows")]
    {
        // Windows implementation using GetProcessMemoryInfo
        use std::mem;
        use std::ptr;

        // Define necessary Windows API structures and functions
        #[repr(C)]
        struct PROCESS_MEMORY_COUNTERS {
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

        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                pmc: *mut PROCESS_MEMORY_COUNTERS,
                cb: u32,
            ) -> i32;
        }

        unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS = mem::zeroed();
            pmc.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

            let result = GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb);

            if result != 0 {
                pmc.working_set_size
            } else {
                0
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux implementation reading /proc/self/status
        use std::fs;

        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    // Format: "VmRSS:    123456 kB"
                    if let Some(value_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = value_str.parse::<usize>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
        0
    }

    #[cfg(target_os = "macos")]
    {
        // macOS implementation using task_info via libc
        use libc::{
            kern_return_t, mach_msg_type_number_t, mach_task_basic_info_data_t, mach_task_self,
            task_info, KERN_SUCCESS, MACH_TASK_BASIC_INFO,
        };
        use std::mem;

        unsafe {
            let mut info: mach_task_basic_info_data_t = mem::zeroed();
            let mut count: mach_msg_type_number_t =
                (mem::size_of::<mach_task_basic_info_data_t>() / mem::size_of::<i32>()) as _;

            let result: kern_return_t = task_info(
                mach_task_self(),
                MACH_TASK_BASIC_INFO,
                (&mut info as *mut mach_task_basic_info_data_t).cast(),
                &mut count,
            );

            if result == KERN_SUCCESS {
                info.resident_size as usize
            } else {
                0
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        // Fallback for other platforms
        0
    }
}

/// CPU usage monitoring
pub struct CpuMonitor {
    interval: Duration,
    running: Arc<AtomicBool>,
    measurements: Arc<tokio::sync::Mutex<Vec<CpuMeasurement>>>,
    handle: Option<JoinHandle<()>>,
    last_cpu_time: Arc<tokio::sync::Mutex<Option<CpuTimes>>>,
}

/// CPU time measurements for calculating usage percentage
#[derive(Debug, Clone, Copy)]
struct CpuTimes {
    user_time: u64,
    system_time: u64,
    timestamp: Instant,
}

/// Single CPU measurement
#[derive(Debug, Clone)]
pub struct CpuMeasurement {
    pub cpu_percent: f64,
}

impl CpuMonitor {
    /// Create new CPU monitor
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            running: Arc::new(AtomicBool::new(false)),
            measurements: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            handle: None,
            last_cpu_time: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Start monitoring CPU
    pub fn start(&mut self) {
        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let measurements = Arc::clone(&self.measurements);
        let last_cpu_time = Arc::clone(&self.last_cpu_time);
        let interval = self.interval;

        let handle = tokio::spawn(async move {
            // Initialize with first measurement
            if let Some(times) = get_process_cpu_times() {
                *last_cpu_time.lock().await = Some(times);
            }

            tokio::time::sleep(interval).await;

            while running.load(Ordering::SeqCst) {
                let cpu_percent = {
                    let mut last = last_cpu_time.lock().await;

                    if let Some(current_times) = get_process_cpu_times() {
                        let cpu = if let Some(prev_times) = *last {
                            calculate_cpu_percent(prev_times, current_times)
                        } else {
                            0.0
                        };

                        *last = Some(current_times);
                        cpu
                    } else {
                        0.0
                    }
                };

                let measurement = CpuMeasurement { cpu_percent };

                measurements.lock().await.push(measurement);
                tokio::time::sleep(interval).await;
            }
        });

        self.handle = Some(handle);
    }

    /// Stop monitoring and return all measurements
    pub async fn stop(&mut self) -> Vec<CpuMeasurement> {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        let measurements = self.measurements.lock().await;
        measurements.clone()
    }
}

/// Get current process CPU times (user and system in 100-nanosecond units or similar)
///
/// Platform-specific implementation:
/// - Windows: Uses `GetProcessTimes`
/// - Linux: Parses `/proc/self/stat`
/// - macOS: Uses `task_info`
fn get_process_cpu_times() -> Option<CpuTimes> {
    #[cfg(target_os = "windows")]
    {
        use std::mem;

        #[repr(C)]
        struct FILETIME {
            dw_low_date_time: u32,
            dw_high_date_time: u32,
        }

        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessTimes(
                process: *mut std::ffi::c_void,
                creation_time: *mut FILETIME,
                exit_time: *mut FILETIME,
                kernel_time: *mut FILETIME,
                user_time: *mut FILETIME,
            ) -> i32;
        }

        unsafe {
            let mut creation: FILETIME = mem::zeroed();
            let mut exit: FILETIME = mem::zeroed();
            let mut kernel: FILETIME = mem::zeroed();
            let mut user: FILETIME = mem::zeroed();

            let result = GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            );

            if result != 0 {
                // Convert FILETIME to u64 (100-nanosecond intervals)
                let user_time =
                    ((user.dw_high_date_time as u64) << 32) | (user.dw_low_date_time as u64);
                let system_time =
                    ((kernel.dw_high_date_time as u64) << 32) | (kernel.dw_low_date_time as u64);

                Some(CpuTimes {
                    user_time,
                    system_time,
                    timestamp: Instant::now(),
                })
            } else {
                None
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::fs;

        // Read /proc/self/stat
        // Format: pid (comm) state ppid ... utime stime ...
        if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
            let fields: Vec<&str> = stat.split_whitespace().collect();

            // utime is field 13 (index 13), stime is field 14 (index 14)
            // Values are in clock ticks
            if fields.len() > 14 {
                if let (Ok(utime), Ok(stime)) =
                    (fields[13].parse::<u64>(), fields[14].parse::<u64>())
                {
                    // Convert clock ticks to milliseconds
                    // Linux uses 100 Hz clock (USER_HZ), so 1 tick = 10ms
                    let user_time = utime * 10_000; // Convert to microseconds
                    let system_time = stime * 10_000;

                    return Some(CpuTimes {
                        user_time,
                        system_time,
                        timestamp: Instant::now(),
                    });
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        use std::mem;

        #[repr(C)]
        struct task_basic_info {
            suspend_count: i32,
            virtual_size: u32,
            resident_size: u32,
            user_time: [u32; 2],   // seconds, microseconds
            system_time: [u32; 2], // seconds, microseconds
            policy: i32,
        }

        extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(
                task: u32,
                flavor: i32,
                task_info: *mut task_basic_info,
                task_info_count: *mut u32,
            ) -> i32;
        }

        const TASK_BASIC_INFO: i32 = 5;

        unsafe {
            let mut info: task_basic_info = mem::zeroed();
            let mut count = (mem::size_of::<task_basic_info>() / mem::size_of::<u32>()) as u32;

            let result = task_info(mach_task_self(), TASK_BASIC_INFO, &mut info, &mut count);

            if result == 0 {
                // Convert to microseconds
                let user_time = (info.user_time[0] as u64 * 1_000_000) + info.user_time[1] as u64;
                let system_time =
                    (info.system_time[0] as u64 * 1_000_000) + info.system_time[1] as u64;

                Some(CpuTimes {
                    user_time,
                    system_time,
                    timestamp: Instant::now(),
                })
            } else {
                None
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Calculate CPU percentage from two measurements
fn calculate_cpu_percent(prev: CpuTimes, current: CpuTimes) -> f64 {
    let elapsed = current.timestamp.duration_since(prev.timestamp);
    let elapsed_micros = elapsed.as_micros() as u64;

    if elapsed_micros == 0 {
        return 0.0;
    }

    // Calculate total CPU time used (in microseconds or 100-nanosecond units)
    let cpu_time_used =
        (current.user_time + current.system_time).saturating_sub(prev.user_time + prev.system_time);

    // On Windows, times are in 100-nanosecond units, convert to microseconds
    #[cfg(target_os = "windows")]
    let cpu_time_micros = cpu_time_used / 10;

    // On Linux and macOS, already in microseconds
    #[cfg(not(target_os = "windows"))]
    let cpu_time_micros = cpu_time_used;

    // Calculate percentage
    let cpu_percent = (cpu_time_micros as f64 / elapsed_micros as f64) * 100.0;

    // Cap at reasonable values (multi-core can exceed 100%)
    cpu_percent.min(800.0) // Max 800% for 8 cores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_writers_basic() {
        let config = WriterConfig {
            writer_count: 2,
            target_rate: 10,
            ..Default::default()
        };

        let mut writers = ConcurrentWriters::new(config);
        writers.start().await;

        tokio::time::sleep(Duration::from_secs(1)).await;

        let stats = writers.stop().await;

        // Should have approximately 10 inserts (may vary due to timing)
        assert!(stats.total_inserts >= 5 && stats.total_inserts <= 15);
    }

    #[tokio::test]
    async fn test_memory_monitor_basic() {
        let mut monitor = MemoryMonitor::new(Duration::from_millis(100));
        monitor.start();

        tokio::time::sleep(Duration::from_millis(350)).await;

        let measurements = monitor.stop().await;

        // Should have 3-4 measurements
        assert!(measurements.len() >= 3);

        // All measurements should have non-zero memory
        for m in &measurements {
            assert!(m.rss_bytes > 0, "Memory measurement should be non-zero");
        }
    }

    #[tokio::test]
    async fn test_cpu_monitor_basic() {
        let mut monitor = CpuMonitor::new(Duration::from_millis(200));
        monitor.start();

        // Do some work to generate CPU usage
        let _work: Vec<_> = (0..1000).map(|i| i * i).collect();

        tokio::time::sleep(Duration::from_millis(600)).await;

        let measurements = monitor.stop().await;

        // Should have 2-3 measurements
        assert!(measurements.len() >= 2);

        // CPU percentage should be reasonable (0-800% for up to 8 cores)
        for m in &measurements {
            assert!(
                m.cpu_percent >= 0.0 && m.cpu_percent <= 800.0,
                "CPU percent should be between 0 and 800, got: {}",
                m.cpu_percent
            );
        }
    }

    #[tokio::test]
    async fn test_memory_growth_calculation() {
        let mut monitor = MemoryMonitor::new(Duration::from_millis(100));
        monitor.start();

        tokio::time::sleep(Duration::from_millis(300)).await;

        let growth = monitor.memory_growth_percentage().await;

        // Should have growth percentage (may be positive or negative)
        assert!(growth.is_some());

        let _measurements = monitor.stop().await;
    }
}
