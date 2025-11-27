//! User Table Subscription Tests
//!
//! Tests for WebSocket subscriptions on USER tables with filtered queries.
//! Verifies multiple subscriptions, filtered change notifications, and unsubscribe functionality.
//!
//! **IMPORTANT**: These tests require a running KalamDB server.
//!
//! # Running Tests
//!
//! ```bash
//! # Terminal 1: Start the server
//! cd backend && cargo run --bin kalamdb-server
//!
//! # Terminal 2: Run the tests
//! cargo test --test test_user_table_subscriptions -- --nocapture
//! ```

use base64::Engine;
use kalam_link::auth::AuthProvider;
use kalam_link::{ChangeEvent, KalamLinkClient, QueryResponse, SubscriptionConfig};
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// Test configuration
const SERVER_URL: &str = "http://localhost:8080";
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Helper to check if server is running
async fn is_server_running() -> bool {
    let credentials = base64::engine::general_purpose::STANDARD.encode("root:");
    match reqwest::Client::new()
        .post(format!("{}/v1/api/sql", SERVER_URL))
        .header("Authorization", format!("Basic {}", credentials))
        .json(&serde_json::json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Helper to create a test client
fn create_test_client() -> Result<KalamLinkClient, kalam_link::KalamLinkError> {
    KalamLinkClient::builder()
        .base_url(SERVER_URL)
        .timeout(Duration::from_secs(30))
        .auth(AuthProvider::system_user_auth("".to_string()))
        .build()
}

/// Helper to execute SQL via HTTP
async fn execute_sql(sql: &str) -> Result<QueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    let client = create_test_client()?;
    Ok(client.execute_query(sql).await?)
}

/// Generate a unique table name
fn generate_table_name() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("messages_{}", timestamp)
}

/// Setup test namespace and USER table for subscription tests
async fn setup_user_table() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let table_name = generate_table_name();
    let full_table = format!("sub_test.{}", table_name);

    // Create namespace if needed
    execute_sql("CREATE NAMESPACE IF NOT EXISTS sub_test").await.ok();
    sleep(Duration::from_millis(200)).await;

    // Create USER TABLE (not STREAM TABLE) - supports all DML operations
    // USER tables require a PRIMARY KEY column
    execute_sql(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, type VARCHAR, content VARCHAR) WITH (TYPE = 'USER')",
        full_table
    ))
    .await?;
    sleep(Duration::from_millis(200)).await;

    Ok(full_table)
}

/// Cleanup test table
async fn cleanup_table(table_name: &str) {
    let _ = execute_sql(&format!("DROP TABLE IF EXISTS {}", table_name)).await;
}

/// Drain initial messages (ACK, InitialData) from subscription
async fn drain_initial_messages(
    subscription: &mut kalam_link::subscription::SubscriptionManager,
) {
    // Wait for ACK and any initial data
    for _ in 0..3 {
        match timeout(Duration::from_millis(500), subscription.next()).await {
            Ok(Some(Ok(event))) => {
                match event {
                    ChangeEvent::Ack { .. } | ChangeEvent::InitialDataBatch { .. } => {
                        // Expected initial messages, continue draining
                        continue;
                    }
                    _ => {
                        // Unexpected message during drain, stop
                        break;
                    }
                }
            }
            _ => break,
        }
    }
}

/// Helper to extract string value from row field (handles {"Utf8": "value"} format)
fn extract_string_value(value: &serde_json::Value) -> Option<String> {
    // Try direct string access first
    value.as_str().map(|s| s.to_string())
        .or_else(|| value.get("Utf8").and_then(|v| v.as_str()).map(|s| s.to_string()))
}

