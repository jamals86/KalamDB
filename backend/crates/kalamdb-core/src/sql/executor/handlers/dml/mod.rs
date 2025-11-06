//! DML module: INSERT / DELETE / UPDATE handlers
//!
//! This module provides granular handlers for DML operations and re-exports
//! their public types for convenient import from `handlers::dml`.

pub mod insert;
pub mod delete;
pub mod update;
pub mod mod_helpers;

pub use insert::InsertHandler;
pub use delete::DeleteHandler;
pub use update::UpdateHandler;
