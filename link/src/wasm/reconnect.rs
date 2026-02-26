use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::prelude::{Closure, JsValue};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

use crate::models::{ClientMessage, ConnectionOptions, ServerMessage, SubscriptionRequest};

use super::auth::WasmAuthProvider;
use super::console_log;
use super::helpers::create_promise;
use super::state::SubscriptionState;

/// Internal reconnection logic with auth provider support
pub(crate) async fn reconnect_internal_with_auth(
    url: String,
    auth: WasmAuthProvider,
    ws_ref: Rc<RefCell<Option<WebSocket>>>,
) -> Result<(), JsValue> {
    if matches!(auth, WasmAuthProvider::Basic { .. }) {
        return Err(JsValue::from_str(
            "WebSocket authentication requires a JWT token. Use KalamClientWithJwt or login first.",
        ));
    }

    let ws_url = super::helpers::ws_url_from_http(url)?;

    let ws = WebSocket::new(&ws_url)?;

    // Set binaryType to arraybuffer so binary messages come as ArrayBuffer, not Blob
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let (connect_promise, connect_resolve, connect_reject) = create_promise();
    let (auth_promise, auth_resolve, auth_reject) = create_promise();

    // Check if auth is required
    let requires_auth = !matches!(auth, WasmAuthProvider::None);
    let auth_message = auth.to_ws_auth_message();
    let ws_clone = ws.clone();
    let auth_resolve_for_anon = auth_resolve.clone();

    let connect_resolve_clone = connect_resolve.clone();
    let onopen = Closure::wrap(Box::new(move || {
        if let Some(auth_msg) = &auth_message {
            if let Ok(json) = serde_json::to_string(&auth_msg) {
                let _ = ws_clone.send_with_str(&json);
            }
        } else {
            // No auth needed (anonymous), resolve auth immediately
            let _ = auth_resolve_for_anon.call0(&JsValue::NULL);
        }
        let _ = connect_resolve_clone.call0(&JsValue::NULL);
    }) as Box<dyn FnMut()>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    let connect_reject_clone = connect_reject.clone();
    let auth_reject_clone = auth_reject.clone();
    let onerror = Closure::wrap(Box::new(move |_: ErrorEvent| {
        let error = JsValue::from_str("Reconnection failed");
        let _ = connect_reject_clone.call1(&JsValue::NULL, &error);
        let _ = auth_reject_clone.call1(&JsValue::NULL, &error);
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    let auth_resolve_clone = auth_resolve.clone();
    let auth_reject_clone2 = auth_reject.clone();
    let auth_handled = Rc::new(RefCell::new(!requires_auth));
    let auth_handled_clone = auth_handled.clone();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            let message = String::from(txt);
            if let Ok(event) = serde_json::from_str::<ServerMessage>(&message) {
                if !*auth_handled_clone.borrow() {
                    match event {
                        ServerMessage::AuthSuccess { .. } => {
                            *auth_handled_clone.borrow_mut() = true;
                            let _ = auth_resolve_clone.call0(&JsValue::NULL);
                        },
                        ServerMessage::AuthError { message } => {
                            *auth_handled_clone.borrow_mut() = true;
                            let error = JsValue::from_str(&format!("Auth failed: {}", message));
                            let _ = auth_reject_clone2.call1(&JsValue::NULL, &error);
                        },
                        _ => {},
                    }
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    *ws_ref.borrow_mut() = Some(ws);

    JsFuture::from(connect_promise).await?;
    JsFuture::from(auth_promise).await?;

    Ok(())
}

/// Restart the keepalive ping interval after a successful reconnection.
///
/// This mirrors `KalamClient::start_ping_timer` but operates on the shared
/// Rc fields passed through the reconnect closure chain.
pub(crate) fn restart_ping_timer(
    ws_ref: &Rc<RefCell<Option<WebSocket>>>,
    connection_options: &Rc<RefCell<ConnectionOptions>>,
    ping_interval_id: &Rc<RefCell<i32>>,
) {
    // Stop any previous timer
    let old_id = *ping_interval_id.borrow();
    if old_id >= 0 {
        super::helpers::global_clear_interval(old_id);
        *ping_interval_id.borrow_mut() = -1;
    }

    let interval_ms = connection_options.borrow().ping_interval_ms;
    if interval_ms == 0 {
        return;
    }

    let ws_clone = Rc::clone(ws_ref);
    let ping_cb = Closure::wrap(Box::new(move || {
        if let Some(ws) = ws_clone.borrow().as_ref() {
            if ws.ready_state() == WebSocket::OPEN {
                if let Ok(payload) = serde_json::to_string(&ClientMessage::Ping) {
                    let _ = ws.send_with_str(&payload);
                }
            }
        }
    }) as Box<dyn FnMut()>);

    let id = super::helpers::global_set_interval(
        ping_cb.as_ref().unchecked_ref(),
        interval_ms as i32,
    );
    ping_cb.forget();
    *ping_interval_id.borrow_mut() = id;
}

/// Re-subscribe to all subscriptions after reconnection with last seq_id
pub(crate) async fn resubscribe_all(
    ws_ref: Rc<RefCell<Option<WebSocket>>>,
    subscription_state: Rc<RefCell<HashMap<String, SubscriptionState>>>,
) {
    let states: Vec<(String, SubscriptionState)> = subscription_state
        .borrow()
        .iter()
        .map(|(id, state)| (id.clone(), state.clone()))
        .collect();

    for (subscription_id, state) in states {
        console_log(&format!(
            "KalamClient: Re-subscribing to {} with last_seq_id: {:?}",
            subscription_id,
            state.last_seq_id.map(|s| s.to_string())
        ));

        // Create options with from_seq_id if we have a last seq_id
        let mut options = state.options.clone();
        if let Some(seq_id) = state.last_seq_id {
            options.from_seq_id = Some(seq_id);
        }

        let subscribe_msg = ClientMessage::Subscribe {
            subscription: SubscriptionRequest {
                id: subscription_id.clone(),
                sql: state.sql.clone(),
                options,
            },
        };

        if let Some(ws) = ws_ref.borrow().as_ref() {
            if let Ok(payload) = serde_json::to_string(&subscribe_msg) {
                if let Err(e) = ws.send_with_str(&payload) {
                    console_log(&format!(
                        "KalamClient: Failed to re-subscribe to {}: {:?}",
                        subscription_id, e
                    ));
                }
            }
        }
    }
}
