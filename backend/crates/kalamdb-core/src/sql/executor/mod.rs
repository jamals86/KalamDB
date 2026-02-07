//! SQL executor orchestrator (Phase 9 foundations)
//!
//! Minimal dispatcher that keeps the public `SqlExecutor` API compiling
//! while provider-based handlers are migrated. Behaviour is intentionally
//! limited for now; most statements return a structured error until
//! handler implementations are in place.

pub mod default_ordering;
pub mod handler_adapter;
pub mod handler_registry;
pub mod handlers;
pub mod helpers;
pub mod parameter_binding;
pub mod parameter_validation;
mod sql_executor;

use crate::sql::executor::handler_registry::HandlerRegistry;
use crate::sql::plan_cache::PlanCache;
pub use crate::sql::ExecutionMetadata;
pub use datafusion::scalar::ScalarValue;
use std::sync::Arc;

/// Public facade for SQL execution routing.
pub struct SqlExecutor {
    app_context: Arc<crate::app_context::AppContext>,
    handler_registry: Arc<HandlerRegistry>,
    plan_cache: Arc<PlanCache>,
}

pub type ExecutorMetadataAlias = ExecutionMetadata;
