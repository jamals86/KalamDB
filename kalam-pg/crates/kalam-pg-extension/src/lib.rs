//! PostgreSQL extension entrypoint and backend wiring.

#[cfg(all(feature = "remote", feature = "embedded"))]
compile_error!("Enable only one backend mode: 'remote' or 'embedded'.");

#[cfg(not(any(feature = "remote", feature = "embedded")))]
compile_error!("Enable one backend mode: 'remote' or 'embedded'.");

mod executor_factory;

#[cfg(feature = "embedded")]
mod embedded_executor_factory;

pub use executor_factory::ExecutorFactory;

#[cfg(feature = "embedded")]
pub use embedded_executor_factory::EmbeddedExecutorFactory;
