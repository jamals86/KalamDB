//! File upload storage service for FILE datatype.
//!
//! Handles file staging, finalization, and cleanup for FILE column uploads.
//! Uses the existing StorageCached infrastructure for file operations.

pub mod file_service;
pub mod staging;

pub use file_service::FileStorageService;
pub use staging::{StagedFile, StagingManager};
