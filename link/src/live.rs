use crate::error::{KalamLinkError, Result};
use crate::models::{ChangeEvent, SubscriptionConfig};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Shared interface for live connection lifecycle management.
///
/// This struct manages a multiplexed WebSocket connection and its subscriptions.
/// It is intended to be used by both the Rust client and the WASM bindings.
pub struct LiveConnection {
    #[allow(dead_code)] // Will be used when WebSocket connection is implemented
    base_url: String,
    subscriptions: Arc<RwLock<HashMap<String, SubscriptionConfig>>>,
    connected: Arc<RwLock<bool>>,
}

impl LiveConnection {
    /// Create a new live connection manager
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Connect to the WebSocket server (if not already connected).
    pub async fn connect(&self) -> Result<()> {
        // TODO: Implement actual WebSocket connection logic
        // For now, we just mark as connected to satisfy the interface
        let mut connected = self.connected.write().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        *connected = true;
        Ok(())
    }

    /// Disconnect from the WebSocket server.
    pub async fn disconnect(&self) -> Result<()> {
        let mut connected = self.connected.write().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        *connected = false;
        Ok(())
    }

    /// Subscribe to a live query.
    ///
    /// Returns a subscription ID.
    pub async fn subscribe(&self, config: SubscriptionConfig) -> Result<String> {
        // Generate a simple unique ID
        let id = format!("sub_{}", Self::generate_id());
        
        let mut subs = self.subscriptions.write().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        subs.insert(id.clone(), config);
        
        // TODO: If connected, send subscription message
        
        Ok(id)
    }

    /// Unsubscribe from a live query.
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<()> {
        let mut subs = self.subscriptions.write().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        subs.remove(subscription_id);
        
        // TODO: If connected, send unsubscribe message
        
        Ok(())
    }

    /// List all active subscriptions.
    pub async fn list_subscriptions(&self) -> Result<Vec<String>> {
        let subs = self.subscriptions.read().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        Ok(subs.keys().cloned().collect())
    }
    
    /// Resume subscriptions after reconnection (internal use mostly, but exposed for testing/manual control)
    pub async fn resume(&self) -> Result<()> {
        let subs = self.subscriptions.read().map_err(|e| KalamLinkError::InternalError(e.to_string()))?;
        if !subs.is_empty() {
            // TODO: Re-send all subscriptions
        }
        Ok(())
    }

    /// Get the next event for a specific subscription (or any if subscription_id is None).
    /// This is a simplified polling interface for WASM/FFI.
    pub async fn next_event(&self) -> Option<Result<ChangeEvent>> {
        // TODO: Implement event queue consumption
        None
    }
    
    fn generate_id() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        since_the_epoch.as_nanos() as u64
    }
}
