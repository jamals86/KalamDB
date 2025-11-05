// //! Configuration model structures for KalamDB.
// //!
// //! This module provides shared configuration structures used across all KalamDB crates.

// /// Flush policy configuration for tables.
// ///
// /// Determines when buffered data should be flushed from RocksDB to Parquet.
// #[derive(Debug, Clone, PartialEq, Eq)]
// pub enum FlushPolicy {
//     /// Flush after N rows per user-table combination
//     Rows(u64),

//     /// Flush after time interval (in seconds)
//     Interval(u64),

//     /// Flush when either condition is met
//     Combined { rows: u64, interval_seconds: u64 },

//     /// Manual flush only (no automatic flushing)
//     Manual,
// }

// impl FlushPolicy {
//     /// Returns the row threshold, if applicable.
//     pub fn row_threshold(&self) -> Option<u64> {
//         match self {
//             FlushPolicy::Rows(n) => Some(*n),
//             FlushPolicy::Combined { rows, .. } => Some(*rows),
//             _ => None,
//         }
//     }

//     /// Returns the interval threshold in seconds, if applicable.
//     pub fn interval_seconds(&self) -> Option<u64> {
//         match self {
//             FlushPolicy::Interval(s) => Some(*s),
//             FlushPolicy::Combined {
//                 interval_seconds, ..
//             } => Some(*interval_seconds),
//             _ => None,
//         }
//     }

//     /// Returns true if automatic flushing is enabled.
//     pub fn is_automatic(&self) -> bool {
//         !matches!(self, FlushPolicy::Manual)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_flush_policy() {
//         let policy = FlushPolicy::Rows(1000);
//         assert_eq!(policy.row_threshold(), Some(1000));
//         assert_eq!(policy.interval_seconds(), None);
//         assert!(policy.is_automatic());

//         let policy = FlushPolicy::Combined {
//             rows: 1000,
//             interval_seconds: 300,
//         };
//         assert_eq!(policy.row_threshold(), Some(1000));
//         assert_eq!(policy.interval_seconds(), Some(300));
//         assert!(policy.is_automatic());

//         let policy = FlushPolicy::Manual;
//         assert!(!policy.is_automatic());
//     }
// }
