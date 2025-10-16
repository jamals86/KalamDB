//! Services module for business logic
//!
//! This module contains service layers that orchestrate operations across
//! multiple components like storage, catalog, and configuration.

pub mod namespace_service;

pub use namespace_service::NamespaceService;