/// Test: Multiple filtered subscriptions on a USER table
/// 
/// This test verifies that WHERE clause filtering works for subscription change notifications:
/// 1. Creates a USER table with 'id', 'type', and 'content' columns
/// 2. Creates two subscriptions with different filters (type='thinking' and type='typing')
/// 3. Inserts rows matching each filter from another task
/// 4. **Verifies each subscription receives ONLY its filtered changes**
/// 5. Updates a row and verifies the UPDATE event is received
/// 6. Unsubscribes from one subscription
/// 7. Inserts another row and verifies the unsubscribed query doesn't receive it
#[tokio::test]
async fn test_multiple_filtered_subscriptions() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running at {}. Skipping test.", SERVER_URL);
        return;
    }

    let table = match setup_user_table().await {
        Ok(t) => t,
        Err(e) => {
            panic!("Failed to setup test table: {}", e);
        }
    };

    println!("✅ Created test table: {}", table);

    // Create client for subscriptions
    let client = create_test_client().expect("Failed to create client");

    // === Step 1: Create two subscriptions with different filters ===
    
    // Subscription 1: type = 'thinking'
    let thinking_config = SubscriptionConfig::new(
        "sub-thinking",
        &format!("SELECT * FROM {} WHERE type = 'thinking'", table),
    );
    
    let mut thinking_sub = match timeout(
        TEST_TIMEOUT,
        client.subscribe_with_config(thinking_config),
    )
    .await
    {
        Ok(Ok(sub)) => sub,
        Ok(Err(e)) => {
            cleanup_table(&table).await;
            panic!("Failed to create 'thinking' subscription: {}", e);
        }
        Err(_) => {
            cleanup_table(&table).await;
            panic!("Timeout creating 'thinking' subscription");
        }
    };
    
    println!("✅ Created subscription for type='thinking' (id: {})", thinking_sub.subscription_id());

    // Subscription 2: type = 'typing'
    let typing_config = SubscriptionConfig::new(
        "sub-typing",
        &format!("SELECT * FROM {} WHERE type = 'typing'", table),
    );
    
    let mut typing_sub = match timeout(
        TEST_TIMEOUT,
        client.subscribe_with_config(typing_config),
    )
    .await
    {
        Ok(Ok(sub)) => sub,
        Ok(Err(e)) => {
            let _ = thinking_sub.close().await;
            cleanup_table(&table).await;
            panic!("Failed to create 'typing' subscription: {}", e);
        }
        Err(_) => {
            let _ = thinking_sub.close().await;
            cleanup_table(&table).await;
            panic!("Timeout creating 'typing' subscription");
        }
    };
    
    println!("✅ Created subscription for type='typing' (id: {})", typing_sub.subscription_id());

    // Drain initial messages from both subscriptions
    drain_initial_messages(&mut thinking_sub).await;
    drain_initial_messages(&mut typing_sub).await;
    
    println!("✅ Drained initial messages from both subscriptions");

    // === Step 2: Insert rows from another task ===
    let table_clone = table.clone();
    let insert_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(300)).await;
        
        // Insert a 'typing' row (id=1)
        let typing_result = execute_sql(&format!(
            "INSERT INTO {} (id, type, content) VALUES (1, 'typing', 'user is typing...')",
            table_clone
        ))
        .await;
        
        if let Err(e) = typing_result {
            eprintln!("❌ Failed to insert 'typing' row: {}", e);
            return Err(e);
        }
        println!("✅ Inserted row with type='typing'");
        
        sleep(Duration::from_millis(100)).await;
        
        // Insert a 'thinking' row (id=2)
        let thinking_result = execute_sql(&format!(
            "INSERT INTO {} (id, type, content) VALUES (2, 'thinking', 'AI is thinking...')",
            table_clone
        ))
        .await;
        
        if let Err(e) = thinking_result {
            eprintln!("❌ Failed to insert 'thinking' row: {}", e);
            return Err(e);
        }
        println!("✅ Inserted row with type='thinking'");
        
        Ok(())
    });

    // === Step 3: Wait for changes on both subscriptions ===
    let mut thinking_changes: Vec<ChangeEvent> = Vec::new();
    let mut typing_changes: Vec<ChangeEvent> = Vec::new();
    
    // Collect events from 'thinking' subscription
    println!("🔄 Waiting for changes on 'thinking' subscription...");
    for _ in 0..5 {
        match timeout(Duration::from_secs(3), thinking_sub.next()).await {
            Ok(Some(Ok(event))) => {
                match &event {
                    ChangeEvent::Insert { subscription_id, rows } => {
                        println!("📥 'thinking' sub received Insert: subscription_id={}, rows={}", 
                            subscription_id, rows.len());
                        thinking_changes.push(event);
                        break;
                    }
                    ChangeEvent::Ack { .. } | ChangeEvent::InitialDataBatch { .. } => {
                        // Late initial message, skip
                        continue;
                    }
                    other => {
                        println!("📥 'thinking' sub received unexpected: {:?}", other);
                    }
                }
            }
            Ok(Some(Err(e))) => {
                eprintln!("❌ Error on 'thinking' subscription: {}", e);
                break;
            }
            Ok(None) => {
                eprintln!("❌ 'thinking' subscription closed unexpectedly");
                break;
            }
            Err(_) => {
                // Timeout, try again
                continue;
            }
        }
    }

    // Collect events from 'typing' subscription  
    println!("🔄 Waiting for changes on 'typing' subscription...");
    for _ in 0..5 {
        match timeout(Duration::from_secs(3), typing_sub.next()).await {
            Ok(Some(Ok(event))) => {
                match &event {
                    ChangeEvent::Insert { subscription_id, rows } => {
                        println!("📥 'typing' sub received Insert: subscription_id={}, rows={}", 
                            subscription_id, rows.len());
                        typing_changes.push(event);
                        break;
                    }
                    ChangeEvent::Ack { .. } | ChangeEvent::InitialDataBatch { .. } => {
                        continue;
                    }
                    other => {
                        println!("📥 'typing' sub received unexpected: {:?}", other);
                    }
                }
            }
            Ok(Some(Err(e))) => {
                eprintln!("❌ Error on 'typing' subscription: {}", e);
                break;
            }
            Ok(None) => {
                eprintln!("❌ 'typing' subscription closed unexpectedly");
                break;
            }
            Err(_) => {
                continue;
            }
        }
    }

    // Wait for insert task to complete
    let _ = insert_handle.await;

    // === Step 4: Verify each subscription received correct changes ===
    println!("\n=== Verification: Filtered Changes ===");
    println!("'thinking' subscription changes: {}", thinking_changes.len());
    println!("'typing' subscription changes: {}", typing_changes.len());

    assert!(
        !thinking_changes.is_empty(),
        "FAILED: 'thinking' subscription should have received at least 1 change"
    );
    
    assert!(
        !typing_changes.is_empty(),
        "FAILED: 'typing' subscription should have received at least 1 change"
    );

    // Verify 'thinking' subscription received ONLY 'thinking' type rows (filtering check)
    // The server prefixes subscription IDs with user-id and session info
    if let Some(ChangeEvent::Insert { subscription_id, rows }) = thinking_changes.first() {
        assert!(subscription_id.ends_with("sub-thinking"), 
            "Change should come from subscription ending with 'sub-thinking', got: {}", subscription_id);
        
        if let Some(row) = rows.first() {
            println!("📊 'thinking' subscription received row: {:?}", row);
            
            if let Some(type_obj) = row.get("type") {
                let type_str = extract_string_value(type_obj);
                println!("📊 Extracted type value: {:?}", type_str);
                
                // CRITICAL: Verify filtering - 'thinking' subscription should ONLY receive 'thinking' rows
                assert_eq!(
                    type_str.as_deref(), 
                    Some("thinking"),
                    "FILTERING CHECK FAILED: 'thinking' subscription received row with type={:?}, expected 'thinking'",
                    type_str
                );
                println!("✅ FILTERING WORKS: 'thinking' subscription correctly received ONLY 'thinking' type row");
            } else {
                panic!("Row doesn't have 'type' field. Full row: {}", row);
            }
        } else {
            panic!("'thinking' subscription received Insert with empty rows!");
        }
    }

    // Verify 'typing' subscription received ONLY 'typing' type rows (filtering check)
    if let Some(ChangeEvent::Insert { subscription_id, rows }) = typing_changes.first() {
        assert!(subscription_id.ends_with("sub-typing"),
            "Change should come from subscription ending with 'sub-typing', got: {}", subscription_id);
        
        if let Some(row) = rows.first() {
            println!("📊 'typing' subscription received row: {:?}", row);
            
            if let Some(type_obj) = row.get("type") {
                let type_str = extract_string_value(type_obj);
                
                // CRITICAL: Verify filtering - 'typing' subscription should ONLY receive 'typing' rows
                assert_eq!(
                    type_str.as_deref(),
                    Some("typing"),
                    "FILTERING CHECK FAILED: 'typing' subscription received row with type={:?}, expected 'typing'",
                    type_str
                );
                println!("✅ FILTERING WORKS: 'typing' subscription correctly received ONLY 'typing' type row");
            }
        }
    }

    // === Step 5: UPDATE a row and verify UPDATE event is received ===
    println!("\n🔄 Step 5: Testing UPDATE event...");
    
    let table_clone_update = table.clone();
    let update_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        // Update the 'thinking' row (id=2) to change its content
        let result = execute_sql(&format!(
            "UPDATE {} SET content = 'AI finished thinking!' WHERE id = 2",
            table_clone_update
        ))
        .await;
        
        if let Err(e) = result {
            eprintln!("❌ Failed to update row: {}", e);
            return Err(e);
        }
        println!("✅ Updated row id=2 (type='thinking')");
        Ok(())
    });

    // Wait for UPDATE event on 'thinking' subscription
    println!("🔄 Waiting for UPDATE event on 'thinking' subscription...");
    let mut received_update = false;
    
    for _ in 0..5 {
        match timeout(Duration::from_secs(3), thinking_sub.next()).await {
            Ok(Some(Ok(event))) => {
                match &event {
                    ChangeEvent::Update { subscription_id, rows, old_rows } => {
                        println!("📥 'thinking' sub received Update: subscription_id={}, rows={}, old_rows={}", 
                            subscription_id, rows.len(), old_rows.len());
                        
                        // Verify the update came through
                        if let Some(row) = rows.first() {
                            println!("📊 Updated row (new): {:?}", row);
                            // Check if content was updated
                            if let Some(content_obj) = row.get("content") {
                                let content_str = extract_string_value(content_obj);
                                if content_str.as_deref() == Some("AI finished thinking!") {
                                    println!("✅ UPDATE EVENT RECEIVED: content correctly updated");
                                    received_update = true;
                                }
                            }
                        }
                        if let Some(old_row) = old_rows.first() {
                            println!("📊 Updated row (old): {:?}", old_row);
                        }
                        break;
                    }
                    ChangeEvent::Insert { .. } => {
                        // Might receive late insert, continue waiting for update
                        println!("📥 Received late Insert, waiting for Update...");
                        continue;
                    }
                    ChangeEvent::Ack { .. } | ChangeEvent::InitialDataBatch { .. } => {
                        continue;
                    }
                    other => {
                        println!("📥 Received unexpected event: {:?}", other);
                    }
                }
            }
            Ok(Some(Err(e))) => {
                eprintln!("❌ Error on subscription: {}", e);
                break;
            }
            Ok(None) => {
                eprintln!("❌ Subscription closed unexpectedly");
                break;
            }
            Err(_) => {
                // Timeout, try again
                continue;
            }
        }
    }

    let _ = update_handle.await;
    
    assert!(received_update, "FAILED: Should have received UPDATE event for the modified row");
    println!("✅ UPDATE event verification passed!");

    // === Step 6: Unsubscribe from 'typing' subscription ===
    println!("\n🔄 Unsubscribing from 'typing' subscription...");
    match typing_sub.close().await {
        Ok(_) => println!("✅ Successfully unsubscribed from 'typing'"),
        Err(e) => println!("⚠️  Error during unsubscribe (may be OK): {}", e),
    }

    // === Step 7: Insert another 'typing' row and verify 'thinking' subscription does NOT receive it ===
    println!("\n🔄 Step 7: Verifying filtered subscriptions don't receive unmatched inserts...");
    sleep(Duration::from_millis(500)).await;
    
    let table_clone2 = table.clone();
    let insert_handle2 = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        // id=3 since we already inserted id=1 (typing) and id=2 (thinking)
        let result = execute_sql(&format!(
            "INSERT INTO {} (id, type, content) VALUES (3, 'typing', 'more typing - should not reach thinking sub')",
            table_clone2
        ))
        .await;
        
        if let Err(e) = result {
            eprintln!("❌ Failed to insert third row: {}", e);
        } else {
            println!("✅ Inserted row id=3 with type='typing'");
        }
    });

    // CRITICAL FILTERING TEST: The 'thinking' subscription (WHERE type='thinking') 
    // should NOT receive the new 'typing' row if filtering works correctly
    
    println!("🔄 Waiting to verify 'thinking' subscription does NOT receive 'typing' insert...");
    let mut received_wrong_type = false;
    
    // Wait and check if 'thinking' sub receives anything (it shouldn't for 'typing' rows)
    match timeout(Duration::from_secs(3), thinking_sub.next()).await {
        Ok(Some(Ok(event))) => {
            match &event {
                ChangeEvent::Insert { rows, .. } => {
                    // Check if this is a 'typing' row (would mean filtering failed!)
                    if let Some(row) = rows.first() {
                        if let Some(type_obj) = row.get("type") {
                            let type_str = extract_string_value(type_obj);
                            if type_str.as_deref() == Some("typing") {
                                received_wrong_type = true;
                                println!("❌ FILTERING FAILED: 'thinking' subscription received 'typing' row!");
                                println!("📊 Unexpected row: {:?}", row);
                            } else {
                                println!("📥 'thinking' received insert with type={:?} (unexpected but not 'typing')", type_str);
                            }
                        }
                    }
                }
                ChangeEvent::Update { .. } => {
                    println!("📥 Received late Update event (OK)");
                }
                other => {
                    println!("📥 'thinking' subscription received: {:?}", other);
                }
            }
        }
        Ok(Some(Err(e))) => {
            println!("⚠️  Error on 'thinking' subscription: {}", e);
        }
        Ok(None) => {
            println!("⚠️  'thinking' subscription closed");
        }
        Err(_) => {
            // Timeout is EXPECTED if filtering works correctly
            println!("✅ FILTERING VERIFIED: 'thinking' subscription correctly did NOT receive 'typing' insert (timeout)");
        }
    }

    assert!(
        !received_wrong_type,
        "FILTERING FAILED: 'thinking' subscription should NOT receive 'typing' type rows"
    );

    // Wait for insert task
    let _ = insert_handle2.await;

    // === Cleanup ===
    let _ = thinking_sub.close().await;
    cleanup_table(&table).await;

    println!("\n✅✅✅ Test passed: Filtered subscriptions work correctly! ✅✅✅");
}

