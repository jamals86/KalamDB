use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CloseEvent, ErrorEvent, Headers, MessageEvent, Request, RequestInit, RequestMode, Response,
    WebSocket,
};

use base64::Engine;
use crate::compression;
use crate::models::{
    ClientMessage, ConnectionOptions, QueryRequest, ServerMessage, SubscriptionOptions,
    SubscriptionRequest,
};

use super::auth::WasmAuthProvider;
use super::console_log;
use super::helpers::{create_promise, md5_hash};
use super::reconnect::{reconnect_internal_with_auth, resubscribe_all};
use super::state::SubscriptionState;
use super::validation::{
    quote_table_name, validate_column_name, validate_row_id, validate_sql_identifier,
};

/// WASM-compatible KalamDB client with auto-reconnection support
///
/// Supports multiple authentication methods:
/// - Basic Auth: `new KalamClient(url, username, password)`
/// - JWT Token: `KalamClient.withJwt(url, token)`
/// - Anonymous: `KalamClient.anonymous(url)`
///
/// # Example (JavaScript)
/// ```js
/// import init, { KalamClient, KalamClientWithJwt, KalamClientAnonymous } from './pkg/kalam_link.js';
///
/// await init();
///
/// // Basic Auth (username/password)
/// const client = new KalamClient(
///   "http://localhost:8080",
///   "username",
///   "password"
/// );
///
/// // JWT Token Auth
/// const jwtClient = KalamClientWithJwt.new(
///   "http://localhost:8080",
///   "eyJhbGciOiJIUzI1NiIs..."
/// );
///
/// // Anonymous (localhost bypass)
/// const anonClient = KalamClientAnonymous.new("http://localhost:8080");
///
/// // Configure auto-reconnect (enabled by default)
/// client.setAutoReconnect(true);
/// client.setReconnectDelay(1000, 30000);
///
/// await client.connect();
///
/// // Subscribe with options
/// const subId = await client.subscribeWithSql(
///   "SELECT * FROM chat.messages",
///   JSON.stringify({
///     batch_size: 100,
///     include_old_values: true
///   }),
///   (event) => console.log('Change:', event)
/// );
/// ```
#[wasm_bindgen]
pub struct KalamClient {
    url: String,
    /// Authentication provider (Basic, JWT, or None)
    auth: WasmAuthProvider,
    ws: Rc<RefCell<Option<WebSocket>>>,
    /// Subscription state including callbacks and last seq_id for resumption
    subscription_state: Rc<RefCell<HashMap<String, SubscriptionState>>>,
    /// Connection options for auto-reconnect
    connection_options: Rc<RefCell<ConnectionOptions>>,
    /// Current reconnection attempt count
    reconnect_attempts: Rc<RefCell<u32>>,
    /// Flag indicating if we're currently reconnecting
    is_reconnecting: Rc<RefCell<bool>>,
}

impl KalamClient {
    fn new_with_auth(url: String, auth: WasmAuthProvider) -> KalamClient {
        KalamClient {
            url,
            auth,
            ws: Rc::new(RefCell::new(None)),
            subscription_state: Rc::new(RefCell::new(HashMap::new())),
            connection_options: Rc::new(RefCell::new(ConnectionOptions::default())),
            reconnect_attempts: Rc::new(RefCell::new(0)),
            is_reconnecting: Rc::new(RefCell::new(false)),
        }
    }
}

