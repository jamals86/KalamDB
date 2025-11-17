//! DML module: INSERT / DELETE / UPDATE handlers
//!
//! This module provides granular handlers for DML operations and re-exports
//! their public types for convenient import from `handlers::dml`.

pub mod delete;
pub mod insert;
pub mod mod_helpers;
pub mod update;

pub use delete::DeleteHandler;
pub use insert::InsertHandler;
pub use update::UpdateHandler;
