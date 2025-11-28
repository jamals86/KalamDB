// WASM bindings for KalamDB client (T041-T053, T063C-T063O)
// Provides JavaScript/TypeScript interface for browser and Node.js usage
// Supports automatic reconnection with seq_id resumption

#![cfg(feature = "wasm")]

use crate::models::{
    ClientMessage, ConnectionOptions, QueryRequest, ServerMessage, SubscriptionOptions,
    SubscriptionRequest,
};
use crate::seq_id::SeqId;
use base64::{engine::general_purpose, Engine as _};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    CloseEvent, ErrorEvent, Headers, MessageEvent, Request, RequestInit, RequestMode, Response,
    WebSocket,
};

/// Stored subscription info for reconnection
#[derive(Clone)]
struct SubscriptionState {
    /// The SQL query for this subscription
    sql: String,
    /// Original subscription options
    options: SubscriptionOptions,
    /// JavaScript callback function
    callback: js_sys::Function,
    /// Last received seq_id for resumption
    last_seq_id: Option<SeqId>,
}

/// WASM-compatible KalamDB client with auto-reconnection support
///
/// # Example (JavaScript)
/// ```js
/// import init, { KalamClient } from './pkg/kalam_link.js';
///
/// await init();
/// const client = new KalamClient(
///   "http://localhost:8080",
///   "username",
///   "password"
/// );
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
    username: String,
    password: String,
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

#[wasm_bindgen]
impl KalamClient {
    /// Create a new KalamDB client (T042, T043, T044)
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

        Ok(KalamClient {
            url,
            username,
            password,
            ws: Rc::new(RefCell::new(None)),
            subscription_state: Rc::new(RefCell::new(HashMap::new())),
            connection_options: Rc::new(RefCell::new(ConnectionOptions::default())),
            reconnect_attempts: Rc::new(RefCell::new(0)),
            is_reconnecting: Rc::new(RefCell::new(false)),
        })
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
        use wasm_bindgen_futures::JsFuture;

        // Check if already connected - prevent duplicate connections
        if self.is_connected() {
            console_log("KalamClient: Already connected, skipping reconnection");
            return Ok(());
        }

        // T063O: Add console.log debugging for connection state changes
        console_log("KalamClient: Connecting to WebSocket...");

        // Convert http(s) URL to ws(s) URL (no auth in URL)
        let ws_url = self
            .url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let ws_url = format!("{}/v1/ws", ws_url);

        // T063C: Implement proper WebSocket connection using web-sys::WebSocket
        let ws = WebSocket::new(&ws_url)?;

        // Create promises for connection and authentication
        let (connect_promise, connect_resolve, connect_reject) = {
            let mut resolve_fn: Option<js_sys::Function> = None;
            let mut reject_fn: Option<js_sys::Function> = None;

            let promise = js_sys::Promise::new(&mut |resolve, reject_arg| {
                resolve_fn = Some(resolve);
                reject_fn = Some(reject_arg);
            });

            (promise, resolve_fn.unwrap(), reject_fn.unwrap())
        };

        let (auth_promise, auth_resolve, auth_reject) = {
            let mut resolve_fn: Option<js_sys::Function> = None;
            let mut reject_fn: Option<js_sys::Function> = None;

            let promise = js_sys::Promise::new(&mut |resolve, reject_arg| {
                resolve_fn = Some(resolve);
                reject_fn = Some(reject_arg);
            });

            (promise, resolve_fn.unwrap(), reject_fn.unwrap())
        };

        // Clone credentials for the onopen handler
        let username = self.username.clone();
        let password = self.password.clone();
        let ws_clone_for_auth = ws.clone();
        