#[wasm_bindgen]
impl KalamClient {
    /// Create a new KalamDB client with HTTP Basic Authentication (T042, T043, T044)
    ///
    /// # Arguments
    /// * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
    /// * `username` - Username for authentication (required)
    /// * `password` - Password for authentication (required)
    ///
    /// # Errors
    /// Returns JsValue error if url, username, or password is empty
    #[wasm_bindgen(constructor)]
    pub fn new(url: String, username: String, password: String) -> Result<KalamClient, JsValue> {
        // T044: Validate required parameters with clear error messages
        if url.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient: 'url' parameter is required and cannot be empty",
            ));
        }
        if username.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient: 'username' parameter is required and cannot be empty",
            ));
        }
        if password.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient: 'password' parameter is required and cannot be empty",
            ));
        }

        Ok(KalamClient::new_with_auth(
            url,
            WasmAuthProvider::Basic { username, password },
        ))
    }

    /// Create a new KalamDB client with JWT Token Authentication
    ///
    /// # Arguments
    /// * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
    /// * `token` - JWT token for authentication (required)
    ///
    /// # Errors
    /// Returns JsValue error if url or token is empty
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const client = KalamClient.withJwt(
    ///   "http://localhost:8080",
    ///   "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
    /// );
    /// await client.connect();
    /// ```
    #[wasm_bindgen(js_name = withJwt)]
    pub fn with_jwt(url: String, token: String) -> Result<KalamClient, JsValue> {
        if url.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient.withJwt: 'url' parameter is required and cannot be empty",
            ));
        }
        if token.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient.withJwt: 'token' parameter is required and cannot be empty",
            ));
        }

        Ok(KalamClient::new_with_auth(url, WasmAuthProvider::Jwt { token }))
    }

    /// Create a new KalamDB client with no authentication
    ///
    /// Useful for localhost connections where the server allows
    /// unauthenticated access, or for development/testing scenarios.
    ///
    /// # Arguments
    /// * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
    ///
    /// # Errors
    /// Returns JsValue error if url is empty
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const client = KalamClient.anonymous("http://localhost:8080");
    /// await client.connect();
    /// ```
    #[wasm_bindgen(js_name = anonymous)]
    pub fn anonymous(url: String) -> Result<KalamClient, JsValue> {
        if url.is_empty() {
            return Err(JsValue::from_str(
                "KalamClient.anonymous: 'url' parameter is required and cannot be empty",
            ));
        }

        Ok(KalamClient::new_with_auth(url, WasmAuthProvider::None))
    }

    /// Get the current authentication type
    ///
    /// Returns one of: "basic", "jwt", or "none"
    #[wasm_bindgen(js_name = getAuthType)]
    pub fn get_auth_type(&self) -> String {
        match &self.auth {
            WasmAuthProvider::Basic { .. } => "basic".to_string(),
            WasmAuthProvider::Jwt { .. } => "jwt".to_string(),
            WasmAuthProvider::None => "none".to_string(),
        }
    }

    /// Enable or disable automatic reconnection
    ///
    /// # Arguments
    /// * `enabled` - Whether to automatically reconnect on connection loss
    #[wasm_bindgen(js_name = setAutoReconnect)]
    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.connection_options.borrow_mut().auto_reconnect = enabled;
    }

    /// Set reconnection delay parameters
    ///
    /// # Arguments
    /// * `initial_delay_ms` - Initial delay in milliseconds between reconnection attempts
    /// * `max_delay_ms` - Maximum delay (for exponential backoff)
    #[wasm_bindgen(js_name = setReconnectDelay)]
    pub fn set_reconnect_delay(&self, initial_delay_ms: u64, max_delay_ms: u64) {
        let mut opts = self.connection_options.borrow_mut();
        opts.reconnect_delay_ms = initial_delay_ms;
        opts.max_reconnect_delay_ms = max_delay_ms;
    }

    /// Set maximum reconnection attempts
    ///
    /// # Arguments
    /// * `max_attempts` - Maximum number of attempts (0 = infinite)
    #[wasm_bindgen(js_name = setMaxReconnectAttempts)]
    pub fn set_max_reconnect_attempts(&self, max_attempts: u32) {
        self.connection_options.borrow_mut().max_reconnect_attempts = if max_attempts == 0 {
            None
        } else {
            Some(max_attempts)
        };
    }

    /// Get the current reconnection attempt count
    #[wasm_bindgen(js_name = getReconnectAttempts)]
    pub fn get_reconnect_attempts(&self) -> u32 {
        *self.reconnect_attempts.borrow()
    }

    /// Check if currently reconnecting
    #[wasm_bindgen(js_name = isReconnecting)]
    pub fn is_reconnecting_flag(&self) -> bool {
        *self.is_reconnecting.borrow()
    }

    /// Get the last received seq_id for a subscription
    ///
    /// Useful for debugging or manual resumption tracking
    #[wasm_bindgen(js_name = getLastSeqId)]
    pub fn get_last_seq_id(&self, subscription_id: String) -> Option<String> {
        self.subscription_state
            .borrow()
            .get(&subscription_id)
            .and_then(|state| state.last_seq_id.map(|seq| seq.to_string()))
    }

    /// Connect to KalamDB server via WebSocket (T045, T063C-T063D)
    ///
    /// # Returns
    /// Promise that resolves when connection is established and authenticated
    pub async fn connect(&mut self) -> Result<(), JsValue> {
        if matches!(self.auth, WasmAuthProvider::Basic { .. }) {
            return Err(JsValue::from_str(
                "WebSocket authentication requires a JWT token. Use KalamClientWithJwt or login first.",
            ));
        }

        // Check if already connected - prevent duplicate connections
        if self.is_connected() {
            console_log("KalamClient: Already connected, skipping reconnection");
            return Ok(());
        }

        // T063O: Add console.log debugging for connection state changes
        console_log("KalamClient: Connecting to WebSocket...");

        // Convert http(s) URL to ws(s) URL (no auth in URL)
        let ws_url = self.url.replace("http://", "ws://").replace("https://", "wss://");
        let ws_url = format!("{}/v1/ws", ws_url);

        // T063C: Implement proper WebSocket connection using web-sys::WebSocket
        let ws = WebSocket::new(&ws_url)?;

        // Set binaryType to arraybuffer so binary messages come as ArrayBuffer, not Blob
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Create promises for connection and authentication
        let (connect_promise, connect_resolve, connect_reject) = create_promise();

        // For anonymous auth, we don't need to wait for auth_promise
        let requires_auth = !matches!(self.auth, WasmAuthProvider::None);

        let (auth_promise, auth_resolve, auth_reject) = create_promise();

        // Clone auth message for the onopen handler
        let auth_message = self.auth.to_ws_auth_message();
        let ws_clone_for_auth = ws.clone();
        let auth_resolve_for_anon = auth_resolve.clone();

        // Set up onopen handler to send authentication message
        let connect_resolve_clone = connect_resolve.clone();
        let onopen_callback = Closure::wrap(Box::new(move || {
            console_log("KalamClient: WebSocket connected, sending authentication...");

            // Send authentication message if we have one
            if let Some(auth_msg) = &auth_message {
                if let Ok(json) = serde_json::to_string(&auth_msg) {
                    if let Err(e) = ws_clone_for_auth.send_with_str(&json) {
                        console_log(&format!("KalamClient: Failed to send auth message: {:?}", e));
                    }
                }
            } else {
                // No auth needed (anonymous), resolve auth immediately
                console_log("KalamClient: Anonymous connection, skipping authentication");
                let _ = auth_resolve_for_anon.call0(&JsValue::NULL);
            }

            let _ = connect_resolve_clone.call0(&JsValue::NULL);
        }) as Box<dyn FnMut()>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        // T063L: Implement WebSocket error and close handlers
        let connect_reject_clone = connect_reject.clone();
        let auth_reject_clone = auth_reject.clone();
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            console_log(&format!("KalamClient: WebSocket error: {:?}", e));
            let error_msg = JsValue::from_str("WebSocket connection failed");
            let _ = connect_reject_clone.call1(&JsValue::NULL, &error_msg);
            let _ = auth_reject_clone.call1(&JsValue::NULL, &error_msg);
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let onclose_callback = Closure::wrap(Box::new(move |e: CloseEvent| {
            console_log(&format!(
                "KalamClient: WebSocket closed: code={}, reason={}",
                e.code(),
                e.reason()
            ));
            // Note: Auto-reconnection is handled via the setup_auto_reconnect callback
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        // Set up auto-reconnect onclose handler
        self.setup_auto_reconnect(&ws);

        // T063K: Implement WebSocket onmessage handler to parse events and invoke registered callbacks
        let subscriptions = Rc::clone(&self.subscription_state);
        let auth_resolve_clone = auth_resolve.clone();
        let auth_reject_clone2 = auth_reject.clone();
        let auth_handled = Rc::new(RefCell::new(!requires_auth)); // Already handled if anonymous
        let auth_handled_clone = Rc::clone(&auth_handled);

        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            // Handle Text, ArrayBuffer, and Blob messages
            let message = if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                // Plain text message
                String::from(txt)
            } else if let Ok(array_buffer) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                // Binary message (ArrayBuffer) - likely gzip compressed
                let uint8_array = js_sys::Uint8Array::new(&array_buffer);
                let data = uint8_array.to_vec();

                // Decompress gzip data
                match compression::decompress_gzip(&data) {
                    Ok(decompressed) => match String::from_utf8(decompressed) {
                        Ok(s) => s,
                        Err(e) => {
                            console_log(&format!(
                                "KalamClient: Invalid UTF-8 in decompressed message: {}",
                                e
                            ));
                            return;
                        },
                    },
                    Err(e) => {
                        console_log(&format!("KalamClient: Failed to decompress message: {}", e));
                        return;
                    },
                }
            } else if e.data().is_instance_of::<web_sys::Blob>() {
                // Blob message - browser may send binary as Blob instead of ArrayBuffer
                // For now, log and skip - we need async handling for Blob
                console_log("KalamClient: Received Blob message - binary mode may be misconfigured. Attempting to read as text.");
                // Try to get it as a string anyway via toString()
                if let Some(s) = e.data().as_string() {
                    s
                } else {
                    console_log("KalamClient: Could not convert Blob to string");
                    return;
                }
            } else {
                // Unknown message type - log extensive debugging info
                let data = e.data();
                let type_name = js_sys::Reflect::get(&data, &"constructor".into())
                    .ok()
                    .and_then(|c| js_sys::Reflect::get(&c, &"name".into()).ok())
                    .and_then(|n| n.as_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // Try to get typeof
                let typeof_str = data.js_typeof().as_string().unwrap_or_else(|| "?".to_string());

                // Try to stringify for debugging
                let data_preview = js_sys::JSON::stringify(&data)
                    .ok()
                    .and_then(|s| s.as_string())
                    .unwrap_or_else(|| format!("{:?}", data));

                console_log(&format!(
                    "KalamClient: Received unknown message type: constructor={}, typeof={}, preview={}",
                    type_name, typeof_str, &data_preview[..data_preview.len().min(200)]
                ));
                return;
            };

            console_log(&format!("KalamClient: Received WebSocket message: {}", message));

            // Parse message using ServerMessage enum
            if let Ok(event) = serde_json::from_str::<ServerMessage>(&message) {
                // Check for authentication response first
                if !*auth_handled_clone.borrow() {
                    match &event {
                        ServerMessage::AuthSuccess { user_id, role } => {
                            console_log(&format!(
                                "KalamClient: Authentication successful - user_id: {}, role: {}",
                                user_id, role
                            ));
                            *auth_handled_clone.borrow_mut() = true;
                            let _ = auth_resolve_clone.call0(&JsValue::NULL);
                            return;
                        },
                        ServerMessage::AuthError { message: error_msg } => {
                            console_log(&format!(
                                "KalamClient: Authentication failed - {}",
                                error_msg
                            ));
                            *auth_handled_clone.borrow_mut() = true;
                            let error =
                                JsValue::from_str(&format!("Authentication failed: {}", error_msg));
                            let _ = auth_reject_clone2.call1(&JsValue::NULL, &error);
                            return;
                        },
                        _ => {}, // Not an auth message, continue to subscription handling
                    }
                }

                // Look for subscription_id in the event and update last_seq_id
                let subscription_id = match &event {
                    ServerMessage::SubscriptionAck {
                        subscription_id,
                        total_rows,
                        ..
                    } => {
                        console_log(&format!(
                            "KalamClient: Parsed SubscriptionAck - id: {}, total_rows: {}",
                            subscription_id, total_rows
                        ));
                        Some(subscription_id.clone())
                    },
                    ServerMessage::InitialDataBatch {
                        subscription_id,
                        batch_control,
                        rows,
                    } => {
                        console_log(&format!(
                            "KalamClient: Parsed InitialDataBatch - id: {}, rows: {}, status: {:?}",
                            subscription_id, rows.len(), batch_control.status
                        ));
                        // Update last_seq_id from batch_control
                        if let Some(seq_id) = &batch_control.last_seq_id {
                            let mut subs = subscriptions.borrow_mut();
                            if let Some(state) = subs.get_mut(subscription_id) {
                                state.last_seq_id = Some(*seq_id);
                                console_log(&format!(
                                    "KalamClient: Updated last_seq_id for {} to {}",
                                    subscription_id, seq_id
                                ));
                            }
                        }
                        Some(subscription_id.clone())
                    },
                    ServerMessage::Change {
                        subscription_id,
                        change_type,
                        rows,
                        ..
                    } => {
                        console_log(&format!(
                            "KalamClient: Parsed Change - id: {}, type: {:?}, rows: {:?}",
                            subscription_id,
                            change_type,
                            rows.as_ref().map(|r| r.len())
                        ));
                        Some(subscription_id.clone())
                    },
                    ServerMessage::Error {
                        subscription_id,
                        code,
                        message,
                        ..
                    } => {
                        console_log(&format!(
                            "KalamClient: Parsed Error - id: {}, code: {}, msg: {}",
                            subscription_id, code, message
                        ));
                        Some(subscription_id.clone())
                    },
                    _ => None, // Auth messages don't have subscription_id
                };

                if let Some(id) = subscription_id.clone() {
                    let subs = subscriptions.borrow();
                    console_log(&format!(
                        "KalamClient: Looking for callback for subscription_id: {} (registered subs: {:?})",
                        id,
                        subs.keys().collect::<Vec<_>>()
                    ));
                    // First try exact match
                    if let Some(state) = subs.get(&id) {
                        console_log(&format!(
                            "KalamClient: Found exact match for {}, calling callback",
                            id
                        ));
                        let _ = state.callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
                    } else {
                        // Server may prefix subscription_id with user_id-session_id-
                        // Try to find a callback where the server's ID ends with our client ID
                        let mut found = false;
                        for (client_id, state) in subs.iter() {
                            if id.ends_with(client_id) {
                                console_log(&format!(
                                    "KalamClient: Found suffix match {} -> {}, calling callback",
                                    id, client_id
                                ));
                                let _ = state
                                    .callback
                                    .call1(&JsValue::NULL, &JsValue::from_str(&message));
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            console_log(&format!(
                                "KalamClient: No callback found for subscription_id: {}",
                                id
                            ));
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        // T063D: Store WebSocket instance in KalamClient struct
        *self.ws.borrow_mut() = Some(ws);

        console_log("KalamClient: Waiting for WebSocket to open...");

        // Wait for the WebSocket to open
        JsFuture::from(connect_promise).await?;

        console_log("KalamClient: Waiting for authentication...");

        // Wait for authentication to complete
        JsFuture::from(auth_promise).await?;

        console_log("KalamClient: WebSocket connection established and authenticated");
        Ok(())
    }

    /// Disconnect from KalamDB server (T046, T063E)
    pub async fn disconnect(&mut self) -> Result<(), JsValue> {
        console_log("KalamClient: Disconnecting from WebSocket...");

        // Disable auto-reconnect during intentional disconnect
        self.connection_options.borrow_mut().auto_reconnect = false;

        // T063E: Properly close WebSocket and cleanup resources
        if let Some(ws) = self.ws.borrow_mut().take() {
            ws.close()?;
        }

        // Clear all subscriptions
        self.subscription_state.borrow_mut().clear();

        console_log("KalamClient: Disconnected");
        Ok(())
    }

    /// Check if client is currently connected (T047)
    ///
    /// # Returns
    /// true if WebSocket connection is active, false otherwise
    #[wasm_bindgen(js_name = isConnected)]
    pub fn is_connected(&self) -> bool {
        self.ws
            .borrow()
            .as_ref()
            .is_some_and(|ws| ws.ready_state() == WebSocket::OPEN)
    }

    /// Insert data into a table (T048, T063G)
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to insert into
    /// * `data` - JSON string representing the row data
    ///
    /// # Example (JavaScript)
    /// ```js
    /// await client.insert("todos", JSON.stringify({
    ///   title: "Buy groceries",
    ///   completed: false
    /// }));
    /// ```
    pub async fn insert(&self, table_name: String, data: String) -> Result<String, JsValue> {
        // Security: Validate table name to prevent SQL injection
        validate_sql_identifier(&table_name, "Table name")?;

        // Parse JSON data to build proper SQL INSERT statement
        let parsed: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| JsValue::from_str(&format!("Invalid JSON data: {}", e)))?;

        let obj = parsed
            .as_object()
            .ok_or_else(|| JsValue::from_str("Data must be a JSON object"))?;

        if obj.is_empty() {
            return Err(JsValue::from_str("Cannot insert empty object"));
        }

        // Security: Validate all column names
        for key in obj.keys() {
            validate_column_name(key)?;
        }

        // Build column names and values
        // Security: Quote identifiers with double quotes (SQL standard)
        let columns: Vec<String> = obj.keys().map(|k| format!("\"{}\"", k)).collect();
        let values: Vec<String> = obj
            .values()
            .map(|v| match v {
                serde_json::Value::Null => "NULL".to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                _ => format!("'{}'", v.to_string().replace('\'', "''")),
            })
            .collect();

        // Security: Quote table name with double quotes, handling namespace.table format
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote_table_name(&table_name),
            columns.join(", "),
            values.join(", ")
        );

        self.execute_sql_internal(&sql, None).await
    }

    /// Delete a row from a table (T049, T063H)
    ///
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `row_id` - ID of the row to delete
    pub async fn delete(&self, table_name: String, row_id: String) -> Result<(), JsValue> {
        // Security: Validate inputs to prevent SQL injection
        validate_sql_identifier(&table_name, "Table name")?;
        validate_row_id(&row_id)?;

        // T063H: Implement using fetch API to execute DELETE statement via /v1/api/sql
        // Security: Quote table name (handling namespace.table format) and use parameterized-style value
        let sql = format!(
            "DELETE FROM {} WHERE id = '{}'",
            quote_table_name(&table_name),
            row_id.replace('\'', "''")
        );
        self.execute_sql_internal(&sql, None).await?;
        Ok(())
    }

    /// Execute a SQL query (T050, T063F)
    ///
    /// # Arguments
    /// * `sql` - SQL query string
    ///
    /// # Returns
    /// JSON string with query results
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const result = await client.query("SELECT * FROM todos WHERE completed = false");
    /// const data = JSON.parse(result);
    /// ```
    pub async fn query(&self, sql: String) -> Result<String, JsValue> {
        // T063F: Implement query() using web-sys fetch API
        self.execute_sql_internal(&sql, None).await
    }

    /// Execute a SQL query with parameters
    ///
    /// # Arguments
    /// * `sql` - SQL query string with placeholders ($1, $2, ...)
    /// * `params` - JSON array string of parameter values
    ///
    /// # Returns
    /// JSON string with query results
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const result = await client.queryWithParams(
    ///   "SELECT * FROM users WHERE id = $1 AND age > $2",
    ///   JSON.stringify([42, 18])
    /// );
    /// const data = JSON.parse(result);
    /// ```
    #[wasm_bindgen(js_name = queryWithParams)]
    pub async fn query_with_params(
        &self,
        sql: String,
        params: Option<String>,
    ) -> Result<String, JsValue> {
        let parsed_params: Option<Vec<serde_json::Value>> = match params {
            Some(p) if !p.is_empty() => Some(
                serde_json::from_str(&p)
                    .map_err(|e| JsValue::from_str(&format!("Invalid params JSON: {}", e)))?,
            ),
            _ => None,
        };
        self.execute_sql_internal(&sql, parsed_params).await
    }

    /// Subscribe to table changes (T051, T063I-T063J)
    ///
    /// # Arguments
    /// * `table_name` - Name of the table to subscribe to
    /// * `callback` - JavaScript function to call when changes occur
    ///
    /// # Returns
    /// Subscription ID for later unsubscribe
    pub async fn subscribe(
        &self,
        table_name: String,
        callback: js_sys::Function,
    ) -> Result<String, JsValue> {
        // Security: Validate table name to prevent SQL injection
        validate_sql_identifier(&table_name, "Table name")?;

        // Default: SELECT * FROM table with default options
        // Security: Quote table name properly (handles namespace.table format)
        let sql = format!("SELECT * FROM {}", quote_table_name(&table_name));
        self.subscribe_with_sql(sql, None, callback).await
    }

    /// Subscribe to a SQL query with optional subscription options
    ///
    /// # Arguments
    /// * `sql` - SQL SELECT query to subscribe to
    /// * `options` - Optional JSON string with subscription options:
    ///   - `batch_size`: Number of rows per batch (default: server-configured)
    ///   - `auto_reconnect`: Override client auto-reconnect for this subscription (default: true)
    ///   - `include_old_values`: Include old values in UPDATE/DELETE events (default: false)
    ///   - `resume_from_seq_id`: Resume from a specific sequence ID (internal use)
    /// * `callback` - JavaScript function to call when changes occur
    ///
    /// # Returns
    /// Subscription ID for later unsubscribe
    ///
    /// # Example (JavaScript)
    /// ```js
    /// // Subscribe with options
    /// const subId = await client.subscribeWithSql(
    ///   "SELECT * FROM chat.messages WHERE conversation_id = 1",
    ///   JSON.stringify({ batch_size: 50, include_old_values: true }),
    ///   (event) => console.log('Change:', event)
    /// );
    /// ```
    #[wasm_bindgen(js_name = subscribeWithSql)]
    pub async fn subscribe_with_sql(
        &self,
        sql: String,
        options: Option<String>,
        callback: js_sys::Function,
    ) -> Result<String, JsValue> {
        if !self.is_connected() {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // Parse options if provided
        let subscription_options: SubscriptionOptions = if let Some(opts_json) = options {
            serde_json::from_str(&opts_json)
                .map_err(|e| JsValue::from_str(&format!("Invalid options JSON: {}", e)))?
        } else {
            SubscriptionOptions::default()
        };

        // Generate unique subscription ID from SQL hash
        let subscription_id = format!("sub-{:x}", md5_hash(&sql));

        // Store subscription state for reconnection (includes callback and last_seq_id)
        self.subscription_state.borrow_mut().insert(
            subscription_id.clone(),
            SubscriptionState {
                sql: sql.clone(),
                options: subscription_options.clone(),
                callback,
                last_seq_id: None,
            },
        );

        // Send subscribe message via WebSocket
        if let Some(ws) = self.ws.borrow().as_ref() {
            let subscribe_msg = ClientMessage::Subscribe {
                subscription: SubscriptionRequest {
                    id: subscription_id.clone(),
                    sql: sql.clone(),
                    options: subscription_options,
                },
            };
            let payload = serde_json::to_string(&subscribe_msg)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
            console_log(&format!(
                "KalamClient: Sending subscribe request - id: {}, sql: {}",
                subscription_id, sql
            ));
            ws.send_with_str(&payload)?;
        }

        console_log(&format!("KalamClient: Subscribed with ID: {}", subscription_id));
        Ok(subscription_id)
    }

    /// Unsubscribe from table changes (T052, T063M)
    ///
    /// # Arguments
    /// * `subscription_id` - ID returned from subscribe()
    pub async fn unsubscribe(&self, subscription_id: String) -> Result<(), JsValue> {
        if !self.is_connected() {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // Remove from subscription state
        self.subscription_state.borrow_mut().remove(&subscription_id);

        // Send unsubscribe message via WebSocket
        if let Some(ws) = self.ws.borrow().as_ref() {
            let unsubscribe_msg = ClientMessage::Unsubscribe {
                subscription_id: subscription_id.clone(),
            };
            let payload = serde_json::to_string(&unsubscribe_msg)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
            ws.send_with_str(&payload)?;
        }

        console_log(&format!("KalamClient: Unsubscribed from: {}", subscription_id));
        Ok(())
    }

    /// Login with current Basic Auth credentials and switch to JWT authentication
    ///
    /// Sends a POST request to `/v1/api/auth/login` with the stored username/password
    /// and updates the client to use JWT authentication on success.
    ///
    /// # Returns
    /// The full LoginResponse as a JsValue (includes access_token, refresh_token, user info, etc.)
    ///
    /// # Errors
    /// - If the client doesn't use Basic Auth
    /// - If login request fails
    /// - If the response doesn't contain an access_token
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const client = new KalamClient("http://localhost:8080", "user", "pass");
    /// const response = await client.login();
    /// console.log(response.access_token, response.refresh_token);
    /// await client.connect(); // Now uses JWT for WebSocket
    /// ```
    pub async fn login(&mut self) -> Result<JsValue, JsValue> {
        let (username, password) = match &self.auth {
            WasmAuthProvider::Basic { username, password } => (username.clone(), password.clone()),
            _ => {
                return Err(JsValue::from_str(
                    "login() requires Basic Auth credentials. Create client with new KalamClient(url, username, password)",
                ))
            },
        };

        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;
        opts.set_headers(&headers);

        let body = serde_json::json!({
            "username": username,
            "password": password,
        });
        opts.set_body(&JsValue::from_str(&body.to_string()));

        let url = format!("{}/v1/api/auth/login", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        let resp_value = JsFuture::from(super::helpers::fetch_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        if !resp.ok() {
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text
                .as_string()
                .unwrap_or_else(|| format!("Login failed: HTTP {}", resp.status()));
            return Err(JsValue::from_str(&error_msg));
        }

        let json = JsFuture::from(resp.text()?).await?;
        let json_str = json.as_string().unwrap_or_default();

        let login_response: crate::models::LoginResponse = serde_json::from_str(&json_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse login response: {}", e)))?;

        // Switch to JWT auth
        self.auth = WasmAuthProvider::Jwt {
            token: login_response.access_token.clone(),
        };

        console_log("KalamClient: Login successful, switched to JWT authentication");

        // Return full LoginResponse as JsValue
        serde_wasm_bindgen::to_value(&login_response)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize login response: {}", e)))
    }

    /// Refresh the access token using a refresh token
    ///
    /// Sends a POST request to `/v1/api/auth/refresh` with the refresh token
    /// in the Authorization Bearer header, and updates the client to use the new JWT.
    ///
    /// # Arguments
    /// * `refresh_token` - The refresh token obtained from a previous login
    ///
    /// # Returns
    /// The full LoginResponse as a JsValue (includes new access_token, refresh_token, etc.)
    ///
    /// # Errors
    /// - If the refresh request fails
    /// - If the response doesn't contain a valid token
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const client = new KalamClient("http://localhost:8080", "user", "pass");
    /// const loginResp = await client.login();
    /// // Later, when access_token expires:
    /// const refreshResp = await client.refresh_access_token(loginResp.refresh_token);
    /// console.log(refreshResp.access_token);
    /// ```
    pub async fn refresh_access_token(&mut self, refresh_token: &str) -> Result<JsValue, JsValue> {
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        let headers = Headers::new()?;
        headers.set("Authorization", &format!("Bearer {}", refresh_token))?;
        opts.set_headers(&headers);

        let url = format!("{}/v1/api/auth/refresh", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        let resp_value = JsFuture::from(super::helpers::fetch_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        if !resp.ok() {
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text
                .as_string()
                .unwrap_or_else(|| format!("Token refresh failed: HTTP {}", resp.status()));
            return Err(JsValue::from_str(&error_msg));
        }

        let json = JsFuture::from(resp.text()?).await?;
        let json_str = json.as_string().unwrap_or_default();

        let login_response: crate::models::LoginResponse = serde_json::from_str(&json_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse refresh response: {}", e)))?;

        // Update to new JWT
        self.auth = WasmAuthProvider::Jwt {
            token: login_response.access_token.clone(),
        };

        console_log("KalamClient: Token refreshed, updated JWT authentication");

        // Return full LoginResponse as JsValue
        serde_wasm_bindgen::to_value(&login_response)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize refresh response: {}", e)))
    }

    /// Consume messages from a topic via HTTP API
    ///
    /// # Arguments
    /// * `options` - Type-safe ConsumeRequest with topic, group_id, batch_size, etc.
    ///
    /// # Returns
    /// A type-safe ConsumeResponse as JsValue (includes messages, next_offset, has_more)
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const result = await client.consume({
    ///   topic: "chat.new_messages",
    ///   group_id: "my-consumer-group",
    ///   batch_size: 10,
    ///   start: "latest",
    /// });
    /// for (const msg of result.messages) {
    ///   console.log(msg.value);
    /// }
    /// ```
    pub async fn consume(&self, options: JsValue) -> Result<JsValue, JsValue> {
        let req: crate::models::ConsumeRequest = serde_wasm_bindgen::from_value(options)
            .map_err(|e| JsValue::from_str(&format!("Invalid consume options: {}", e)))?;

        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;
        if let Some(auth_header) = self.auth.to_http_header() {
            headers.set("Authorization", &auth_header)?;
        }
        opts.set_headers(&headers);

        let mut body = serde_json::json!({
            "topic_id": req.topic,
            "group_id": req.group_id,
            "start": req.start,
            "limit": req.batch_size,
            "partition_id": req.partition_id,
        });
        if let Some(timeout) = req.timeout_seconds {
            body["timeout_seconds"] = serde_json::json!(timeout);
        }
        opts.set_body(&JsValue::from_str(&body.to_string()));

        let url = format!("{}/v1/api/topics/consume", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        let resp_value = JsFuture::from(super::helpers::fetch_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        if !resp.ok() {
            let status = resp.status();
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text
                .as_string()
                .unwrap_or_else(|| format!("Consume failed: HTTP {}", status));
            return Err(JsValue::from_str(&error_msg));
        }

        let json = JsFuture::from(resp.text()?).await?;
        let json_str = json.as_string().unwrap_or_default();

        // Parse the raw server response and convert to type-safe ConsumeResponse
        let raw: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse consume response: {}", e)))?;

        let next_offset = raw["next_offset"].as_u64().unwrap_or(0);
        let has_more = raw["has_more"].as_bool().unwrap_or(false);

        let messages = if let Some(raw_msgs) = raw["messages"].as_array() {
            raw_msgs
                .iter()
                .map(|msg| {
                    // Decode base64 payload if present
                    let value = if let Some(payload_str) = msg["payload"].as_str() {
                        match base64::engine::general_purpose::STANDARD.decode(
                            payload_str,
                        ) {
                            Ok(bytes) => {
                                serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
                            },
                            Err(_) => serde_json::Value::String(payload_str.to_string()),
                        }
                    } else if msg["payload"].is_object() {
                        msg["payload"].clone()
                    } else {
                        serde_json::Value::Null
                    };

                    crate::models::ConsumeMessage {
                        message_id: msg["message_id"].as_str().map(|s| s.to_string())
                            .or_else(|| msg["key"].as_str().map(|s| s.to_string())),
                        source_table: msg["source_table"].as_str().map(|s| s.to_string()),
                        op: msg["op"].as_str().map(|s| s.to_string()),
                        timestamp_ms: msg["timestamp_ms"].as_u64().or_else(|| msg["ts"].as_u64()),
                        offset: msg["offset"].as_u64().unwrap_or(0),
                        partition_id: msg["partition_id"].as_u64().unwrap_or(0) as u32,
                        topic: req.topic.clone(),
                        group_id: req.group_id.clone(),
                        username: msg["username"].as_str().map(|s| crate::models::Username::from(s)),
                        value,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let response = crate::models::ConsumeResponse {
            messages,
            next_offset,
            has_more,
        };

        // Use json_compatible() to produce plain JS objects instead of Maps
        // for nested serde_json::Value fields (ConsumeMessage.value).
        response.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize consume response: {}", e)))
    }

    /// Acknowledge processed messages on a topic
    ///
    /// # Arguments
    /// * `topic` - Topic name
    /// * `group_id` - Consumer group ID
    /// * `partition_id` - Partition ID
    /// * `upto_offset` - Acknowledge all messages up to and including this offset
    ///
    /// # Returns
    /// A type-safe AckResponse as JsValue
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const result = await client.ack("chat.new_messages", "my-group", 0, 42);
    /// console.log(result.success, result.acknowledged_offset);
    /// ```
    pub async fn ack(
        &self,
        topic: String,
        group_id: String,
        partition_id: u32,
        upto_offset: u64,
    ) -> Result<JsValue, JsValue> {
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;
        if let Some(auth_header) = self.auth.to_http_header() {
            headers.set("Authorization", &auth_header)?;
        }
        opts.set_headers(&headers);

        let body = serde_json::json!({
            "topic_id": topic,
            "group_id": group_id,
            "partition_id": partition_id,
            "upto_offset": upto_offset,
        });
        opts.set_body(&JsValue::from_str(&body.to_string()));

        let url = format!("{}/v1/api/topics/ack", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        let resp_value = JsFuture::from(super::helpers::fetch_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        if !resp.ok() {
            let status = resp.status();
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text
                .as_string()
                .unwrap_or_else(|| format!("Ack failed: HTTP {}", status));
            return Err(JsValue::from_str(&error_msg));
        }

        let json = JsFuture::from(resp.text()?).await?;
        let json_str = json.as_string().unwrap_or_default();

        let raw: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse ack response: {}", e)))?;

        let response = crate::models::AckResponse {
            success: raw["success"].as_bool().unwrap_or(true),
            acknowledged_offset: raw["acknowledged_offset"].as_u64().unwrap_or(upto_offset),
        };

        serde_wasm_bindgen::to_value(&response)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize ack response: {}", e)))
    }

    /// Internal: Execute SQL via HTTP POST to /v1/api/sql (T063F)
    async fn execute_sql_internal(
        &self,
        sql: &str,
        params: Option<Vec<serde_json::Value>>,
    ) -> Result<String, JsValue> {
        // T063F: Implement HTTP fetch for SQL queries with authentication
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        // Set headers with authentication
        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;

        // Add Authorization header if we have authentication
        if let Some(auth_header) = self.auth.to_http_header() {
            headers.set("Authorization", &auth_header)?;
        }
        opts.set_headers(&headers);

        // Set body
        let body = QueryRequest {
            sql: sql.to_string(),
            params,
            namespace_id: None,
        };
        let body_str = serde_json::to_string(&body)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
        opts.set_body(&JsValue::from_str(&body_str));

        let url = format!("{}/v1/api/sql", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        // Execute fetch (cross-platform: works in both browser and Node.js)
        let resp_value = JsFuture::from(super::helpers::fetch_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        // Check status
        if !resp.ok() {
            let status = resp.status();
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text.as_string().unwrap_or_else(|| format!("HTTP error {}", status));
            return Err(JsValue::from_str(&error_msg));
        }

        // Parse response
        let json = JsFuture::from(resp.text()?).await?;
        Ok(json.as_string().unwrap_or_else(|| "{}".to_string()))
    }

    /// Set up auto-reconnection handler for the WebSocket
    fn setup_auto_reconnect(&self, ws: &WebSocket) {
        let connection_options = Rc::clone(&self.connection_options);
        let subscription_state = Rc::clone(&self.subscription_state);
        let reconnect_attempts = Rc::clone(&self.reconnect_attempts);
        let is_reconnecting = Rc::clone(&self.is_reconnecting);
        let ws_ref = Rc::clone(&self.ws);
        let url = self.url.clone();
        let auth = self.auth.clone();

        let onclose_reconnect = Closure::wrap(Box::new(move |_e: CloseEvent| {
            let opts = connection_options.borrow();
            if !opts.auto_reconnect || *is_reconnecting.borrow() {
                return;
            }

            let current_attempts = *reconnect_attempts.borrow();
            if let Some(max) = opts.max_reconnect_attempts {
                if current_attempts >= max {
                    console_log(&format!(
                        "KalamClient: Max reconnection attempts ({}) reached",
                        max
                    ));
                    return;
                }
            }

            // Calculate delay with exponential backoff
            let delay = std::cmp::min(
                opts.reconnect_delay_ms * (2u64.pow(current_attempts)),
                opts.max_reconnect_delay_ms,
            );

            console_log(&format!(
                "KalamClient: Scheduling reconnection in {}ms (attempt {})",
                delay,
                current_attempts + 1
            ));

            // Clone for async closure
            let is_reconnecting_clone = is_reconnecting.clone();
            let reconnect_attempts_clone = reconnect_attempts.clone();
            let subscription_state_clone = subscription_state.clone();
            let ws_ref_clone = ws_ref.clone();
            let url_clone = url.clone();
            let auth_clone = auth.clone();

            let reconnect_fn = Closure::wrap(Box::new(move || {
                *is_reconnecting_clone.borrow_mut() = true;
                *reconnect_attempts_clone.borrow_mut() += 1;

                let url = url_clone.clone();
                let auth = auth_clone.clone();
                let ws_ref = ws_ref_clone.clone();
                let subscription_state = subscription_state_clone.clone();
                let is_reconnecting = is_reconnecting_clone.clone();
                let reconnect_attempts = reconnect_attempts_clone.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    match reconnect_internal_with_auth(url, auth, ws_ref.clone()).await {
                        Ok(()) => {
                            console_log("KalamClient: Reconnection successful");
                            *reconnect_attempts.borrow_mut() = 0; // Reset attempts on success
                            resubscribe_all(ws_ref, subscription_state).await;
                        },
                        Err(e) => {
                            console_log(&format!("KalamClient: Reconnection failed: {:?}", e));
                        },
                    }
                    *is_reconnecting.borrow_mut() = false;
                });
            }) as Box<dyn FnMut()>);

            super::helpers::global_set_timeout(
                reconnect_fn.as_ref().unchecked_ref(),
                delay as i32,
            );
            reconnect_fn.forget();
        }) as Box<dyn FnMut(CloseEvent)>);

        // Note: We add a second onclose handler for auto-reconnect
        // The first one just logs, this one handles reconnection
        ws.add_event_listener_with_callback("close", onclose_reconnect.as_ref().unchecked_ref())
            .ok();
        onclose_reconnect.forget();
    }
}
