/// Configuration for live-query row materialization.
#[derive(Debug, Clone, Default)]
pub struct LiveRowsConfig {
    /// Maximum number of rows to retain in the materialized result set.
    ///
    /// When set, the newest rows are kept and older rows are discarded.
    pub limit: Option<usize>,
}