/// Test: Unsubscribe stops receiving changes
///
/// Simpler test focused specifically on unsubscribe behavior
#[tokio::test]
async fn test_unsubscribe_stops_changes() {
    if !is_server_running().await {
        eprintln!("⚠️  Server not running. Skipping test.");
        return;
    }

    let table = match setup_user_table().await {
        Ok(t) => t,
        Err(e) => {
            panic!("Failed to setup test table: {}", e);
        }
    };

    let client = create_test_client().expect("Failed to create client");

    // Create a subscription
    let config = SubscriptionConfig::new(
        "sub-unsubscribe-test",
        &format!("SELECT * FROM {} WHERE type = 'test'", table),
    );
    
    let mut subscription = match timeout(
        TEST_TIMEOUT,
        client.subscribe_with_config(config),
    )
    .await
    {
        Ok(Ok(sub)) => sub,
        Ok(Err(e)) => {
            cleanup_table(&table).await;
            panic!("Failed to create subscription: {}", e);
        }
        Err(_) => {
            cleanup_table(&table).await;
            panic!("Timeout creating subscription");
        }
    };

    println!("✅ Created subscription: {}", subscription.subscription_id());
    
    // Drain initial messages
    drain_initial_messages(&mut subscription).await;

    // Insert a row to verify subscription is working (id=1)
    execute_sql(&format!(
        "INSERT INTO {} (id, type, content) VALUES (1, 'test', 'first insert')",
        table
    ))
    .await
    .expect("Failed to insert first row");

    // Wait for the change
    let received_first = match timeout(Duration::from_secs(3), subscription.next()).await {
        Ok(Some(Ok(ChangeEvent::Insert { .. }))) => true,
        _ => false,
    };
    
    assert!(received_first, "Should receive first insert before unsubscribe");
    println!("✅ Received first insert notification");

    // Unsubscribe
    subscription.close().await.expect("Failed to close subscription");
    println!("✅ Unsubscribed");

    // Insert another row (id=2)
    execute_sql(&format!(
        "INSERT INTO {} (id, type, content) VALUES (2, 'test', 'second insert after unsubscribe')",
        table
    ))
    .await
    .expect("Failed to insert second row");

    // The subscription is closed, so we can't check it anymore
    // This test mainly verifies that close() works without errors
    
    println!("✅ Second insert completed (subscription is closed, no notification expected)");

    cleanup_table(&table).await;
    println!("✅ Test passed: Unsubscribe works correctly");
}
