pub mod runtime;

// Re-export version constants for convenient access
pub use runtime::{BUILD_DATE, GIT_BRANCH, GIT_COMMIT_HASH, SERVER_VERSION};
