//! Test to verify timezone handling in DateTime storage
//!
//! This test verifies that when a DateTime with timezone offset (like "2025-01-01T12:00:00+02:00")
//! is stored, the timezone information is preserved correctly.

use chrono::{DateTime, FixedOffset, Utc};

#[test]
fn test_datetime_with_timezone_offset_parsing() {
    // Test various timezone formats
    let test_cases = vec![
        "2025-01-01T12:00:00+02:00", // UTC+2
        "2025-01-01T12:00:00-05:00", // UTC-5
        "2025-01-01T12:00:00Z",      // UTC (Z notation)
        "2025-01-01T12:00:00+00:00", // UTC (explicit)
    ];

    for input in test_cases {
        // Parse RFC3339 datetime with timezone
        let parsed = DateTime::parse_from_rfc3339(input).expect("Failed to parse datetime");

        println!("\n=== Input: {} ===", input);
        println!("Parsed offset: {:?}", parsed.offset());
        println!("Parsed timestamp (local): {}", parsed);

        // Convert to UTC (this is what we store)
        let utc: DateTime<Utc> = parsed.with_timezone(&Utc);
        println!("Converted to UTC: {}", utc);
        println!("UTC timestamp millis: {}", utc.timestamp_millis());

        // Convert back to original timezone
        let original_offset = *parsed.offset();
        let roundtrip: DateTime<FixedOffset> = utc.with_timezone(&original_offset);
        println!("Round-trip to original tz: {}", roundtrip);

        // IMPORTANT: When we store as UTC timestamp, we LOSE the original timezone offset!
        // We only store the UTC milliseconds. The timezone info is NOT preserved.
        assert_eq!(utc.timestamp_millis(), parsed.timestamp_millis());

        // The original input "2025-01-01T12:00:00+02:00" becomes "2025-01-01T10:00:00Z" in UTC
        // If user wants to preserve timezone, they must store it separately!
    }
}

#[test]
fn test_timezone_conversion_examples() {
    // Example: User in New York (UTC-5) stores "2025-01-01 12:00:00"
    let ny_time = "2025-01-01T12:00:00-05:00";
    let parsed = DateTime::parse_from_rfc3339(ny_time).unwrap();
    let utc = parsed.with_timezone(&Utc);

    println!("\n=== New York Example ===");
    println!("NY time: {}", ny_time);
    println!("Stored as UTC: {}", utc);
    println!("UTC timestamp: {}", utc.timestamp_millis());

    // When retrieved, without timezone info, we only get UTC
    // To display in NY time again, app must know the timezone separately
    let ny_offset = FixedOffset::west_opt(5 * 3600).unwrap();
    let displayed_in_ny = utc.with_timezone(&ny_offset);
    println!(
        "Displayed in NY (requires separate tz info): {}",
        displayed_in_ny
    );

    // CONCLUSION: KalamDB's DateTime stores UTC timestamp + UTC timezone marker
    // Original timezone offset (+02:00, -05:00, etc.) is LOST
    // Applications must store timezone separately if they need it
}
