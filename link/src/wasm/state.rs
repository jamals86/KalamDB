use crate::models::SubscriptionOptions;
use crate::seq_id::SeqId;

/// Stored subscription info for reconnection
#[derive(Clone)]
pub(crate) struct SubscriptionState {
    /// The SQL query for this subscription
    pub(crate) sql: String,
    /// Original subscription options
    pub(crate) options: SubscriptionOptions,
    /// JavaScript callback function
    pub(crate) callback: js_sys::Function,
    /// Last received seq_id for resumption
    pub(crate) last_seq_id: Option<SeqId>,
}