        // Set up onopen handler to send authentication message
        let connect_resolve_clone = connect_resolve.clone();
        let onopen_callback = Closure::wrap(Box::new(move || {
            console_log("KalamClient: WebSocket connected, sending authentication...");
            
            // Send authentication message
            let auth_msg = ClientMessage::Authenticate {
                username: username.clone(),
                password: password.clone(),
            };
            
            if let Ok(json) = serde_json::to_string(&auth_msg) {
                if let Err(e) = ws_clone_for_auth.send_with_str(&json) {
                    console_log(&format!("KalamClient: Failed to send auth message: {:?}", e));
                }
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
        let auth_handled = Rc::new(RefCell::new(false));
        let auth_handled_clone = Rc::clone(&auth_handled);
        
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message = String::from(txt);
                console_log(&format!(
                    "KalamClient: Received WebSocket message: {}",
                    message
                ));

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
                            }
                            ServerMessage::AuthError { message: error_msg } => {
                                console_log(&format!(
                                    "KalamClient: Authentication failed - {}",
                                    error_msg
                                ));
                                *auth_handled_clone.borrow_mut() = true;
                                let error = JsValue::from_str(&format!("Authentication failed: {}", error_msg));
                                let _ = auth_reject_clone2.call1(&JsValue::NULL, &error);
                                return;
                            }
                            _ => {} // Not an auth message, continue to subscription handling
                        }
                    }

                    // Look for subscription_id in the event and update last_seq_id
                    let subscription_id = match &event {
                        ServerMessage::SubscriptionAck {
                            subscription_id, ..
                        } => Some(subscription_id.clone()),
                        ServerMessage::InitialDataBatch {
                            subscription_id,
                            batch_control,
                            ..
                        } => {
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
                        }
                        ServerMessage::Change {
                            subscription_id, ..
                        } => Some(subscription_id.clone()),
                        ServerMessage::Error {
                            subscription_id, ..
                        } => Some(subscription_id.clone()),
                        _ => None, // Auth messages don't have subscription_id
                    };

                    if let Some(id) = subscription_id {
                        let subs = subscriptions.borrow();
                        // First try exact match
                        if let Some(state) = subs.get(&id) {
                            let _ = state.callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
                        } else {
                            // Server may prefix subscription_id with user_id-session_id-
                            // Try to find a callback where the server's ID ends with our client ID
                            for (client_id, state) in subs.iter() {
                                if id.ends_with(client_id) {
                                    let _ = state.callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
                                    break;
                                }
                            }
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
            .map_or(false, |ws| ws.ready_state() == WebSocket::OPEN)
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
        // Parse JSON data to build proper SQL INSERT statement
        let parsed: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| JsValue::from_str(&format!("Invalid JSON data: {}", e)))?;
        
        let obj = parsed.as_object()
            .ok_or_else(|| JsValue::from_str("Data must be a JSON object"))?;
        
        if obj.is_empty() {
            return Err(JsValue::from_str("Cannot insert empty object"));
        }
        
        // Build column names and values
        let columns: Vec<String> = obj.keys().cloned().collect();
        let values: Vec<String> = obj.values().map(|v| {
            match v {
                serde_json::Value::Null => "NULL".to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => format!("'{}'", s.replace("'", "''")), // SQL escape single quotes
                _ => format!("'{}'", v.to_string().replace("'", "''")),
            }
        }).collect();
        
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            columns.join(", "),
            values.join(", ")
        );
        
        self.execute_sql(&sql).await
    }

