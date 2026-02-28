//! SQL query execution with HTTP transport.

mod executor;

pub use executor::{QueryExecutor, UploadProgressCallback};
