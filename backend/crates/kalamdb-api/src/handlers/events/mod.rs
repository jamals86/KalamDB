//! WebSocket event handlers
//!
//! This module provides handlers for different WebSocket message types:
//! - Authentication (auth.rs)
//! - Subscription management (subscription.rs, unsubscribe.rs)
//! - Batch fetching (batch.rs)
//! - Connection cleanup (cleanup.rs)
//!
//! All handlers use SharedConnectionState which contains the connection_id,
//! eliminating the need to pass connection_id as a separate parameter.

pub mod auth;
pub mod batch;
pub mod cleanup;
pub mod subscription;
pub mod unsubscribe;

use actix_ws::{CloseCode, CloseReason, Session};
use kalamdb_commons::WebSocketMessage;

use crate::models::Notification;

/// Send auth error and close (takes ownership of session to close it)
pub async fn send_auth_error(mut session: Session, message: &str) -> Result<(), ()> {
    let msg = WebSocketMessage::AuthError {
        message: message.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = session.text(json).await;
    }
    session
        .close(Some(CloseReason {
            code: CloseCode::Policy,
            description: Some("Authentication failed".into()),
        }))
        .await
        .map_err(|_| ())
}

/// Send error notification
pub async fn send_error(session: &mut Session, id: &str, code: &str, message: &str) -> Result<(), ()> {
    let msg = Notification::error(id.to_string(), code.to_string(), message.to_string());
    send_json(session, &msg).await
}

/// Send JSON message
pub async fn send_json<T: serde::Serialize>(session: &mut Session, msg: &T) -> Result<(), ()> {
    if let Ok(json) = serde_json::to_string(msg) {
        session.text(json).await.map_err(|_| ())
    } else {
        Err(())
    }
}
