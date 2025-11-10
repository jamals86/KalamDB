#!/bin/bash
#Test script to verify TTL filtering logic

# 1. Start server in background
pkill -f kalamdb-server
sleep 1
cd /Users/jamal/git/KalamDB
RUST_LOG=debug cargo run --bin kalamdb-server > server.log 2>&1 &
SERVER_PID=$!
echo "Started server with PID: $SERVER_PID"
sleep 5

# 2. Run just the first part of the test (create table + insert)
cd cli
cat > /tmp/test_ttl.rs << 'EOF'
use std::process::Command;

fn main() {
    let server_url = "http://localhost:8000";
    
    // Create namespace
    let ns = format!("ttl_test_{}", chrono::Utc::now().timestamp());
    let create_ns = format!("CREATE NAMESPACE IF NOT EXISTS {}", ns);
    
    let output = Command::new("cargo")
        .args(&["run", "--", "exec", &create_ns, "--username", "root", "--password", "password"])
        .output()
        .expect("Failed to execute");
    
    println!("Create namespace: {}", String::from_utf8_lossy(&output.stdout));
    
    // Create stream table with 30s TTL
    let table = "test_stream";
    let full = format!("{}.{}", ns, table);
    let create_table = format!(
        "CREATE STREAM TABLE {} (event_id TEXT NOT NULL, payload TEXT) TTL 30",
        full
    );
    
    let output = Command::new("cargo")
        .args(&["run", "--", "exec", &create_table, "--username", "root", "--password", "password"])
        .output()
        .expect("Failed to execute");
        
    println!("Create table: {}", String::from_utf8_lossy(&output.stdout));
    
    // Insert data
    let insert = format!("INSERT INTO {} (event_id, payload) VALUES ('e1', 'test_data')", full);
    let output = Command::new("cargo")
        .args(&["run", "--", "exec", &insert, "--username", "root", "--password", "password"])
        .output()
        .expect("Failed to execute");
        
    println!("Insert: {}", String::from_utf8_lossy(&output.stdout));
    
    // SELECT immediately
    let select = format!("SELECT * FROM {}", full);
    let output = Command::new("cargo")
        .args(&["run", "--", "exec", &select, "--username", "root", "--password", "password"])
        .output()
        .expect("Failed to execute");
        
    println!("SELECT (immediate): {}", String::from_utf8_lossy(&output.stdout));
    
    // Wait 31 seconds
    println!("Waiting 31 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(31));
    
    // SELECT after TTL
    let output = Command::new("cargo")
        .args(&["run", "--", "exec", &select, "--username", "root", "--password", "password"])
        .output()
        .expect("Failed to execute");
        
    println!("SELECT (after 31s): {}", String::from_utf8_lossy(&output.stdout));
}
EOF

# Run the test
rustc /tmp/test_ttl.rs --edition 2021 -o /tmp/test_ttl
/tmp/test_ttl

# Cleanup
kill $SERVER_PID
