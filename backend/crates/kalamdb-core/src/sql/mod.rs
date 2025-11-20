//! SQL module for DataFusion integration and query processing

pub mod datafusion_session;
pub mod executor;
pub mod functions;
pub mod plan_cache;

pub use datafusion_session::DataFusionSessionFactory; // KalamSessionState removed in v3 refactor
pub use executor::handlers::ExecutionResult;
pub use executor::SqlExecutor;
pub use functions::CurrentUserFunction;
