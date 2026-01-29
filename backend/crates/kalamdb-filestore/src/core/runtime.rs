use crate::error::{FilestoreError, Result};
use std::future::Future;

/// Run an async operation in a synchronous context.
///
/// This function handles the tricky case of needing to call async code from sync context.
/// 
/// **Strategy**:
/// - If we're in a tokio multi-thread runtime context, use `block_in_place` which allows
///   blocking while letting other tasks run on other threads
/// - If we're in a tokio current-thread runtime (common in tests), spawn a new thread
///   to avoid deadlock since block_in_place is not allowed
/// - If no runtime exists, create a lightweight current-thread runtime
///
/// **Why this matters for object_store**:
/// Remote backends (S3, GCS, Azure) use the tokio I/O driver for networking.
/// Calling `handle.block_on()` from within the runtime thread will deadlock because
/// the network I/O futures need the runtime thread to poll them.
pub fn run_blocking<F, Fut, T>(make_future: F) -> Result<T>
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = Result<T>> + Send,
    T: Send,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // We're inside a tokio runtime. Check if we can use block_in_place.
            // block_in_place is only allowed in multi-thread runtime, not current_thread.
            match handle.runtime_flavor() {
                tokio::runtime::RuntimeFlavor::MultiThread => {
                    // Safe to use block_in_place - this lets us block while the runtime
                    // continues to service I/O on other threads
                    tokio::task::block_in_place(|| handle.block_on(make_future()))
                }
                tokio::runtime::RuntimeFlavor::CurrentThread => {
                    // Current-thread runtime: block_in_place would panic.
                    // Spawn a new OS thread to run the future without blocking the runtime.
                    std::thread::scope(|s| {
                        s.spawn(|| {
                            // Create a fresh runtime in this thread to avoid deadlock
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .map_err(|e| {
                                    FilestoreError::Other(format!("Failed to create runtime: {e}"))
                                })?;
                            rt.block_on(make_future())
                        })
                        .join()
                        .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
                    })
                }
                _ => {
                    // Unknown runtime flavor - fall back to thread spawn
                    std::thread::scope(|s| {
                        s.spawn(|| {
                            let rt = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .map_err(|e| {
                                    FilestoreError::Other(format!("Failed to create runtime: {e}"))
                                })?;
                            rt.block_on(make_future())
                        })
                        .join()
                        .map_err(|_| FilestoreError::Other("Thread panicked".into()))?
                    })
                }
            }
        }
        Err(_) => {
            // No tokio runtime in current thread - create one
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| FilestoreError::Other(format!("Failed to create runtime: {e}")))?;

            rt.block_on(make_future())
        }
    }
}