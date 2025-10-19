//! Flush trigger logic for monitoring and triggering flush operations
//!
//! This module monitors row count and time intervals per column family to determine
//! when flush operations should be triggered based on configured flush policies.

use crate::catalog::TableName;
use crate::error::KalamDbError;
use crate::flush::FlushPolicy;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Flush trigger state for a single table/column family
#[derive(Debug, Clone)]
pub struct FlushTriggerState {
    /// Table name
    pub table_name: TableName,
    
    /// Column family name
    pub cf_name: String,
    
    /// Flush policy for this table
    pub flush_policy: FlushPolicy,
    
    /// Current row count (approximate)
    pub current_row_count: usize,
    
    /// Last flush timestamp
    pub last_flush_time: DateTime<Utc>,
}

impl FlushTriggerState {
    /// Create a new flush trigger state
    pub fn new(
        table_name: TableName,
        cf_name: String,
        flush_policy: FlushPolicy,
    ) -> Self {
        Self {
            table_name,
            cf_name,
            flush_policy,
            current_row_count: 0,
            last_flush_time: Utc::now(),
        }
    }

    /// Check if flush should be triggered based on current state
    ///
    /// # Returns
    /// `true` if flush should be triggered, `false` otherwise
    pub fn should_flush(&self) -> bool {
        let now = Utc::now();

        match &self.flush_policy {
            FlushPolicy::RowLimit { row_limit } => {
                // Flush if row count >= limit
                self.current_row_count >= *row_limit as usize
            }
            FlushPolicy::TimeInterval { interval_seconds } => {
                // Flush if time since last flush >= interval
                let elapsed = now.signed_duration_since(self.last_flush_time);
                elapsed.num_seconds() >= *interval_seconds as i64
            }
            FlushPolicy::Combined {
                row_limit,
                interval_seconds,
            } => {
                // Flush if EITHER condition is met
                let row_condition = self.current_row_count >= *row_limit as usize;
                let time_condition = {
                    let elapsed = now.signed_duration_since(self.last_flush_time);
                    elapsed.num_seconds() >= *interval_seconds as i64
                };
                row_condition || time_condition
            }
        }
    }

    /// Increment row count
    pub fn increment_row_count(&mut self, count: usize) {
        self.current_row_count += count;
    }

    /// Reset state after flush
    pub fn reset_after_flush(&mut self) {
        self.current_row_count = 0;
        self.last_flush_time = Utc::now();
    }
}

/// Flush trigger monitor
///
/// Tracks flush state for all tables and determines when to trigger flush jobs
pub struct FlushTriggerMonitor {
    /// Flush state per column family (keyed by column family name)
    states: Arc<RwLock<HashMap<String, FlushTriggerState>>>,
}

