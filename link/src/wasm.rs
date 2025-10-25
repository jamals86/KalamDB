// WASM bindings for KalamDB client (T041-T053)
// Provides JavaScript/TypeScript interface for browser and Node.js usage

#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    connected: bool,
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
            connected: false,
        })
    }

    /// Connect to KalamDB server via WebSocket (T045)
    /// 
    /// # Returns
    /// Promise that resolves when connection is established
    pub async fn connect(&mut self) -> Result<(), JsValue> {
        // TODO: Implement WebSocket connection
        // For now, just mark as connected
        self.connected = true;
        Ok(())
    }

    /// Disconnect from KalamDB server (T046)
    pub async fn disconnect(&mut self) -> Result<(), JsValue> {
        self.connected = false;
        Ok(())
    }

    /// Check if client is currently connected (T047)
    /// 
    /// # Returns
    /// true if WebSocket connection is active, false otherwise
    #[wasm_bindgen(js_name = isConnected)]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Insert data into a table (T048)
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
        if !self.connected {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // T053: Add X-API-KEY header to requests
        // TODO: Implement HTTP POST with API key header
        Ok(format!("{{\"id\": \"placeholder\", \"table\": \"{}\"}}", table_name))
    }

    /// Delete a row from a table (T049)
    /// 
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `row_id` - ID of the row to delete
    pub async fn delete(&self, table_name: String, row_id: String) -> Result<(), JsValue> {
        if !self.connected {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // T053: Add X-API-KEY header to requests
        // TODO: Implement HTTP DELETE with API key header
        Ok(())
    }

    /// Execute a SQL query (T050)
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
        if !self.connected {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // T053: Add X-API-KEY header to requests
        // TODO: Implement HTTP POST to /api/v1/sql with API key header
        Ok("[]".to_string())
    }

    /// Subscribe to table changes (T051)
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
        if !self.connected {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // T053: Add X-API-KEY header to WebSocket requests
        // TODO: Implement WebSocket subscription with API key
        Ok("subscription-id-placeholder".to_string())
    }

    /// Unsubscribe from table changes (T052)
    /// 
    /// # Arguments
    /// * `subscription_id` - ID returned from subscribe()
    pub async fn unsubscribe(&self, subscription_id: String) -> Result<(), JsValue> {
        if !self.connected {
            return Err(JsValue::from_str("Not connected to server. Call connect() first."));
        }

        // TODO: Implement WebSocket unsubscribe
        Ok(())
    }
}

// Helper function to log to browser console
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// Log helper for debugging
#[allow(dead_code)]
fn console_log(s: &str) {
    #[cfg(target_arch = "wasm32")]
    log(s);
}
