//! Library entry point for kalam-cli components.
//!
//! Exposes reusable modules (formatter, session, config, etc.) so integration
//! tests and other crates can leverage CLI formatting and behaviors without
//! going through the binary entry point.

pub mod completer;
pub mod config;
pub mod error;
pub mod formatter;
pub mod highlighter;
pub mod history;
pub mod parser;
pub mod session;

pub use config::CLIConfiguration;
pub use error::{CLIError, Result};
pub use formatter::OutputFormatter;
pub use session::{CLISession, OutputFormat};
