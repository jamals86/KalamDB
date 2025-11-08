//! Namespace handlers module

pub mod create;
pub mod alter;
pub mod drop;
pub mod show;

pub use create::CreateNamespaceHandler;
pub use alter::AlterNamespaceHandler;
pub use drop::DropNamespaceHandler;
pub use show::ShowNamespacesHandler;
