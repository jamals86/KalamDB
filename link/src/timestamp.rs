//! Timestamp formatting utilities for KalamDB.
//!
//! Provides configurable formatting of millisecond timestamps (i64) from KalamDB
//! to various human-readable formats. This module is exposed via WASM bindings
//! for use in all language SDKs (TypeScript, Python, etc.).

use chrono::{DateTime, Utc, TimeZone, Datelike};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Timestamp format options.
///
/// Controls how timestamps are displayed in query results and subscriptions.
/// Default format is ISO 8601 with milliseconds (`2024-12-14T15:30:45.123Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TimestampFormat {
    /// ISO 8601 with milliseconds: `2024-12-14T15:30:45.123Z`
    /// This is the default and most widely compatible format.
    #[serde(rename = "iso8601")]
    #[default]
    Iso8601,

    /// ISO 8601 date only: `2024-12-14`
    #[serde(rename = "iso8601-date")]
    Iso8601Date,

    /// ISO 8601 without milliseconds: `2024-12-14T15:30:45Z`
    #[serde(rename = "iso8601-datetime")]
    Iso8601DateTime,

    /// Unix timestamp in milliseconds: `1734211234567`
    #[serde(rename = "unix-ms")]
    UnixMs,

    /// Unix timestamp in seconds: `1734211234`
    #[serde(rename = "unix-sec")]
    UnixSec,

    /// Relative time: `2 hours ago`, `in 5 minutes`
    #[serde(rename = "relative")]
    Relative,

    /// RFC 2822 format: `Fri, 14 Dec 2024 15:30:45 +0000`
    #[serde(rename = "rfc2822")]
    Rfc2822,

    /// RFC 3339 format (same as ISO 8601): `2024-12-14T15:30:45.123+00:00`
    #[serde(rename = "rfc3339")]
    Rfc3339,
}


impl fmt::Display for TimestampFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iso8601 => write!(f, "iso8601"),
            Self::Iso8601Date => write!(f, "iso8601-date"),
            Self::Iso8601DateTime => write!(f, "iso8601-datetime"),
            Self::UnixMs => write!(f, "unix-ms"),
            Self::UnixSec => write!(f, "unix-sec"),
            Self::Relative => write!(f, "relative"),
            Self::Rfc2822 => write!(f, "rfc2822"),
            Self::Rfc3339 => write!(f, "rfc3339"),
        }
    }
}

/// Configuration for timestamp formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampFormatterConfig {
    /// Format to use for timestamps
    #[serde(default)]
    pub format: TimestampFormat,
}

impl Default for TimestampFormatterConfig {
    fn default() -> Self {
        Self {
            format: TimestampFormat::Iso8601,
        }
    }
}

/// Timestamp formatter for converting millisecond timestamps to strings.
///
/// # Examples
///
/// ```rust
/// use kalam_link::timestamp::{TimestampFormatter, TimestampFormat};
///
/// let formatter = TimestampFormatter::new(TimestampFormat::Iso8601);
/// let formatted = formatter.format(1734211234567);
/// assert_eq!(formatted, "2024-12-14T19:13:54.567Z");
/// ```
#[derive(Debug, Clone)]
pub struct TimestampFormatter {
    format: TimestampFormat,
}

impl TimestampFormatter {
    /// Create a new timestamp formatter with the specified format.
    pub fn new(format: TimestampFormat) -> Self {
        Self { format }
    }

    /// Create a formatter from configuration.
    pub fn from_config(config: TimestampFormatterConfig) -> Self {
        Self::new(config.format)
    }

    /// Format a millisecond timestamp according to the configured format.
    ///
    /// Returns "NULL" for None values.
    pub fn format(&self, ms: Option<i64>) -> String {
        match ms {
            None => "NULL".to_string(),
            Some(ms) => self.format_value(ms),
        }
    }

    /// Format a non-null millisecond timestamp.
    fn format_value(&self, ms: i64) -> String {
        match self.format {
            TimestampFormat::Iso8601 => self.format_iso8601(ms),
            TimestampFormat::Iso8601Date => self.format_iso8601_date(ms),
            TimestampFormat::Iso8601DateTime => self.format_iso8601_datetime(ms),
            TimestampFormat::UnixMs => ms.to_string(),
            TimestampFormat::UnixSec => (ms / 1000).to_string(),
            TimestampFormat::Relative => self.format_relative(ms),
            TimestampFormat::Rfc2822 => self.format_rfc2822(ms),
            TimestampFormat::Rfc3339 => self.format_rfc3339(ms),
        }
    }

