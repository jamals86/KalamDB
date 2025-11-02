// Smoke Test 1 (revised): User table with subscription lifecycle
// Covers: namespace creation, user table creation, inserts, subscription receiving events, flush job visibility

use crate::common::*;

#[test]
fn smoke_user_table_subscription_lifecycle() {
    if !is_server_running() {
        println!(
            "Skipping smoke_user_table_subscription_lifecycle: server not running at {}",
            SERVER_URL
        );
        return;
    }

    // Unique per run
    let namespace = generate_unique_namespace("smoke_ns");
    let table = generate_unique_table("user_smoke");
    let full = format!("{}.{}", namespace, table);

    // 1) Create namespace
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    execute_sql_as_root_via_cli(&ns_sql).expect("create namespace should succeed");

    // 2) Create user table
    let create_sql = format!(
        r#"CREATE USER TABLE {} (
            id INT AUTO_INCREMENT,
            name VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        ) FLUSH ROWS 10"#,
        full
    );
    execute_sql_as_root_via_cli(&create_sql).expect("create user table should succeed");

    // 3) Insert a couple rows
    let ins1 = format!("INSERT INTO {} (name) VALUES ('alpha')", full);
    let ins2 = format!("INSERT INTO {} (name) VALUES ('beta')", full);
    execute_sql_as_root_via_cli(&ins1).expect("insert alpha should succeed");
    execute_sql_as_root_via_cli(&ins2).expect("insert beta should succeed");

    // Quick verification via SELECT
    let sel = format!("SELECT * FROM {}", full);
    let out = execute_sql_as_root_via_cli(&sel).expect("select should succeed");
    assert!(out.contains("alpha"), "expected to see 'alpha' in select output: {}", out);
    assert!(out.contains("beta"), "expected to see 'beta' in select output: {}", out);

    // Small delay to ensure data is visible to subscription queries
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Double-check data is visible right before subscribing
    let verify_sel = format!("SELECT COUNT(*) as cnt FROM {}", full);
    let count_out = execute_sql_as_root_via_cli(&verify_sel).expect("count query should succeed");
    println!("[DEBUG] Row count before subscribing: {}", count_out);

    // 4) Subscribe to the user table
    let query = format!("SELECT * FROM {}", full);
    let mut listener = SubscriptionListener::start(&query).expect("subscription should start");

    // 4a) Collect snapshot rows immediately after subscription starts
    let mut snapshot_lines: Vec<String> = Vec::new();
    let snapshot_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < snapshot_deadline {
        match listener.try_read_line(std::time::Duration::from_millis(200)) {
            Ok(Some(line)) => {
                if !line.trim().is_empty() {
                    println!("[subscription][snapshot] {}", line);
                    snapshot_lines.push(line);
                }
            }
            Ok(None) => break, // EOF
            Err(_) => continue, // timeout, keep polling until deadline
        }
    }
    // Hard requirement: snapshot must include initial rows
    let snapshot_joined = snapshot_lines.join("\n");
    assert!(snapshot_joined.contains("alpha"), "snapshot should contain 'alpha' but was: {}", snapshot_joined);
    assert!(snapshot_joined.contains("beta"), "snapshot should contain 'beta' but was: {}", snapshot_joined);

    // 5) Insert a new row and verify subscription outputs the change event
    let sub_val = "from_subscription";
    let ins3 = format!("INSERT INTO {} (name) VALUES ('{}')", full, sub_val);
    execute_sql_as_root_via_cli(&ins3).expect("insert sub row should succeed");

    let mut change_lines: Vec<String> = Vec::new();
    let change_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < change_deadline {
        match listener.try_read_line(std::time::Duration::from_millis(250)) {
            Ok(Some(line)) => {
                if !line.trim().is_empty() {
                    println!("[subscription][change] {}", line);
                    let is_match = line.contains(sub_val);
                    change_lines.push(line);
                    if is_match {
                        break;
                    }
                }
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    let changes_joined = change_lines.join("\n");
    assert!(changes_joined.contains(sub_val), "expected change event containing '{}' within 5s; got: {}", sub_val, changes_joined);

    // 6) Flush the user table and verify job completes successfully
    let flush_sql = format!("FLUSH TABLE {}", full);
    let flush_output = execute_sql_as_root_via_cli(&flush_sql)
        .expect("flush should succeed for user table");
    
    println!("[FLUSH] Output: {}", flush_output);
    
    // Parse job ID from flush output
    let job_id = parse_job_id_from_flush_output(&flush_output)
        .expect("should parse job_id from FLUSH output");
    
    println!("[FLUSH] Job ID: {}", job_id);
    
    // Verify the job completes successfully (10 second timeout)
    verify_job_completed(&job_id, std::time::Duration::from_secs(10))
        .expect("flush job should complete successfully");
    
    println!("[FLUSH] Job {} completed successfully", job_id);

    // Stop subscription
    listener.stop().ok();
}
