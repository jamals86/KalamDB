//! Observability Tests
//!
//! Tests covering:
//! - Production observability features
//! - Monitoring and metrics

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// Observability Tests
mod test_production_observability_http;
