// Snowflake ID generator
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Snowflake ID generator for time-ordered unique identifiers
///
/// Format (64 bits):
/// - 41 bits: timestamp in milliseconds since custom epoch
/// - 10 bits: machine/worker ID
/// - 12 bits: sequence number
pub struct SnowflakeGenerator {
    /// Machine/worker ID (0-1023)
    worker_id: u16,

    /// Custom epoch (milliseconds since Unix epoch)
    /// Default: 2024-01-01 00:00:00 UTC (1704067200000)
    epoch: u64,

    /// State protected by mutex
    state: Mutex<GeneratorState>,
}

struct GeneratorState {
    /// Last timestamp used
    last_timestamp: u64,

    /// Sequence number (0-4095)
    sequence: u16,
}

impl SnowflakeGenerator {
    /// Custom epoch: 2024-01-01 00:00:00 UTC
    pub const DEFAULT_EPOCH: u64 = 1704067200000;

    /// Maximum worker ID
    pub const MAX_WORKER_ID: u16 = 1023;

    /// Maximum sequence number
    const MAX_SEQUENCE: u16 = 4095;

    /// Create a new Snowflake ID generator
    pub fn new(worker_id: u16) -> Self {
        Self::with_epoch(worker_id, Self::DEFAULT_EPOCH)
    }

    /// Create a new Snowflake ID generator with custom epoch
    pub fn with_epoch(worker_id: u16, epoch: u64) -> Self {
        assert!(
            worker_id <= Self::MAX_WORKER_ID,
            "worker_id must be <= {}",
            Self::MAX_WORKER_ID
        );

        Self {
            worker_id,
            epoch,
            state: Mutex::new(GeneratorState {
                last_timestamp: 0,
                sequence: 0,
            }),
        }
    }

    /// Generate the next Snowflake ID
    pub fn next_id(&self) -> Result<i64, String> {
        let mut state = self.state.lock().unwrap();

        let mut timestamp = self.current_timestamp()?;

        // Handle clock going backwards
        if timestamp < state.last_timestamp {
            return Err(format!(
                "Clock moved backwards. Refusing to generate id for {} milliseconds",
                state.last_timestamp - timestamp
            ));
        }

        if timestamp == state.last_timestamp {
            // Same millisecond - increment sequence
            state.sequence = (state.sequence + 1) & Self::MAX_SEQUENCE;

            if state.sequence == 0 {
                // Sequence overflow - wait for next millisecond
                timestamp = self.wait_next_millis(state.last_timestamp)?;
            }
        } else {
            // New millisecond - reset sequence
            state.sequence = 0;
        }

        state.last_timestamp = timestamp;

        // Construct the ID
        let id = ((timestamp - self.epoch) << 22)
            | ((self.worker_id as u64) << 12)
            | (state.sequence as u64);

        Ok(id as i64)
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp(&self) -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .map_err(|e| format!("Failed to get current timestamp: {}", e))
    }

    /// Wait until next millisecond
    fn wait_next_millis(&self, last_timestamp: u64) -> Result<u64, String> {
        let mut timestamp = self.current_timestamp()?;
        while timestamp <= last_timestamp {
            timestamp = self.current_timestamp()?;
        }
        Ok(timestamp)
    }

    /// Extract timestamp from a Snowflake ID
    pub fn extract_timestamp(&self, id: i64) -> u64 {
        let id = id as u64;
        (id >> 22) + self.epoch
    }

    /// Extract worker ID from a Snowflake ID
    pub fn extract_worker_id(&self, id: i64) -> u16 {
        let id = id as u64;
        ((id >> 12) & 0x3FF) as u16
    }

    /// Extract sequence from a Snowflake ID
    pub fn extract_sequence(&self, id: i64) -> u16 {
        let id = id as u64;
        (id & 0xFFF) as u16
    }
}

impl Default for SnowflakeGenerator {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_snowflake_generation() {
        let gen = SnowflakeGenerator::new(1);
        let id = gen.next_id().unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_snowflake_uniqueness() {
        let gen = SnowflakeGenerator::new(1);
        let mut ids = HashSet::new();

        for _ in 0..10000 {
            let id = gen.next_id().unwrap();
            assert!(ids.insert(id), "Duplicate ID generated: {}", id);
        }
    }

    #[test]
    fn test_snowflake_ordering() {
        let gen = SnowflakeGenerator::new(1);
        let mut last_id = 0i64;

        for _ in 0..1000 {
            let id = gen.next_id().unwrap();
            assert!(id > last_id, "IDs not in order: {} <= {}", id, last_id);
            last_id = id;
        }
    }

    #[test]
    fn test_extract_timestamp() {
        let gen = SnowflakeGenerator::new(1);
        let id = gen.next_id().unwrap();
        let timestamp = gen.extract_timestamp(id);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Timestamp should be within 1 second of now
        assert!((timestamp as i64 - now as i64).abs() < 1000);
    }

    #[test]
    fn test_extract_worker_id() {
        let worker_id = 42;
        let gen = SnowflakeGenerator::new(worker_id);
        let id = gen.next_id().unwrap();
        let extracted = gen.extract_worker_id(id);

        assert_eq!(extracted, worker_id);
    }

    #[test]
    fn test_extract_sequence() {
        let gen = SnowflakeGenerator::new(1);

        // Generate multiple IDs in same millisecond
        let id1 = gen.next_id().unwrap();
        let id2 = gen.next_id().unwrap();

        let seq1 = gen.extract_sequence(id1);
        let seq2 = gen.extract_sequence(id2);

        // Sequences should be different (likely seq2 = seq1 + 1)
        assert!(seq2 >= seq1);
    }

    #[test]
    #[should_panic(expected = "worker_id must be")]
    fn test_invalid_worker_id() {
        SnowflakeGenerator::new(2000);
    }

    #[test]
    fn test_max_worker_id() {
        let gen = SnowflakeGenerator::new(SnowflakeGenerator::MAX_WORKER_ID);
        let id = gen.next_id().unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_custom_epoch() {
        let custom_epoch = 1600000000000; // Sept 2020
        let gen = SnowflakeGenerator::with_epoch(1, custom_epoch);
        let id = gen.next_id().unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_concurrent_generation() {
        use std::sync::Arc;
        use std::thread;

        let gen = Arc::new(SnowflakeGenerator::new(1));
        let mut handles = vec![];

        for _ in 0..10 {
            let gen_clone = Arc::clone(&gen);
            let handle = thread::spawn(move || {
                let mut ids = Vec::new();
                for _ in 0..100 {
                    ids.push(gen_clone.next_id().unwrap());
                }
                ids
            });
            handles.push(handle);
        }

        let mut all_ids = HashSet::new();
        for handle in handles {
            let ids = handle.join().unwrap();
            for id in ids {
                assert!(all_ids.insert(id), "Duplicate ID in concurrent test");
            }
        }

        assert_eq!(all_ids.len(), 1000);
    }
}
