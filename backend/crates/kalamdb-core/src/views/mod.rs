// System schema provider wiring stays in core (depends on SchemaRegistry)
pub mod system_schema_provider;
pub use system_schema_provider::*;

// All view implementations are in the kalamdb-views crate
pub use kalamdb_views::cluster;
pub use kalamdb_views::cluster_groups;
pub use kalamdb_views::columns_view;
pub use kalamdb_views::datatypes;
pub use kalamdb_views::describe;
pub use kalamdb_views::error;
pub use kalamdb_views::server_logs;
pub use kalamdb_views::settings;
pub use kalamdb_views::stats;
pub use kalamdb_views::tables_view;
pub use kalamdb_views::view_base;

// Re-export all public items for backward compatibility
pub use kalamdb_views::cluster::*;
pub use kalamdb_views::cluster_groups::*;
pub use kalamdb_views::columns_view::*;
pub use kalamdb_views::datatypes::*;
pub use kalamdb_views::describe::*;
pub use kalamdb_views::server_logs::*;
pub use kalamdb_views::settings::*;
pub use kalamdb_views::stats::*;
pub use kalamdb_views::tables_view::*;
pub use kalamdb_views::view_base::*;
