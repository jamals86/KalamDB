//! WebSocket authentication handler
//!
//! Handles the Authenticate message for WebSocket connections.
//! Uses the unified authentication module from kalamdb-auth.
//!
//! Supports multiple authentication methods:
//! - Direct credentials (username/password in Authenticate message)
//! - Authorization header (Basic or Bearer token sent in message)

use actix_ws::Session;
use kalamdb_auth::{
    authenticate, extract_username_for_audit, AuthRequest, UserRepository,
};
use kalamdb_commons::models::ConnectionInfo;
use kalamdb_commons::models::UserId;
use kalamdb_commons::WebSocketMessage;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::live::{ConnectionsManager, SharedConnectionState};
use kalamdb_core::sql::executor::helpers::audit;
use log::{error, info};
use std::sync::Arc;

use super::{send_auth_error, send_json};

/// Handle authentication message with username/password
///
/// Uses connection_id from SharedConnectionState, no separate parameter needed.
/// Delegates to the unified authentication module in kalamdb-auth.
pub async fn handle_authenticate(
    connection_state: &SharedConnectionState,
    client_ip: &ConnectionInfo,
    username: &str,
    password: &str,
    session: &mut Session,
    registry: &Arc<ConnectionsManager>,
    app_context: &Arc<AppContext>,
    user_repo: &Arc<dyn UserRepository>,
) -> Result<(), String> {
    let auth_request = AuthRequest::Credentials {
        username: username.to_string(),
        password: password.to_string(),
    };

    authenticate_with_request(
        connection_state,
        client_ip,
        auth_request,
        session,
        registry,
        app_context,
        user_repo,
    )
    .await
}

/// Handle authentication message with Authorization header (Basic or Bearer)
///
/// Allows WebSocket clients to authenticate using the same Authorization header
/// format as HTTP requests, enabling JWT and Basic Auth over WebSocket.
pub async fn handle_authenticate_header(
    connection_state: &SharedConnectionState,
    client_ip: &ConnectionInfo,
    auth_header: &str,
    session: &mut Session,
    registry: &Arc<ConnectionsManager>,
    app_context: &Arc<AppContext>,
    user_repo: &Arc<dyn UserRepository>,
) -> Result<(), String> {
    let auth_request = AuthRequest::Header(auth_header.to_string());

    authenticate_with_request(
        connection_state,
        client_ip,
        auth_request,
        session,
        registry,
        app_context,
        user_repo,
    )
    .await
}

/// Internal function that handles authentication for any AuthRequest type
async fn authenticate_with_request(
    connection_state: &SharedConnectionState,
    connection_info: &ConnectionInfo,
    auth_request: AuthRequest,
    session: &mut Session,
    registry: &Arc<ConnectionsManager>,
    app_context: &Arc<AppContext>,
    user_repo: &Arc<dyn UserRepository>,
) -> Result<(), String> {
    let connection_id = connection_state.read().connection_id().clone();

    // Get username for logging (before authentication attempt)
    let username_for_log = extract_username_for_audit(&auth_request);

    info!(
        "Authenticating WebSocket: connection_id={}, username={}",
        connection_id, username_for_log
    );

    // Authenticate using unified auth module
    let auth_result = match authenticate(auth_request, &connection_info, user_repo).await {
        Ok(result) => {
            // Log successful authentication
            let event_type = if connection_info.is_localhost() {
                "LOGIN_WS_LOCALHOST"
            } else {
                "LOGIN_WS"
            };
            let entry = audit::log_auth_event(&result.user.user_id, event_type, true, None);
            if let Err(e) = audit::persist_audit_entry(app_context, &entry).await {
                error!("Failed to persist audit log: {}", e);
            }
            result.user
        }
        Err(e) => {
            // Log failed authentication
            let entry = audit::log_auth_event(
                &UserId::new(username_for_log),
                "LOGIN_WS",
                false,
                Some(format!("{}", e)),
            );
            if let Err(e) = audit::persist_audit_entry(app_context, &entry).await {
                error!("Failed to persist audit log: {}", e);
            }

            let _ = send_auth_error(session.clone(), "Invalid username or password").await;
            return Err("Authentication failed".to_string());
        }
    };

    // Mark authenticated in connection state
    connection_state
        .write()
        .mark_authenticated(auth_result.user_id.clone());
    // Update registry's user index
    registry.on_authenticated(&connection_id, auth_result.user_id.clone());

    // Send success
    let msg = WebSocketMessage::AuthSuccess {
        user_id: auth_result.user_id.as_str().to_string(),
        role: format!("{:?}", auth_result.role),
    };
    let _ = send_json(session, &msg).await;

    info!(
        "WebSocket authenticated: {} as {} ({:?})",
        connection_id,
        auth_result.user_id.as_str(),
        auth_result.role
    );

    Ok(())
}
