#[derive(Debug, Clone, Default)]
pub struct ConsumerOffsets {
    pub position: u64,
    pub last_committed: Option<u64>,
    pub highest_processed: Option<u64>,
}
