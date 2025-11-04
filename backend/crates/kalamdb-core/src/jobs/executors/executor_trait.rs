//! Job Executor Trait
//!
//! Unified trait for all job executors with standard lifecycle methods.
//! All job types (Flush, Cleanup, Retention, etc.) implement this trait.

use crate::error::KalamDbError;
use crate::app_context::AppContext;
use kalamdb_commons::models::system::Job;
use kalamdb_commons::models::JobType;
use std::sync::Arc;
use log::{debug, info, warn, error};

// Note: CancellationToken is not available in tokio::sync in older versions
// We'll use a simple atomic bool for now
use std::sync::atomic::{AtomicBool, Ordering};

/// Simple cancellation token wrapper
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Decision made by a job executor after execution
#[derive(Debug, Clone)]
pub enum JobDecision {
    /// Job completed successfully
    Completed {
        /// Success message
        message: Option<String>,
    },
    /// Job should be retried after a delay
    Retry {
        /// Error message
        message: String,
        /// Optional stack trace
        exception_trace: Option<String>,
        /// Backoff delay in milliseconds
        backoff_ms: u64,
    },
    /// Job failed permanently (no more retries)
    Failed {
        /// Error message
        message: String,
        /// Optional stack trace
        exception_trace: Option<String>,
    },
}

/// Context passed to job executors
#[derive(Clone)]
pub struct JobContext {
    /// Application context for accessing stores, registries, etc.
    pub app_ctx: Arc<AppContext>,
    /// Cancellation token for graceful shutdown
    pub cancellation_token: CancellationToken,
    /// Job ID for logging (automatically prefixed to log messages)
    pub job_id: String,
}

impl JobContext {
    /// Create a new job context
    pub fn new(app_ctx: Arc<AppContext>, job_id: String) -> Self {
        Self {
            app_ctx,
            cancellation_token: CancellationToken::new(),
            job_id,
        }
    }

    /// Create a new job context with cancellation token
    pub fn with_cancellation(app_ctx: Arc<AppContext>, job_id: String, token: CancellationToken) -> Self {
        Self {
            app_ctx,
            cancellation_token: token,
            job_id,
        }
    }

    /// Log debug message with [JobId] prefix
    pub fn log_debug(&self, message: &str) {
        debug!("[{}] {}", self.job_id, message);
    }

    /// Log info message with [JobId] prefix
    pub fn log_info(&self, message: &str) {
        info!("[{}] {}", self.job_id, message);
    }

    /// Log warning message with [JobId] prefix
    pub fn log_warn(&self, message: &str) {
        warn!("[{}] {}", self.job_id, message);
    }

    /// Log error message with [JobId] prefix
    pub fn log_error(&self, message: &str) {
        error!("[{}] {}", self.job_id, message);
    }

    /// Get current timestamp in milliseconds
    pub fn now_millis() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Get current timestamp in seconds
    pub fn now_secs() -> i64 {
        chrono::Utc::now().timestamp()
    }
}

/// Trait for job executors
///
/// All job types must implement this trait to be registered in the JobRegistry.
/// 
/// # Example
///
/// ```no_run
/// use kalamdb_core::jobs::executors::{JobExecutor, JobDecision, JobContext};
/// use kalamdb_core::error::KalamDbError;
/// use kalamdb_commons::models::system::{Job, JobType};
/// use async_trait::async_trait;
///
/// pub struct FlushJobExecutor;
///
/// #[async_trait]
/// impl JobExecutor for FlushJobExecutor {
///     fn job_type(&self) -> JobType {
///         JobType::Flush
///     }
///
///     fn name(&self) -> &'static str {
///         "FlushJobExecutor"
///     }
///
///     async fn validate_params(&self, job: &Job) -> Result<(), KalamDbError> {
///         // Validate job parameters
///         Ok(())
///     }
///
///     async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError> {
///         ctx.log_info("Starting flush job");
///         // Execute flush logic
///         Ok(JobDecision::Completed {
///             message: Some("Flushed 1000 rows".to_string()),
///         })
///     }
///
///     async fn cancel(&self, ctx: &JobContext, job: &Job) -> Result<(), KalamDbError> {
///         ctx.log_info("Cancelling flush job");
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait JobExecutor: Send + Sync {
    /// Returns the job type this executor handles
    fn job_type(&self) -> JobType;

    /// Returns the executor name for logging
    fn name(&self) -> &'static str;

    /// Validates job parameters before execution
    /// 
    /// Returns an error if parameters are invalid, preventing execution.
    async fn validate_params(&self, job: &Job) -> Result<(), KalamDbError>;

    /// Executes the job
    ///
    /// Returns a JobDecision indicating whether the job completed, should retry, or failed.
    async fn execute(&self, ctx: &JobContext, job: &Job) -> Result<JobDecision, KalamDbError>;

    /// Cancels a running job
    ///
    /// Called when a job cancellation is requested. The executor should
    /// perform any necessary cleanup and stop execution gracefully.
    async fn cancel(&self, ctx: &JobContext, job: &Job) -> Result<(), KalamDbError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_decision_completed() {
        let decision = JobDecision::Completed {
            message: Some("Success".to_string()),
        };
        match decision {
            JobDecision::Completed { message } => {
                assert_eq!(message, Some("Success".to_string()));
            }
            _ => panic!("Expected Completed decision"),
        }
    }

    #[test]
    fn test_job_decision_retry() {
        let decision = JobDecision::Retry {
            message: "Temporary failure".to_string(),
            exception_trace: Some("Stack trace".to_string()),
            backoff_ms: 1000,
        };
        match decision {
            JobDecision::Retry { message, exception_trace, backoff_ms } => {
                assert_eq!(message, "Temporary failure");
                assert_eq!(exception_trace, Some("Stack trace".to_string()));
                assert_eq!(backoff_ms, 1000);
            }
            _ => panic!("Expected Retry decision"),
        }
    }

    #[test]
    fn test_job_decision_failed() {
        let decision = JobDecision::Failed {
            message: "Permanent failure".to_string(),
            exception_trace: None,
        };
        match decision {
            JobDecision::Failed { message, exception_trace } => {
                assert_eq!(message, "Permanent failure");
                assert_eq!(exception_trace, None);
            }
            _ => panic!("Expected Failed decision"),
        }
    }

    #[test]
    fn test_job_context_logging() {
        let app_ctx = Arc::new(AppContext::default());
        let ctx = JobContext::new(app_ctx, "FL-abc123".to_string());
        
        // These should not panic
        ctx.log_debug("Debug message");
        ctx.log_info("Info message");
        ctx.log_warn("Warning message");
        ctx.log_error("Error message");
    }

    #[test]
    fn test_job_context_timestamp() {
        let now_millis = JobContext::now_millis();
        let now_secs = JobContext::now_secs();
        
        assert!(now_millis > 0);
        assert!(now_secs > 0);
        assert!(now_millis > now_secs * 1000);
    }
}
