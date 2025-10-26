// WASM bindings for KalamDB client (T041-T053, T063C-T063O)
// Provides JavaScript/TypeScript interface for browser and Node.js usage

#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use web_sys::{WebSocket, MessageEvent, CloseEvent, ErrorEvent, Request, RequestInit, RequestMode, Response, Headers};

/// WASM-compatible KalamDB client
/// 
/// # Example (JavaScript)
/// ```js
/// import init, { KalamClient } from './pkg/kalam_link.js';
/// 
/// await init();
/// const client = new KalamClient(
///   "http://localhost:8080",
///   "your-api-key-here"
/// );
/// ```
#[wasm_bindgen]
pub struct KalamClient {
    url: String,
    api_key: String,
    ws: Rc<RefCell<Option<WebSocket>>>,
    subscriptions: Rc<RefCell<HashMap<String, js_sys::Function>>>,
}

#[wasm_bindgen]
impl KalamClient {
    /// Create a new KalamDB client (T042, T043, T044)
    /// 
    /// # Arguments
    /// * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
    /// * `api_key` - API key for authentication (required, generated via create-user command)
    /// 
    /// # Errors
    /// Returns JsValue error if url or api_key is empty
    #[wasm_bindgen(constructor)]
    pub fn new(url: String, api_key: String) -> Result<KalamClient, JsValue> {
        // T044: Validate required parameters with clear error messages
        if url.is_empty() {
            return Err(JsValue::from_str("KalamClient: 'url' parameter is required and cannot be empty"));
        }
        if api_key.is_empty() {
            return Err(JsValue::from_str("KalamClient: 'api_key' parameter is required and cannot be empty"));
        }

        Ok(KalamClient {
            url,
            api_key,
            ws: Rc::new(RefCell::new(None)),
            subscriptions: Rc::new(RefCell::new(HashMap::new())),
        })
    }

