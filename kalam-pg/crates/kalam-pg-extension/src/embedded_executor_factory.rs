use crate::executor_factory::ExecutorFactory;
use kalam_pg_api::KalamBackendExecutor;
use kalam_pg_common::KalamPgError;
use kalam_pg_embedded::EmbeddedKalamRuntime;
use kalamdb_core::app_context::AppContext;
use std::sync::Arc;

/// Embedded-mode executor factory backed by an in-process Kalam runtime.
pub struct EmbeddedExecutorFactory {
    app_context: Arc<AppContext>,
}

impl EmbeddedExecutorFactory {
    /// Create a factory from an existing shared AppContext.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

impl ExecutorFactory for EmbeddedExecutorFactory {
    fn build(&self) -> Result<Arc<dyn KalamBackendExecutor>, KalamPgError> {
        let runtime = EmbeddedKalamRuntime::from_app_context(Arc::clone(&self.app_context))?;
        Ok(Arc::new(runtime))
    }
}
