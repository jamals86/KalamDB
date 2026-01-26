//! User models for system.users.

mod user;

pub use user::{User, DEFAULT_LOCKOUT_DURATION_MINUTES, DEFAULT_MAX_FAILED_ATTEMPTS};

// Re-export from kalamdb-commons for convenience
pub use kalamdb_commons::models::{AuthType, Role, UserName};
