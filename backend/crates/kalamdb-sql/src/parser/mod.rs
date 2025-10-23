//! SQL parser module organization.
//!
//! This module provides a clean separation between standard SQL parsing
//! (using sqlparser-rs) and KalamDB-specific extensions.
//!
//! ## Architecture
//!
//! - **standard.rs**: Wraps sqlparser-rs for ANSI SQL, PostgreSQL, and MySQL syntax
//! - **extensions.rs**: Custom parsers for KalamDB-specific commands (CREATE STORAGE, FLUSH, etc.)
//! - **system.rs**: Parsers for system table queries
//! - **utils.rs**: Common parsing utilities (keyword extraction, normalization, etc.)
//!
//! ## Design Principles
//!
//! 1. **Use sqlparser-rs for standard SQL**: SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, etc.
//! 2. **Custom parsers only for KalamDB extensions**: Commands that don't fit SQL standards
//! 3. **PostgreSQL/MySQL compatibility**: Accept common syntax variants
//! 4. **Type-safe AST**: Strongly typed statement representations

pub mod extensions;
pub mod standard;
pub mod system;
pub mod utils;

pub use extensions::*;
pub use standard::*;
pub use system::*;
