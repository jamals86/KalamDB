use crate::app_context::AppContext;
use crate::jobs::executors::JobRegistry;
use kalamdb_commons::NodeId;
use kalamdb_system::{JobNodesTableProvider, JobsTableProvider};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

/// Unified Job Manager
///
/// Provides centralized job creation, execution, tracking, and lifecycle management.
pub struct JobsManager {
    /// System table provider for job persistence
    pub(crate) jobs_provider: Arc<JobsTableProvider>,
    /// System table provider for per-node job state
    pub(crate) job_nodes_provider: Arc<JobNodesTableProvider>,

    /// Registry of job executors (trait-based dispatch)
    pub(crate) job_registry: Arc<JobRegistry>,

    /// Node ID for this instance
    pub(crate) node_id: NodeId,

    /// Flag for graceful shutdown (AtomicBool for lock-free access in hot loop)
    pub(crate) shutdown: AtomicBool,
    /// AppContext for global services - uses Weak to avoid Arc cycle
    /// (AppContext holds Arc<JobsManager>, so we use Weak here)
    pub(crate) app_context: Weak<AppContext>,
}

impl JobsManager {
    /// Create a new JobsManager
    ///
    /// # Arguments
    /// * `jobs_provider` - System table provider for job persistence
    /// * `job_registry` - Registry of job executors
    /// * `app_ctx` - AppContext for accessing shared services
    pub fn new(
        jobs_provider: Arc<JobsTableProvider>,
        job_nodes_provider: Arc<JobNodesTableProvider>,
        job_registry: Arc<JobRegistry>,
        app_ctx: Arc<AppContext>,
    ) -> Self {
        let node_id = app_ctx.node_id().as_ref().clone();
        Self {
            jobs_provider,
            job_nodes_provider,
            job_registry,
            node_id,
            shutdown: AtomicBool::new(false),
            app_context: Arc::downgrade(&app_ctx),
        }
    }

    /// Get attached AppContext (panics if AppContext was dropped)
    pub(crate) fn get_attached_app_context(&self) -> Arc<AppContext> {
        self.app_context
            .upgrade()
            .expect("AppContext was dropped - JobsManager outlived AppContext")
    }

    /// Request graceful shutdown
    pub fn shutdown(&self) {
        log::debug!("Initiating job manager shutdown");
        self.shutdown.store(true, Ordering::Release);
    }
}
