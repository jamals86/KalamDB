//! Namespace handlers module

pub mod alter;
pub mod create;
pub mod drop;
pub mod show;

pub use alter::AlterNamespaceHandler;
pub use create::CreateNamespaceHandler;
pub use drop::DropNamespaceHandler;
pub use show::ShowNamespacesHandler;
