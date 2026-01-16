/// Time bucket granularity for log layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTimeBucket {
    Hour,
    Day,
    Week,
    Month,
}

impl StreamTimeBucket {
    pub fn duration_ms(&self) -> u64 {
        match self {
            StreamTimeBucket::Hour => 60 * 60 * 1000,
            StreamTimeBucket::Day => 24 * 60 * 60 * 1000,
            StreamTimeBucket::Week => 7 * 24 * 60 * 60 * 1000,
            StreamTimeBucket::Month => 31 * 24 * 60 * 60 * 1000,
        }
    }
}

/// Resolve bucket granularity from TTL (seconds).
pub fn bucket_for_ttl(ttl_seconds: u64) -> StreamTimeBucket {
    const HOUR: u64 = 60 * 60;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 30 * DAY;

    if ttl_seconds <= DAY {
        StreamTimeBucket::Hour
    } else if ttl_seconds <= WEEK {
        StreamTimeBucket::Day
    } else if ttl_seconds <= MONTH {
        StreamTimeBucket::Week
    } else {
        StreamTimeBucket::Month
    }
}