    /// Delete a row from a table (T049, T063H)
    ///
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `row_id` - ID of the row to delete
    pub async fn delete(&self, table_name: String, row_id: String) -> Result<(), JsValue> {
        // T063H: Implement using fetch API to execute DELETE statement via /v1/api/sql
        let sql = format!("DELETE FROM {} WHERE id = {}", table_name, row_id);
        self.execute_sql(&sql).await?;
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
        self.execute_sql(&sql).await
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
        // Default: SELECT * FROM table with default options
        let sql = format!("SELECT * FROM {}", table_name);
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
            return Err(JsValue::from_str(
                "Not connected to server. Call connect() first.",
            ));
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
                    sql,
                    options: subscription_options,
                },
            };
            let payload = serde_json::to_string(&subscribe_msg)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
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
            return Err(JsValue::from_str(
                "Not connected to server. Call connect() first.",
            ));
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

        console_log(&format!(
            "KalamClient: Unsubscribed from: {}",
            subscription_id
        ));
        Ok(())
    }

    /// Internal: Execute SQL via HTTP POST to /v1/api/sql (T063F)
    async fn execute_sql(&self, sql: &str) -> Result<String, JsValue> {
        // T063N: Add proper error handling with JsValue conversion
        let window =
            web_sys::window().ok_or_else(|| JsValue::from_str("No window object available"))?;

        // T063F: Implement HTTP fetch for SQL queries with Basic Auth
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);

        // Set headers with HTTP Basic Auth
        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;

        // Encode username:password as base64 for Authorization: Basic header
        let credentials = format!("{}:{}", self.username, self.password);
        let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
        headers.set("Authorization", &format!("Basic {}", encoded))?;
        opts.set_headers(&headers);

        // Set body
        let body = QueryRequest {
            sql: sql.to_string(),
            params: None,
        };
        let body_str = serde_json::to_string(&body)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;
        opts.set_body(&JsValue::from_str(&body_str));

        let url = format!("{}/v1/api/sql", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;

        // Execute fetch
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        // Check status
        if !resp.ok() {
            let status = resp.status();
            let text = JsFuture::from(resp.text()?).await?;
            let error_msg = text
                .as_string()
                .unwrap_or_else(|| format!("HTTP error {}", status));
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
        let username = self.username.clone();
        let password = self.password.clone();

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
            let username_clone = username.clone();
            let password_clone = password.clone();

            let reconnect_fn = Closure::wrap(Box::new(move || {
                *is_reconnecting_clone.borrow_mut() = true;
                *reconnect_attempts_clone.borrow_mut() += 1;

                let url = url_clone.clone();
                let username = username_clone.clone();
                let password = password_clone.clone();
                let ws_ref = ws_ref_clone.clone();
                let subscription_state = subscription_state_clone.clone();
                let is_reconnecting = is_reconnecting_clone.clone();
                let reconnect_attempts = reconnect_attempts_clone.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    match reconnect_internal(url, username, password, ws_ref.clone()).await {
                        Ok(()) => {
                            console_log("KalamClient: Reconnection successful");
                            *reconnect_attempts.borrow_mut() = 0; // Reset attempts on success
                            resubscribe_all(ws_ref, subscription_state).await;
                        }
                        Err(e) => {
                            console_log(&format!("KalamClient: Reconnection failed: {:?}", e));
                        }
                    }
                    *is_reconnecting.borrow_mut() = false;
                });
            }) as Box<dyn FnMut()>);

            let window = web_sys::window().unwrap();
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                reconnect_fn.as_ref().unchecked_ref(),
                delay as i32,
            );
            reconnect_fn.forget();
        }) as Box<dyn FnMut(CloseEvent)>);

        // Note: We add a second onclose handler for auto-reconnect
        // The first one just logs, this one handles reconnection
        ws.add_event_listener_with_callback(
            "close",
            onclose_reconnect.as_ref().unchecked_ref(),
        ).ok();
        onclose_reconnect.forget();
    }
}

// Simple hash function to generate unique subscription IDs from SQL
fn md5_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Internal reconnection logic
async fn reconnect_internal(
    url: String,
    username: String,
    password: String,
    ws_ref: Rc<RefCell<Option<WebSocket>>>,
) -> Result<(), JsValue> {
    let ws_url = url
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let ws_url = format!("{}/v1/ws", ws_url);

    let ws = WebSocket::new(&ws_url)?;

    let (connect_promise, connect_resolve, connect_reject) = create_promise();
    let (auth_promise, auth_resolve, auth_reject) = create_promise();

    let username_clone = username.clone();
    let password_clone = password.clone();
    let ws_clone = ws.clone();

    let connect_resolve_clone = connect_resolve.clone();
    let onopen = Closure::wrap(Box::new(move || {
        let auth_msg = ClientMessage::Authenticate {
            username: username_clone.clone(),
            password: password_clone.clone(),
        };
        if let Ok(json) = serde_json::to_string(&auth_msg) {
            let _ = ws_clone.send_with_str(&json);
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
    let auth_handled = Rc::new(RefCell::new(false));
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
                        }
                        ServerMessage::AuthError { message } => {
                            *auth_handled_clone.borrow_mut() = true;
                            let error = JsValue::from_str(&format!("Auth failed: {}", message));
                            let _ = auth_reject_clone2.call1(&JsValue::NULL, &error);
                        }
                        _ => {}
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

/// Helper to create a Promise with resolve/reject functions
fn create_promise() -> (js_sys::Promise, js_sys::Function, js_sys::Function) {
    let mut resolve_fn: Option<js_sys::Function> = None;
    let mut reject_fn: Option<js_sys::Function> = None;

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        resolve_fn = Some(resolve);
        reject_fn = Some(reject);
    });

    (promise, resolve_fn.unwrap(), reject_fn.unwrap())
}

/// Re-subscribe to all subscriptions after reconnection with last seq_id
async fn resubscribe_all(
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

// Helper function to log to browser console
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// T063O: Log helper for debugging connection state changes
fn console_log(_s: &str) {
    #[cfg(target_arch = "wasm32")]
    log(_s);
}
