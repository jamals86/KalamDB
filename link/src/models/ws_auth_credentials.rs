use serde::{Deserialize, Serialize};

/// Authentication credentials for WebSocket connection
///
/// This enum mirrors `WsAuthCredentials` from the backend (`kalamdb-commons/websocket.rs`).
/// Both enums must stay in sync for proper serialization/deserialization.
///
/// # Supported Methods
///
/// - `Basic` - Username/password authentication
/// - `Jwt` - JWT token (Bearer) authentication
///
/// # JSON Wire Format
///
/// ```json
/// // Basic Auth
/// {"type": "authenticate", "method": "basic", "username": "alice", "password": "secret"}
///
/// // JWT Auth
/// {"type": "authenticate", "method": "jwt", "token": "eyJhbGciOiJIUzI1NiIs..."}
/// ```
///
/// # Adding a New Authentication Method
///
/// 1. Add variant here (client side)
/// 2. Add matching variant to backend's `WsAuthCredentials`
/// 3. Update `WasmAuthProvider` in `wasm.rs` if WASM support needed
/// 4. Update TypeScript `Auth` class in SDK
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum WsAuthCredentials {
    /// Username and password authentication
    Basic { username: String, password: String },
    /// JWT token authentication
    Jwt { token: String },
    // Future auth methods can be added here:
    // ApiKey { key: String },
    // OAuth { provider: String, token: String },
}
