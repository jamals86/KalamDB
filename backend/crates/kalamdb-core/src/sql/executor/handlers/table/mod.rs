//! Table handlers module

pub mod create;
pub mod alter;
pub mod drop;
pub mod show;
pub mod describe;
pub mod show_stats;

pub use create::CreateTableHandler;
pub use alter::AlterTableHandler;
pub use drop::DropTableHandler;
pub use show::ShowTablesHandler;
pub use describe::DescribeTableHandler;
pub use show_stats::ShowStatsHandler;
