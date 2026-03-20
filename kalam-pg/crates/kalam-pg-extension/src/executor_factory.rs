use kalam_pg_api::KalamBackendExecutor;
use kalam_pg_common::KalamPgError;
use std::sync::Arc;

/// Build the active backend executor for the PostgreSQL extension.
pub trait ExecutorFactory {
    fn build(&self) -> Result<Arc<dyn KalamBackendExecutor>, KalamPgError>;
}
