use crate::error::{FilestoreError, Result};
use std::future::Future;

/// Run an async operation in a synchronous context.
///
/// Uses the current Tokio runtime when available, otherwise creates a
/// lightweight current-thread runtime.
pub fn run_blocking<F, Fut, T>(make_future: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = Result<T>> + Send,
    T: Send,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::scope(|s| {
            s.spawn(|| handle.block_on(make_future()))
                .join()
                .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
        })
    } else {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

        rt.block_on(make_future())
    }
}