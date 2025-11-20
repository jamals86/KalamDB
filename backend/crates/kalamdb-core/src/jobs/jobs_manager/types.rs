use crate::app_context::AppContext;
use crate::jobs::executors::JobRegistry;
use kalamdb_commons::NodeId;
use kalamdb_system::JobsTableProvider;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unified Job Manager
///
/// Provides centralized job creation, execution, tracking, and lifecycle management.
pub struct JobsManager {
    /// System table provider for job persistence
    pub(crate) jobs_provider: Arc<JobsTableProvider>,

    /// Registry of job executors (trait-based dispatch)
    pub(crate) job_registry: Arc<JobRegistry>,

    /// Node ID for this instance
    pub(crate) node_id: NodeId,

    /// Flag for graceful shutdown
    pub(crate) shutdown: Arc<RwLock<bool>>,
    /// AppContext for global services (avoid calling AppContext::get() repeatedly)
    pub(crate) app_context: Arc<RwLock<Option<Arc<AppContext>>>>,
}

impl JobsManager {
    /// Create a new JobsManager
    ///
    /// # Arguments
    /// * `jobs_provider` - System table provider for job persistence
    /// * `job_registry` - Registry of job executors
    pub fn new(jobs_provider: Arc<JobsTableProvider>, job_registry: Arc<JobRegistry>) -> Self {
        Self {
            jobs_provider,
            job_registry,
            node_id: NodeId::new("node_default".to_string()), // TODO: Get from config
            shutdown: Arc::new(RwLock::new(false)),
            app_context: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach an AppContext instance to this JobsManager. This is used to avoid
    /// calling AppContext::get() repeatedly while still supporting initialization
    /// ordering where AppContext is created after JobsManager.
    pub fn set_app_context(&self, app_ctx: Arc<AppContext>) {
        // Use try_write() to avoid blocking in async context
        if let Ok(mut w) = self.app_context.try_write() {
            *w = Some(app_ctx);
        } else {
            // Fallback: spin until we can acquire the lock (should be rare)
            loop {
                if let Ok(mut w) = self.app_context.try_write() {
                    *w = Some(app_ctx);
                    break;
                }
                std::thread::yield_now();
            }
        }
    }

    /// Get attached AppContext (panics if not attached)
    pub(crate) fn get_attached_app_context(&self) -> Arc<AppContext> {
        // Use try_read() to avoid blocking in async context
        if let Ok(r) = self.app_context.try_read() {
            r.clone().expect("AppContext not attached to JobsManager")
        } else {
            // Fallback: spin until we can acquire the lock (should be rare)
            loop {
                if let Ok(r) = self.app_context.try_read() {
                    return r.clone().expect("AppContext not attached to JobsManager");
                }
                std::thread::yield_now();
            }
        }
    }

    /// Request graceful shutdown
    pub async fn shutdown(&self) {
        log::info!("Initiating job manager shutdown");
        *self.shutdown.write().await = true;
    }
}