impl FlushTriggerMonitor {
    /// Create a new flush trigger monitor
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a table for flush monitoring
    ///
    /// # Arguments
    /// * `table_name` - Name of the table
    /// * `cf_name` - Column family name
    /// * `flush_policy` - Flush policy for this table
    pub fn register_table(
        &self,
        table_name: TableName,
        cf_name: String,
        flush_policy: FlushPolicy,
    ) -> Result<(), KalamDbError> {
        let mut states = self.states.write().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire write lock: {}", e))
        })?;

        let state = FlushTriggerState::new(table_name, cf_name.clone(), flush_policy);
        states.insert(cf_name, state);

        Ok(())
    }

    /// Unregister a table from flush monitoring
    ///
    /// # Arguments
    /// * `cf_name` - Column family name
    pub fn unregister_table(&self, cf_name: &str) -> Result<(), KalamDbError> {
        let mut states = self.states.write().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire write lock: {}", e))
        })?;

        states.remove(cf_name);

        Ok(())
    }

    /// Notify the monitor that rows were inserted
    ///
    /// # Arguments
    /// * `cf_name` - Column family name
    /// * `row_count` - Number of rows inserted
    pub fn on_rows_inserted(
        &self,
        cf_name: &str,
        row_count: usize,
    ) -> Result<(), KalamDbError> {
        let mut states = self.states.write().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire write lock: {}", e))
        })?;

        if let Some(state) = states.get_mut(cf_name) {
            state.increment_row_count(row_count);
        }

        Ok(())
    }

    /// Check if flush should be triggered for a table
    ///
    /// # Arguments
    /// * `cf_name` - Column family name
    ///
    /// # Returns
    /// `true` if flush should be triggered, `false` otherwise
    pub fn should_flush(&self, cf_name: &str) -> Result<bool, KalamDbError> {
        let states = self.states.read().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire read lock: {}", e))
        })?;

        if let Some(state) = states.get(cf_name) {
            Ok(state.should_flush())
        } else {
            // Table not registered - don't trigger flush
            Ok(false)
        }
    }

    /// Get tables that need flushing
    ///
    /// # Returns
    /// Vector of (table_name, cf_name) pairs that need flushing
    pub fn get_tables_needing_flush(&self) -> Result<Vec<(TableName, String)>, KalamDbError> {
        let states = self.states.read().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire read lock: {}", e))
        })?;

        let mut result = Vec::new();
        for (cf_name, state) in states.iter() {
            if state.should_flush() {
                result.push((state.table_name.clone(), cf_name.clone()));
            }
        }

        Ok(result)
    }

    /// Notify the monitor that a flush completed
    ///
    /// # Arguments
    /// * `cf_name` - Column family name
    pub fn on_flush_completed(&self, cf_name: &str) -> Result<(), KalamDbError> {
        let mut states = self.states.write().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire write lock: {}", e))
        })?;

        if let Some(state) = states.get_mut(cf_name) {
            state.reset_after_flush();
        }

        Ok(())
    }

    /// Get current state for a table
    ///
    /// # Arguments
    /// * `cf_name` - Column family name
    ///
    /// # Returns
    /// Clone of the current flush trigger state
    pub fn get_state(&self, cf_name: &str) -> Result<Option<FlushTriggerState>, KalamDbError> {
        let states = self.states.read().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire read lock: {}", e))
        })?;

        Ok(states.get(cf_name).cloned())
    }

    /// Get all registered tables
    ///
    /// # Returns
    /// Vector of column family names
    pub fn get_registered_tables(&self) -> Result<Vec<String>, KalamDbError> {
        let states = self.states.read().map_err(|e| {
            KalamDbError::Other(format!("Failed to acquire read lock: {}", e))
        })?;

        Ok(states.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_flush_trigger_state_row_limit() {
        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::row_limit(100).unwrap();

        let mut state = FlushTriggerState::new(table_name, cf_name, flush_policy);

        // Should not flush initially
        assert!(!state.should_flush());

        // Add 50 rows - should not flush
        state.increment_row_count(50);
        assert!(!state.should_flush());

        // Add 50 more rows (total 100) - should flush
        state.increment_row_count(50);
        assert!(state.should_flush());

        // Reset after flush
        state.reset_after_flush();
        assert_eq!(state.current_row_count, 0);
        assert!(!state.should_flush());
    }

    #[test]
    fn test_flush_trigger_state_time_interval() {
        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::time_interval(1).unwrap(); // 1 second

        let mut state = FlushTriggerState::new(table_name, cf_name, flush_policy);

        // Should not flush initially
        assert!(!state.should_flush());

        // Wait 2 seconds - should flush
        thread::sleep(Duration::from_secs(2));
        assert!(state.should_flush());

        // Reset after flush
        state.reset_after_flush();
        assert!(!state.should_flush());
    }

    #[test]
    fn test_flush_trigger_state_combined() {
        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::combined(100, 10).unwrap();

        let mut state = FlushTriggerState::new(table_name, cf_name, flush_policy);

        // Should not flush initially
        assert!(!state.should_flush());

        // Add 100 rows - should flush (row condition met)
        state.increment_row_count(100);
        assert!(state.should_flush());

        // Reset and test time condition
        state.reset_after_flush();
        assert!(!state.should_flush());

        // Add only 50 rows but wait 11 seconds - should flush (time condition met)
        state.increment_row_count(50);
        thread::sleep(Duration::from_secs(11));
        assert!(state.should_flush());
    }

    #[test]
    fn test_flush_trigger_monitor_registration() {
        
        let monitor = FlushTriggerMonitor::new();

        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::row_limit(100).unwrap();

        // Register table
        let result = monitor.register_table(table_name.clone(), cf_name.clone(), flush_policy);
        assert!(result.is_ok());

        // Verify registration
        let registered = monitor.get_registered_tables().unwrap();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0], cf_name);

        // Unregister table
        let result = monitor.unregister_table(&cf_name);
        assert!(result.is_ok());

        // Verify unregistration
        let registered = monitor.get_registered_tables().unwrap();
        assert_eq!(registered.len(), 0);
    }

    #[test]
    fn test_flush_trigger_monitor_row_tracking() {
        
        let monitor = FlushTriggerMonitor::new();

        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::row_limit(100).unwrap();

        monitor
            .register_table(table_name, cf_name.clone(), flush_policy)
            .unwrap();

        // Initially should not flush
        assert!(!monitor.should_flush(&cf_name).unwrap());

        // Insert 50 rows
        monitor.on_rows_inserted(&cf_name, 50).unwrap();
        assert!(!monitor.should_flush(&cf_name).unwrap());

        // Insert 50 more rows (total 100)
        monitor.on_rows_inserted(&cf_name, 50).unwrap();
        assert!(monitor.should_flush(&cf_name).unwrap());

        // Complete flush
        monitor.on_flush_completed(&cf_name).unwrap();
        assert!(!monitor.should_flush(&cf_name).unwrap());
    }

    #[test]
    fn test_get_tables_needing_flush() {
        
        let monitor = FlushTriggerMonitor::new();

        // Register multiple tables with different policies
        let table1 = TableName::new("table1");
        let cf1 = "user_table:test:table1".to_string();
        let policy1 = FlushPolicy::row_limit(100).unwrap();

        let table2 = TableName::new("table2");
        let cf2 = "user_table:test:table2".to_string();
        let policy2 = FlushPolicy::row_limit(50).unwrap();

        monitor
            .register_table(table1.clone(), cf1.clone(), policy1)
            .unwrap();
        monitor
            .register_table(table2.clone(), cf2.clone(), policy2)
            .unwrap();

        // Initially no tables need flushing
        let needing_flush = monitor.get_tables_needing_flush().unwrap();
        assert_eq!(needing_flush.len(), 0);

        // Insert rows into table1 (100 rows - triggers flush)
        monitor.on_rows_inserted(&cf1, 100).unwrap();

        // Insert rows into table2 (30 rows - doesn't trigger flush)
        monitor.on_rows_inserted(&cf2, 30).unwrap();

        // Only table1 should need flushing
        let needing_flush = monitor.get_tables_needing_flush().unwrap();
        assert_eq!(needing_flush.len(), 1);
        assert_eq!(needing_flush[0].0, table1);
        assert_eq!(needing_flush[0].1, cf1);

        // Insert more rows into table2 (30 more - total 60, triggers flush)
        monitor.on_rows_inserted(&cf2, 30).unwrap();

        // Both tables should need flushing
        let needing_flush = monitor.get_tables_needing_flush().unwrap();
        assert_eq!(needing_flush.len(), 2);
    }

    #[test]
    fn test_get_state() {
        
        let monitor = FlushTriggerMonitor::new();

        let table_name = TableName::new("test_table");
        let cf_name = "user_table:test:test_table".to_string();
        let flush_policy = FlushPolicy::row_limit(100).unwrap();

        monitor
            .register_table(table_name.clone(), cf_name.clone(), flush_policy)
            .unwrap();

        // Get initial state
        let state = monitor.get_state(&cf_name).unwrap();
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.table_name, table_name);
        assert_eq!(state.current_row_count, 0);

        // Insert rows
        monitor.on_rows_inserted(&cf_name, 50).unwrap();

        // Get updated state
        let state = monitor.get_state(&cf_name).unwrap().unwrap();
        assert_eq!(state.current_row_count, 50);
    }
}