    /// Format as ISO 8601 with milliseconds: `2024-12-14T15:30:45.123Z`
    fn format_iso8601(&self, ms: i64) -> String {
        match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            None => format!("Invalid timestamp: {}", ms),
        }
    }

    /// Format as ISO 8601 date only: `2024-12-14`
    fn format_iso8601_date(&self, ms: i64) -> String {
        match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day()),
            None => format!("Invalid timestamp: {}", ms),
        }
    }

    /// Format as ISO 8601 without milliseconds: `2024-12-14T15:30:45Z`
    fn format_iso8601_datetime(&self, ms: i64) -> String {
        match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            None => format!("Invalid timestamp: {}", ms),
        }
    }

    /// Format as RFC 2822: `Fri, 14 Dec 2024 15:30:45 +0000`
    fn format_rfc2822(&self, ms: i64) -> String {
        match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => dt.to_rfc2822(),
            None => format!("Invalid timestamp: {}", ms),
        }
    }

    /// Format as RFC 3339: `2024-12-14T15:30:45.123+00:00`
    fn format_rfc3339(&self, ms: i64) -> String {
        match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
            None => format!("Invalid timestamp: {}", ms),
        }
    }

    /// Format as relative time: `2 hours ago`, `in 5 minutes`
    pub fn format_relative(&self, ms: i64) -> String {
        let now = Utc::now().timestamp_millis();
        let diff_ms = now - ms;
        let abs_diff = diff_ms.abs();
        let is_future = diff_ms < 0;

        let seconds = abs_diff / 1000;
        let minutes = seconds / 60;
        let hours = minutes / 60;
        let days = hours / 24;
        let weeks = days / 7;
        let months = days / 30;
        let years = days / 365;

        let (value, unit) = if years > 0 {
            (years, if years == 1 { "year" } else { "years" })
        } else if months > 0 {
            (months, if months == 1 { "month" } else { "months" })
        } else if weeks > 0 {
            (weeks, if weeks == 1 { "week" } else { "weeks" })
        } else if days > 0 {
            (days, if days == 1 { "day" } else { "days" })
        } else if hours > 0 {
            (hours, if hours == 1 { "hour" } else { "hours" })
        } else if minutes > 0 {
            (minutes, if minutes == 1 { "minute" } else { "minutes" })
        } else {
            (seconds, if seconds == 1 { "second" } else { "seconds" })
        };

        if is_future {
            format!("in {} {}", value, unit)
        } else {
            format!("{} {} ago", value, unit)
        }
    }

    /// Format multiple timestamps.
    pub fn format_many(&self, timestamps: &[Option<i64>]) -> Vec<String> {
        timestamps.iter().map(|ts| self.format(*ts)).collect()
    }

    /// Change the format.
    pub fn set_format(&mut self, format: TimestampFormat) {
        self.format = format;
    }

    /// Get current format.
    pub fn get_format(&self) -> TimestampFormat {
        self.format
    }
}

impl Default for TimestampFormatter {
    fn default() -> Self {
        Self::new(TimestampFormat::Iso8601)
    }
}

/// Parse ISO 8601 string back to milliseconds.
///
/// # Examples
///
/// ```rust
/// use kalam_link::timestamp::parse_iso8601;
///
/// let ms = parse_iso8601("2024-12-14T15:30:45.123Z").unwrap();
/// assert_eq!(ms, 1734191445123);
/// ```
pub fn parse_iso8601(iso: &str) -> Result<i64, chrono::ParseError> {
    let dt = DateTime::parse_from_rfc3339(iso)?;
    Ok(dt.timestamp_millis())
}

/// Get current timestamp in milliseconds (compatible with KalamDB).
///
/// # Examples
///
/// ```rust
/// use kalam_link::timestamp::now;
///
/// let current_ms = now();
/// assert!(current_ms > 1700000000000); // After Nov 2023
/// ```
pub fn now() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_iso8601() {
        let formatter = TimestampFormatter::new(TimestampFormat::Iso8601);
        let result = formatter.format(Some(1734191445123));
        assert!(result.contains("2024-12-14"));
        assert!(result.ends_with("Z"));
    }

    #[test]
    fn test_format_iso8601_date() {
        let formatter = TimestampFormatter::new(TimestampFormat::Iso8601Date);
        let result = formatter.format(Some(1734191445123));
        assert_eq!(result, "2024-12-14");
    }

    #[test]
    fn test_format_unix_ms() {
        let formatter = TimestampFormatter::new(TimestampFormat::UnixMs);
        let result = formatter.format(Some(1734191445123));
        assert_eq!(result, "1734191445123");
    }

    #[test]
    fn test_format_unix_sec() {
        let formatter = TimestampFormatter::new(TimestampFormat::UnixSec);
        let result = formatter.format(Some(1734191445123));
        assert_eq!(result, "1734191445");
    }

    #[test]
    fn test_format_null() {
        let formatter = TimestampFormatter::new(TimestampFormat::Iso8601);
        let result = formatter.format(None);
        assert_eq!(result, "NULL");
    }

    #[test]
    fn test_format_relative() {
        let formatter = TimestampFormatter::new(TimestampFormat::Relative);
        let two_hours_ago = now() - (2 * 60 * 60 * 1000);
        let result = formatter.format(Some(two_hours_ago));
        assert!(result.contains("2 hours ago") || result.contains("1 hour ago"));
    }

    #[test]
    fn test_parse_iso8601() {
        let ms = parse_iso8601("2024-12-14T15:30:45.123Z").unwrap();
        assert!(ms > 1700000000000);
    }

    #[test]
    fn test_now() {
        let ms = now();
        assert!(ms > 1700000000000);
    }
}
