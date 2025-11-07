//! Job Registry
//!
//! Central registry for all job executors. Maps JobType to executor implementations.

use super::executor_trait::JobExecutor;
use crate::error::KalamDbError;
use kalamdb_commons::models::JobType;
use dashmap::DashMap;
use std::sync::Arc;

/// Registry of job executors
///
/// Thread-safe registry that maps job types to their executor implementations.
/// Uses DashMap for lock-free concurrent access.
///
/// # Example
///
/// ```no_run
/// use kalamdb_core::jobs::executors::{JobRegistry, JobExecutor};
/// use std::sync::Arc;
///
/// let registry = JobRegistry::new();
/// 
/// // Register executors
/// // registry.register(Arc::new(FlushJobExecutor));
/// // registry.register(Arc::new(CleanupJobExecutor));
///
/// // Look up executor
/// // let executor = registry.get(&JobType::Flush).unwrap();
/// ```
pub struct JobRegistry {
    executors: DashMap<JobType, Arc<dyn JobExecutor>>,
}

impl JobRegistry {
    /// Create a new empty job registry
    pub fn new() -> Self {
        Self {
            executors: DashMap::new(),
        }
    }

    /// Register a job executor
    ///
    /// # Arguments
    /// * `executor` - Job executor implementation
    ///
    /// # Panics
    /// Panics if an executor for this job type is already registered
    pub fn register(&self, executor: Arc<dyn JobExecutor>) {
        let job_type = executor.job_type();
        if self.executors.contains_key(&job_type) {
            panic!(
                "Executor for job type {:?} is already registered",
                job_type
            );
        }
        self.executors.insert(job_type, executor);
    }

    /// Register a job executor, replacing any existing one
    ///
    /// # Arguments
    /// * `executor` - Job executor implementation
    ///
    /// Returns the previous executor if one was registered
    pub fn register_or_replace(&self, executor: Arc<dyn JobExecutor>) -> Option<Arc<dyn JobExecutor>> {
        let job_type = executor.job_type();
        self.executors.insert(job_type, executor)
    }

    /// Get a job executor by type
    ///
    /// # Arguments
    /// * `job_type` - Type of job to look up
    ///
    /// # Returns
    /// * `Some(executor)` - If an executor is registered for this type
    /// * `None` - If no executor is registered
    pub fn get(&self, job_type: &JobType) -> Option<Arc<dyn JobExecutor>> {
        self.executors.get(job_type).map(|e| e.clone())
    }

    /// Get a job executor by type, returning an error if not found
    ///
    /// # Arguments
    /// * `job_type` - Type of job to look up
    ///
    /// # Errors
    /// Returns `KalamDbError::NotFound` if no executor is registered for this type
    pub fn get_or_error(&self, job_type: &JobType) -> Result<Arc<dyn JobExecutor>, KalamDbError> {
        self.get(job_type).ok_or_else(|| {
            KalamDbError::NotFound(format!("No executor registered for job type: {:?}", job_type))
        })
    }

    /// Check if an executor is registered for a job type
    pub fn contains(&self, job_type: &JobType) -> bool {
        self.executors.contains_key(job_type)
    }

    /// Get the number of registered executors
    pub fn len(&self) -> usize {
        self.executors.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.executors.is_empty()
    }

    /// List all registered job types
    pub fn job_types(&self) -> Vec<JobType> {
        self.executors.iter().map(|entry| *entry.key()).collect()
    }

    /// Clear all registered executors
    pub fn clear(&self) {
        self.executors.clear();
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::system::Job;
    use kalamdb_commons::models::JobType;
    use crate::jobs::executors::{JobContext, JobDecision};
    use async_trait::async_trait;

    struct MockExecutor {
        job_type: JobType,
    }

    #[async_trait]
    impl JobExecutor for MockExecutor {
        fn job_type(&self) -> JobType {
            self.job_type
        }

        fn name(&self) -> &'static str {
            "MockExecutor"
        }

        async fn validate_params(&self, _job: &Job) -> Result<(), KalamDbError> {
            Ok(())
        }

        async fn execute(&self, _ctx: &JobContext, _job: &Job) -> Result<JobDecision, KalamDbError> {
            Ok(JobDecision::Completed { message: None })
        }

        async fn cancel(&self, _ctx: &JobContext, _job: &Job) -> Result<(), KalamDbError> {
            Ok(())
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let registry = JobRegistry::new();
        let executor = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });

        registry.register(executor.clone());

        let retrieved = registry.get(&JobType::Flush).unwrap();
        assert_eq!(retrieved.job_type(), JobType::Flush);
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn test_registry_duplicate_registration() {
        let registry = JobRegistry::new();
        let executor1 = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });
        let executor2 = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });

        registry.register(executor1);
        registry.register(executor2); // Should panic
    }

    #[test]
    fn test_registry_replace() {
        let registry = JobRegistry::new();
        let executor1 = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });
        let executor2 = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });

        let old = registry.register_or_replace(executor1);
        assert!(old.is_none());

        let old = registry.register_or_replace(executor2);
        assert!(old.is_some());
    }

    #[test]
    fn test_registry_get_or_error() {
        let registry = JobRegistry::new();

        let result = registry.get_or_error(&JobType::Flush);
        assert!(result.is_err());

        let executor = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });
        registry.register(executor);

        let result = registry.get_or_error(&JobType::Flush);
        assert!(result.is_ok());
    }

    #[test]
    fn test_registry_contains() {
        let registry = JobRegistry::new();
        assert!(!registry.contains(&JobType::Flush));

        let executor = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });
        registry.register(executor);

        assert!(registry.contains(&JobType::Flush));
        assert!(!registry.contains(&JobType::Cleanup));
    }

    #[test]
    fn test_registry_len() {
        let registry = JobRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());

        let executor = Arc::new(MockExecutor {
            job_type: JobType::Flush,
        });
        registry.register(executor);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_registry_job_types() {
        let registry = JobRegistry::new();

        let types = registry.job_types();
        assert_eq!(types.len(), 0);

        registry.register(Arc::new(MockExecutor {
            job_type: JobType::Flush,
        }));
        registry.register(Arc::new(MockExecutor {
            job_type: JobType::Cleanup,
        }));

        let types = registry.job_types();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&JobType::Flush));
        assert!(types.contains(&JobType::Cleanup));
    }

    #[test]
    fn test_registry_clear() {
        let registry = JobRegistry::new();

        registry.register(Arc::new(MockExecutor {
            job_type: JobType::Flush,
        }));
        assert_eq!(registry.len(), 1);

        registry.clear();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }
}
