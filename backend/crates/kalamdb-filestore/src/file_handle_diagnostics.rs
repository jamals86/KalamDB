//! File Handle Diagnostics
//!
//! **Task #184**: Track file handle usage to detect and diagnose leaks.
//!
//! This module provides:
//! - Atomic counter for active file handles
//! - Logging when files are opened/closed
//! - Diagnostic APIs for monitoring
//!
//! ## Usage
//!
//! Use `tracked_open` and `tracked_close` around file operations:
//!
//! ```rust,ignore
//! use kalamdb_filestore::file_handle_diagnostics::{FileHandleTracker, record_open, record_close};
//!
//! // When opening a file
//! let file = File::open(path)?;
//! record_open("parquet_reader", path);
//!
//! // When done with file
//! drop(file);
//! record_close("parquet_reader", path);
//! ```
//!
//! ## Diagnostics
//!
//! ```rust,ignore
//! // Get current stats
//! let stats = FileHandleTracker::stats();
//! println!("Active handles: {}", stats.active_handles);
//! ```

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use log::{debug, warn};

/// Global file handle tracker
static TRACKER: FileHandleTracker = FileHandleTracker::new();

/// Tracks file handle statistics
pub struct FileHandleTracker {
    /// Currently open file handles
    active_handles: AtomicU64,
    /// Total files opened since startup
    total_opened: AtomicU64,
    /// Total files closed since startup
    total_closed: AtomicU64,
    /// Peak concurrent handles observed
    peak_handles: AtomicU64,
}

/// Snapshot of file handle statistics
#[derive(Debug, Clone, Copy)]
pub struct FileHandleStats {
    /// Currently open file handles
    pub active_handles: u64,
    /// Total files opened since startup
    pub total_opened: u64,
    /// Total files closed since startup
    pub total_closed: u64,
    /// Peak concurrent handles observed
    pub peak_handles: u64,
}

impl FileHandleTracker {
    /// Create a new tracker
    const fn new() -> Self {
        Self {
            active_handles: AtomicU64::new(0),
            total_opened: AtomicU64::new(0),
            total_closed: AtomicU64::new(0),
            peak_handles: AtomicU64::new(0),
        }
    }

    /// Get current statistics
    pub fn stats() -> FileHandleStats {
        FileHandleStats {
            active_handles: TRACKER.active_handles.load(Ordering::Relaxed),
            total_opened: TRACKER.total_opened.load(Ordering::Relaxed),
            total_closed: TRACKER.total_closed.load(Ordering::Relaxed),
            peak_handles: TRACKER.peak_handles.load(Ordering::Relaxed),
        }
    }

    /// Reset all counters (for testing)
    pub fn reset() {
        TRACKER.active_handles.store(0, Ordering::Relaxed);
        TRACKER.total_opened.store(0, Ordering::Relaxed);
        TRACKER.total_closed.store(0, Ordering::Relaxed);
        TRACKER.peak_handles.store(0, Ordering::Relaxed);
    }
}

/// Record a file being opened
///
/// # Arguments
/// * `context` - Description of what's opening the file (e.g., "parquet_reader")
/// * `path` - Path being opened (for logging)
pub fn record_open<P: AsRef<Path>>(context: &str, path: P) {
    let path = path.as_ref();
    let new_count = TRACKER.active_handles.fetch_add(1, Ordering::SeqCst) + 1;
    TRACKER.total_opened.fetch_add(1, Ordering::Relaxed);
    
    // Update peak if needed
    let mut current_peak = TRACKER.peak_handles.load(Ordering::Relaxed);
    while new_count > current_peak {
        match TRACKER.peak_handles.compare_exchange_weak(
            current_peak,
            new_count,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current_peak = actual,
        }
    }

    debug!(
        "[FILE_HANDLE] OPEN: context={}, path={}, active={}, total_opened={}",
        context,
        path.display(),
        new_count,
        TRACKER.total_opened.load(Ordering::Relaxed)
    );

    // Warn if handles are getting high
    if new_count > 1000 {
        warn!(
            "[FILE_HANDLE] HIGH HANDLE COUNT: {} handles open (context={}, path={})",
            new_count,
            context,
            path.display()
        );
    }
}

