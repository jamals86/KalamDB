//! Time helpers for consumer logic.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_nanos() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}
