// Smoke Test 4: Stream table subscription
// Covers: namespace creation, stream table creation with TTL, subscription, insert, receive event

use crate::common::*;

#[test]
fn smoke_stream_table_subscription() {
    if !is_server_running() {
        println!(
            "Skipping smoke_stream_table_subscription: server not running at {}",
            SERVER_URL
        );
        return;
    }

    // Unique per run
    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("stream_smoke");
    let full = format!("{}.{}", namespace, table);

    // 1) Create namespace
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    execute_sql_as_root_via_cli(&ns_sql).expect("create namespace should succeed");

    // 2) Create stream table with small TTL
    let create_sql = format!(
        r#"CREATE STREAM TABLE {} (
            event_id TEXT NOT NULL,
            event_type TEXT,
            payload TEXT,
            timestamp TIMESTAMP
        ) TTL 10"#,
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create stream table should succeed");

    // 3) Subscribe to the stream table
    let query = format!("SELECT * FROM {}", full);
    let mut listener = SubscriptionListener::start(&query).expect("subscription should start");

    // 4) Insert a stream event and expect subscription output
    let ev_val = "smoke_stream_event";
    let mut got_any = false;
    let mut attempt = 0;
    while attempt < 5 && !got_any {
        attempt += 1;
        let event_id = format!("e{}", attempt);
        let ins = format!(
            "INSERT INTO {} (event_id, event_type, payload) VALUES ('{}', 'info', '{}')",
            full, event_id, ev_val
        );
        execute_sql_as_root_via_cli(&ins).expect("insert stream event should succeed");

        // After each insert, poll for up to 1s for a subscription line
        let per_attempt_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < per_attempt_deadline {
            match listener.try_read_line(std::time::Duration::from_millis(250)) {
                Ok(Some(line)) => {
                    if !line.trim().is_empty() {
                        got_any = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => continue,
            }
        }
    }
    assert!(got_any, "expected to receive some subscription output within retry window");

    // Stop subscription
    listener.stop().ok();
}