/// Record a file being closed
///
/// # Arguments
/// * `context` - Description of what's closing the file
/// * `path` - Path being closed (for logging)
pub fn record_close<P: AsRef<Path>>(context: &str, path: P) {
    let path = path.as_ref();
    let old_count = TRACKER.active_handles.fetch_sub(1, Ordering::SeqCst);
    TRACKER.total_closed.fetch_add(1, Ordering::Relaxed);

    debug!(
        "[FILE_HANDLE] CLOSE: context={}, path={}, active={}, total_closed={}",
        context,
        path.display(),
        old_count.saturating_sub(1),
        TRACKER.total_closed.load(Ordering::Relaxed)
    );

    // Detect underflow (more closes than opens - indicates bug)
    if old_count == 0 {
        warn!(
            "[FILE_HANDLE] UNDERFLOW: Closed more files than opened! context={}, path={}",
            context,
            path.display()
        );
    }
}

/// Check for potential leaks (more opens than closes)
///
/// Returns Some(count) if there's a mismatch, None if balanced
pub fn check_for_leaks() -> Option<i64> {
    let opened = TRACKER.total_opened.load(Ordering::Relaxed) as i64;
    let closed = TRACKER.total_closed.load(Ordering::Relaxed) as i64;
    let diff = opened - closed;
    
    if diff != 0 {
        Some(diff)
    } else {
        None
    }
}

/// Log a summary of file handle statistics
pub fn log_stats_summary() {
    let stats = FileHandleTracker::stats();
    log::info!(
        "[FILE_HANDLE] SUMMARY: active={}, total_opened={}, total_closed={}, peak={}",
        stats.active_handles,
        stats.total_opened,
        stats.total_closed,
        stats.peak_handles
    );
    
    if let Some(leak_count) = check_for_leaks() {
        warn!(
            "[FILE_HANDLE] POTENTIAL LEAK: {} unclosed handles detected",
            leak_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_handle_tracking() {
        FileHandleTracker::reset();

        // Open 3 files
        record_open("test", "/path/a");
        record_open("test", "/path/b");
        record_open("test", "/path/c");

        let stats = FileHandleTracker::stats();
        assert_eq!(stats.active_handles, 3);
        assert_eq!(stats.total_opened, 3);
        assert_eq!(stats.total_closed, 0);
        assert_eq!(stats.peak_handles, 3);

        // Close 2
        record_close("test", "/path/a");
        record_close("test", "/path/b");

        let stats = FileHandleTracker::stats();
        assert_eq!(stats.active_handles, 1);
        assert_eq!(stats.total_opened, 3);
        assert_eq!(stats.total_closed, 2);
        assert_eq!(stats.peak_handles, 3); // Peak unchanged

        // Check for leaks
        assert_eq!(check_for_leaks(), Some(1));

        // Close last one
        record_close("test", "/path/c");
        assert_eq!(check_for_leaks(), None);
    }

    #[test]
    fn test_peak_tracking() {
        FileHandleTracker::reset();

        // Open 5, close 3, open 2 more
        for i in 0..5 {
            record_open("test", &format!("/path/{}", i));
        }
        
        let stats = FileHandleTracker::stats();
        assert_eq!(stats.peak_handles, 5);

        for i in 0..3 {
            record_close("test", &format!("/path/{}", i));
        }

        // Open 2 more (total active = 4)
        record_open("test", "/path/5");
        record_open("test", "/path/6");

        let stats = FileHandleTracker::stats();
        assert_eq!(stats.active_handles, 4);
        assert_eq!(stats.peak_handles, 5); // Still 5 from earlier
    }
}
