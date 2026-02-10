//! Atomic offset allocation for topic partitions.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Manages per-topic-partition offset counters using atomic operations.
///
/// Each counter is an `AtomicU64` behind an `Arc`, so concurrent callers
/// on different tokio tasks can increment without holding a DashMap shard lock
/// during the actual increment.
pub(crate) struct OffsetAllocator {
    /// Key: "topic_id:partition_id" → atomic counter
    counters: DashMap<String, Arc<AtomicU64>>,
}

impl OffsetAllocator {
    pub fn new() -> Self {
        Self { counters: DashMap::new() }
    }

    /// Get the next offset for a topic-partition and atomically increment.
    pub fn next_offset(&self, topic_id: &str, partition_id: u32) -> u64 {
        let key = format!("{}:{}", topic_id, partition_id);

        // Get or insert an AtomicU64 counter.
        // The DashMap shard lock is held only for the lookup/insert, not the increment.
        let counter = self
            .counters
            .entry(key)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();

        counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Seed a counter to a specific value (used during restore from persisted messages).
    pub fn seed(&self, topic_id: &str, partition_id: u32, next_offset: u64) {
        let key = format!("{}:{}", topic_id, partition_id);
        self.counters.insert(key, Arc::new(AtomicU64::new(next_offset)));
    }

    /// Clear all counters.
    pub fn clear(&self) {
        self.counters.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monotonic_offsets() {
        let alloc = OffsetAllocator::new();
        assert_eq!(alloc.next_offset("topic1", 0), 0);
        assert_eq!(alloc.next_offset("topic1", 0), 1);
        assert_eq!(alloc.next_offset("topic1", 0), 2);
    }

    #[test]
    fn test_independent_partitions() {
        let alloc = OffsetAllocator::new();
        assert_eq!(alloc.next_offset("topic1", 0), 0);
        assert_eq!(alloc.next_offset("topic1", 1), 0);
        assert_eq!(alloc.next_offset("topic1", 0), 1);
        assert_eq!(alloc.next_offset("topic1", 1), 1);
    }

    #[test]
    fn test_seed() {
        let alloc = OffsetAllocator::new();
        alloc.seed("topic1", 0, 100);
        assert_eq!(alloc.next_offset("topic1", 0), 100);
        assert_eq!(alloc.next_offset("topic1", 0), 101);
    }

    #[test]
    fn test_concurrent_offsets() {
        use std::thread;

        let alloc = Arc::new(OffsetAllocator::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let alloc = Arc::clone(&alloc);
            handles.push(thread::spawn(move || {
                let mut offsets = vec![];
                for _ in 0..100 {
                    offsets.push(alloc.next_offset("topic1", 0));
                }
                offsets
            }));
        }

        let mut all_offsets: Vec<u64> = vec![];
        for handle in handles {
            all_offsets.extend(handle.join().unwrap());
        }

        // All 1000 offsets should be unique
        all_offsets.sort();
        all_offsets.dedup();
        assert_eq!(all_offsets.len(), 1000);
        // Should cover 0..1000
        assert_eq!(*all_offsets.first().unwrap(), 0);
        assert_eq!(*all_offsets.last().unwrap(), 999);
    }
}
