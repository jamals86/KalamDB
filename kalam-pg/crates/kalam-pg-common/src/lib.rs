//! Shared low-level types for the PostgreSQL extension workspace.

pub mod config;
pub mod constants;
pub mod error;
pub mod mode;

pub use config::EmbeddedRuntimeConfig;
pub use constants::{DELETED_COLUMN, SEQ_COLUMN, USER_ID_COLUMN};
pub use error::KalamPgError;
pub use mode::BackendMode;
