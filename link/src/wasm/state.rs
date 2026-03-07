use crate::models::ServerMessage;
use crate::models::SubscriptionOptions;
use crate::seq_id::SeqId;
use crate::subscription::{LiveRowsConfig, LiveRowsMaterializer};

#[derive(Clone)]
pub(crate) enum SubscriptionCallbackMode {
    RawEvents,
    LiveRows { materializer: LiveRowsMaterializer },
}

impl SubscriptionCallbackMode {
    pub(crate) fn raw() -> Self {
        Self::RawEvents
    }

    pub(crate) fn live_rows(config: LiveRowsConfig) -> Self {
        Self::LiveRows {
            materializer: LiveRowsMaterializer::new(config),
        }
    }
}

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
    /// Promise resolver for an in-flight subscribe request waiting for ack.
    pub(crate) pending_subscribe_resolve: Option<js_sys::Function>,
    /// Promise rejector for an in-flight subscribe request waiting for ack.
    pub(crate) pending_subscribe_reject: Option<js_sys::Function>,
    /// True until the server responds with subscription_ack or error.
    pub(crate) awaiting_initial_response: bool,
    /// Callback behavior for this subscription.
    pub(crate) callback_mode: SubscriptionCallbackMode,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum WasmLiveRowsEvent {
    Rows {
        subscription_id: String,
        rows: Vec<crate::models::RowData>,
    },
    Error {
        subscription_id: String,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct WasmLiveRowsOptions {
    pub(crate) limit: Option<usize>,
    pub(crate) subscription_options: Option<SubscriptionOptions>,
}

pub(crate) fn live_rows_payload(
    mode: &mut SubscriptionCallbackMode,
    raw_message: &str,
    event: &ServerMessage,
) -> Option<String> {
    match mode {
        SubscriptionCallbackMode::RawEvents => Some(raw_message.to_string()),
        SubscriptionCallbackMode::LiveRows { materializer } => {
            let change_event = server_message_to_change_event(event)?;
            let update = materializer.apply(change_event)?;
            let wasm_event = match update {
                crate::subscription::LiveRowsEvent::Rows {
                    subscription_id,
                    rows,
                } => WasmLiveRowsEvent::Rows {
                    subscription_id,
                    rows,
                },
                crate::subscription::LiveRowsEvent::Error {
                    subscription_id,
                    code,
                    message,
                } => WasmLiveRowsEvent::Error {
                    subscription_id,
                    code,
                    message,
                },
            };
            serde_json::to_string(&wasm_event).ok()
        },
    }
}

fn server_message_to_change_event(event: &ServerMessage) -> Option<crate::models::ChangeEvent> {
    match event {
        ServerMessage::SubscriptionAck {
            subscription_id,
            total_rows,
            batch_control,
            schema,
        } => Some(crate::models::ChangeEvent::Ack {
            subscription_id: subscription_id.clone(),
            total_rows: *total_rows,
            batch_control: batch_control.clone(),
            schema: schema.clone(),
        }),
        ServerMessage::InitialDataBatch {
            subscription_id,
            rows,
            batch_control,
        } => Some(crate::models::ChangeEvent::InitialDataBatch {
            subscription_id: subscription_id.clone(),
            rows: rows.clone(),
            batch_control: batch_control.clone(),
        }),
        ServerMessage::Change {
            subscription_id,
            change_type,
            rows,
            old_values,
        } => Some(match change_type {
            crate::models::ChangeTypeRaw::Insert => crate::models::ChangeEvent::Insert {
                subscription_id: subscription_id.clone(),
                rows: rows.clone().unwrap_or_default(),
            },
            crate::models::ChangeTypeRaw::Update => crate::models::ChangeEvent::Update {
                subscription_id: subscription_id.clone(),
                rows: rows.clone().unwrap_or_default(),
                old_rows: old_values.clone().unwrap_or_default(),
            },
            crate::models::ChangeTypeRaw::Delete => crate::models::ChangeEvent::Delete {
                subscription_id: subscription_id.clone(),
                old_rows: old_values.clone().unwrap_or_default(),
            },
        }),
        ServerMessage::Error {
            subscription_id,
            code,
            message,
        } => Some(crate::models::ChangeEvent::Error {
            subscription_id: subscription_id.clone(),
            code: code.clone(),
            message: message.clone(),
        }),
        _ => None,
    }
}