    /// Connect to KalamDB server via WebSocket (T045, T063C-T063D)
    /// 
    /// # Returns
    /// Promise that resolves when connection is established
    pub async fn connect(&mut self) -> Result<(), JsValue> {
        use wasm_bindgen_futures::JsFuture;
        
        // T063O: Add console.log debugging for connection state changes
        console_log("KalamClient: Connecting to WebSocket...");
        
        // Convert http(s) URL to ws(s) URL
        let ws_url = self.url.replace("http://", "ws://").replace("https://", "wss://");
        // T063AAB: Pass API key as query parameter for WebSocket authentication
        let ws_url = format!("{}/v1/ws?api_key={}", ws_url, self.api_key);
        
        // T063C: Implement proper WebSocket connection using web-sys::WebSocket
        let ws = WebSocket::new(&ws_url)?;
        
        // Create a promise that resolves when WebSocket opens
        let (promise, resolve, reject) = {
            let mut resolve_fn: Option<js_sys::Function> = None;
            let mut reject_fn: Option<js_sys::Function> = None;
            
            let promise = js_sys::Promise::new(&mut |resolve, reject_arg| {
                resolve_fn = Some(resolve);
                reject_fn = Some(reject_arg);
            });
            
            (promise, resolve_fn.unwrap(), reject_fn.unwrap())
        };
        
        // Set up onopen handler to resolve the promise
        let resolve_clone = resolve.clone();
        let onopen_callback = Closure::wrap(Box::new(move || {
            console_log("KalamClient: WebSocket connected");
            let _ = resolve_clone.call0(&JsValue::NULL);
        }) as Box<dyn FnMut()>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();
        
        // T063L: Implement WebSocket error and close handlers
        let reject_clone = reject.clone();
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            console_log(&format!("KalamClient: WebSocket error: {:?}", e));
            let error_msg = JsValue::from_str("WebSocket connection failed");
            let _ = reject_clone.call1(&JsValue::NULL, &error_msg);
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let onclose_callback = Closure::wrap(Box::new(move |e: CloseEvent| {
            console_log(&format!("KalamClient: WebSocket closed: code={}, reason={}", e.code(), e.reason()));
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        // T063K: Implement WebSocket onmessage handler to parse events and invoke registered callbacks
        let subscriptions = Rc::clone(&self.subscriptions);
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let message = String::from(txt);
                console_log(&format!("KalamClient: Received WebSocket message: {}", message));
                
                // Parse message and invoke callbacks
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&message) {
                    // Look for subscription_id in the event
                    if let Some(subscription_id) = event.get("subscription_id").and_then(|t| t.as_str()) {
                        let subs = subscriptions.borrow();
                        if let Some(callback) = subs.get(subscription_id) {
                            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&message));
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
        JsFuture::from(promise).await?;
        
        console_log("KalamClient: WebSocket connection established");
        Ok(())
    }

    /// Disconnect from KalamDB server (T046, T063E)
    pub async fn disconnect(&mut self) -> Result<(), JsValue> {
        console_log("KalamClient: Disconnecting from WebSocket...");
        
        // T063E: Properly close WebSocket and cleanup resources
        if let Some(ws) = self.ws.borrow_mut().take() {
            ws.close()?;
        }
        
        // Clear all subscriptions
        self.subscriptions.borrow_mut().clear();
        
        console_log("KalamClient: Disconnected");
        Ok(())
    }

    /// Check if client is currently connected (T047)
    /// 
    /// # Returns
    /// true if WebSocket connection is active, false otherwise
    #[wasm_bindgen(js_name = isConnected)]
    pub fn is_connected(&self) -> bool {
        self.ws.borrow().as_ref().map_or(false, |ws| ws.ready_state() == WebSocket::OPEN)
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
        // T063G: Implement using fetch API to execute INSERT statement via /v1/api/sql
        let sql = format!("INSERT INTO {} VALUES {}", table_name, data);
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
        // T063F: Implement query() using web-sys fetch API with X-API-KEY header
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
        if !self.is_connected() {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // T063J: Store subscription callbacks in HashMap for proper lifetime management
        let subscription_id = format!("sub-{}", table_name);
        self.subscriptions.borrow_mut().insert(subscription_id.clone(), callback);
        
        // Send subscribe message via WebSocket
        // Server expects: {"subscriptions": [{"id": "sub-1", "sql": "SELECT * FROM ...", "options": {}}]}
        if let Some(ws) = self.ws.borrow().as_ref() {
            let subscribe_msg = serde_json::json!({
                "subscriptions": [{
                    "id": subscription_id,
                    "sql": format!("SELECT * FROM {}", table_name),
                    "options": {}
                }]
            });
            ws.send_with_str(&subscribe_msg.to_string())?;
        }
        
        console_log(&format!("KalamClient: Subscribed to table: {}", table_name));
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

        // T063M: Remove callback from HashMap and send unsubscribe message
        self.subscriptions.borrow_mut().remove(&subscription_id);
        
        // Send unsubscribe message via WebSocket
        // Note: Current server doesn't have unsubscribe - connection close will clean up
        if let Some(ws) = self.ws.borrow().as_ref() {
            let unsubscribe_msg = serde_json::json!({
                "unsubscribe": [subscription_id.clone()]
            });
            ws.send_with_str(&unsubscribe_msg.to_string())?;
        }
        
        console_log(&format!("KalamClient: Unsubscribed from: {}", subscription_id));
        Ok(())
    }

    /// Internal: Execute SQL via HTTP POST to /v1/api/sql (T063F)
    async fn execute_sql(&self, sql: &str) -> Result<String, JsValue> {
        // T063N: Add proper error handling with JsValue conversion
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object available"))?;
        
        // T063F: Implement HTTP fetch for SQL queries with X-API-KEY header
        let opts = RequestInit::new();
        opts.set_method("POST");
        opts.set_mode(RequestMode::Cors);
        
        // Set headers
        let headers = Headers::new()?;
        headers.set("Content-Type", "application/json")?;
        headers.set("X-API-KEY", &self.api_key)?;
        opts.set_headers(&headers);
        
        // Set body
        let body = serde_json::json!({ "sql": sql });
        let body_str = JsValue::from_str(&body.to_string());
        opts.set_body(&body_str);
        
        let url = format!("{}/v1/api/sql", self.url);
        let request = Request::new_with_str_and_init(&url, &opts)?;
        
        // Execute fetch
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
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

