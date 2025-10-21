//! SQL functions module
//!
//! This module provides custom SQL functions for DataFusion.

pub mod current_user;

pub use current_user::CurrentUserFunction;
