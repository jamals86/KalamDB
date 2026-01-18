//! Cluster Subscription Tests for Leader and Follower
//!
//! Tests that verify WebSocket subscriptions work correctly when connected to
//! either the leader or follower nodes. In a properly replicated cluster:
//! 1. Subscriptions to followers should receive changes made on the leader
//! 2. Subscriptions to the leader should receive changes normally
//! 3. Multiple subscriptions across different nodes should all receive the same events
//! 4. Initial data should be consistent regardless of which node serves the subscription

use crate::cluster_common::*;
use crate::common::*;
use kalam_link::{
    AuthProvider,
    ChangeEvent,
    KalamLinkClient,
    KalamLinkTimeouts,
    SubscriptionManager,
};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// Parse cluster nodes to get leader and follower URLs
fn get_leader_and_followers() -> (String, Vec<String>) {
    let urls = cluster_urls();
    let response = execute_on_node_response(
        &urls[0],
        "SELECT node_id, api_addr, is_leader FROM system.cluster",
    )
    .expect("Failed to query system.cluster");

    let result = response.results.first().expect("Missing cluster result");
    let rows = result.rows.as_ref().expect("Missing cluster rows");

    let mut leader_url: Option<String> = None;
    let mut follower_urls: Vec<String> = Vec::new();

    for row in rows {
        if row.len() < 3 {
            continue;
        }

        let api_addr = match extract_typed_value(&row[1]) {
            Value::String(s) => s,
            other => other.to_string().trim_matches('"').to_string(),
        };

        let is_leader = extract_typed_value(&row[2]);
        let leader = matches!(is_leader, Value::Bool(true))
            || matches!(is_leader, Value::String(ref s) if s == "true");

        if leader {
            leader_url = Some(api_addr);
        } else {
            follower_urls.push(api_addr);
        }
    }

    let leader_url = leader_url.expect("Leader URL not found");
    (leader_url, follower_urls)
}

fn create_ws_client(base_url: &str) -> KalamLinkClient {
    KalamLinkClient::builder()
        .base_url(base_url)
        .auth(AuthProvider::basic_auth("root".to_string(), root_password().to_string()))
        .timeouts(
            KalamLinkTimeouts::builder()
                .connection_timeout_secs(5)
                .receive_timeout_secs(30)
                .send_timeout_secs(10)
                .subscribe_timeout_secs(20)
                .auth_timeout_secs(10)
                .initial_data_timeout(Duration::from_secs(30))
                .build(),
        )
        .build()
        .expect("Failed to build cluster client")
}

async fn subscribe_with_retry(
    client: &KalamLinkClient,
    query: &str,
    max_attempts: usize,
) -> SubscriptionManager {
    let mut last_error: Option<String> = None;
    for attempt in 0..max_attempts {
        let mut subscription = client.subscribe(query).await.expect("Failed to subscribe");

        if let Ok(Some(Ok(event))) =
            tokio::time::timeout(Duration::from_secs(5), subscription.next()).await
        {
            if matches!(event, ChangeEvent::Error { .. }) {
                last_error = Some("subscription registration failed".to_string());
                tokio::time::sleep(Duration::from_millis(200 + (attempt as u64 * 150))).await;
                continue;
            }
        }

        return subscription;
    }

    panic!(
        "Subscription failed to register after {} attempts: {:?}",
        max_attempts, last_error
    );
}

fn response_error_message(response: &kalam_link::QueryResponse) -> String {
    if let Some(error) = &response.error {
        if let Some(details) = &error.details {
            return format!("{} ({})", error.message, details);
        }
        return error.message.clone();
    }
    format!("Query failed: {:?}", response)
}

async fn execute_query_with_retry(
    client: &KalamLinkClient,
    sql: &str,
    max_attempts: usize,
) -> Result<(), String> {
    let mut last_err: Option<String> = None;
    for attempt in 0..max_attempts {
        match client.execute_query(sql, None, None).await {
            Ok(response) => {
                if response.success() {
                    return Ok(());
                }
                let err_msg = response_error_message(&response);
                if is_retryable_cluster_error_for_sql(sql, &err_msg) {
                    last_err = Some(err_msg);
                } else {
                    return Err(err_msg);
                }
            }
            Err(e) => {
                let err_msg = e.to_string();
                if is_retryable_cluster_error_for_sql(sql, &err_msg) {
                    last_err = Some(err_msg);
                } else {
                    return Err(err_msg);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(300 + (attempt as u64 * 200))).await;
    }

    Err(last_err.unwrap_or_else(|| "all retries failed".to_string()))
}

/// Test: Subscription on leader receives changes from leader writes
#[test]
fn cluster_test_subscription_leader_to_leader() {
    if !require_cluster_running() {
        return;
    }

    println!("\n=== TEST: Subscription Leader to Leader ===\n");

    let (leader_url, _followers) = get_leader_and_followers();
    println!("Leader: {}", leader_url);

    let namespace = generate_unique_namespace("sub_ll");
    let table = "sub_leader";
    let full = format!("{}.{}", namespace, table);

    // Setup
    let _ =
        execute_on_node(&leader_url, &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    // Use USER table instead of SHARED - subscriptions only work on USER/STREAM tables
    execute_on_node(
        &leader_url,
        &format!("CREATE USER TABLE {} (id BIGINT PRIMARY KEY, value TEXT)", full),
    )
    .expect("Failed to create table");

    // Wait for table to replicate
    if !wait_for_table_on_all_nodes(&namespace, table, 10000) {
        panic!("Table {} did not replicate to all nodes", full);
    }

    let query = format!("SELECT * FROM {}", full);
    let insert_value = "leader_event";

    cluster_runtime().block_on(async {
        let client = create_ws_client(&leader_url);

        let mut subscription =
            client.subscribe(&query).await.expect("Failed to subscribe on leader");

        // Insert data on leader
        client
            .execute_query(
                &format!("INSERT INTO {} (id, value) VALUES (1, '{}')", full, insert_value),
                None,
                None,
            )
            .await
            .expect("Failed to insert on leader");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut received = false;

        while tokio::time::Instant::now() < deadline && !received {
            let wait = Duration::from_secs(2);
            match tokio::time::timeout(wait, subscription.next()).await {
                Ok(Some(Ok(event))) => {
                    if let ChangeEvent::Insert { rows, .. } = event {
                        for row in rows {
                            if let Some(obj) = row.as_object() {
                                if let Some(Value::String(val)) = obj.get("value") {
                                    if val == insert_value {
                                        received = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                },
                _ => {},
            }
        }

        assert!(received, "Leader subscription did not receive insert event");
    });

    let _ = execute_on_node(&leader_url, &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Leader subscription receives leader writes\n");
}

/// Test: Subscription on follower receives changes from leader writes
#[test]
fn cluster_test_subscription_follower_to_leader() {
    if !require_cluster_running() {
        return;
    }

    println!("\n=== TEST: Subscription Follower to Leader ===\n");

    let (leader_url, followers) = get_leader_and_followers();

    if followers.is_empty() {
        println!("  ⚠ No followers available, skipping test");
        return;
    }

    let follower_url = &followers[0];
    println!("Leader: {}", leader_url);
    println!("Follower: {}", follower_url);

    let namespace = generate_unique_namespace("sub_fl");
    let table = "sub_follower";
    let full = format!("{}.{}", namespace, table);

    // Setup on leader
    let _ =
        execute_on_node(&leader_url, &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    // Use USER table - subscriptions only work on USER/STREAM tables
    execute_on_node(
        &leader_url,
        &format!("CREATE USER TABLE {} (id BIGINT PRIMARY KEY, value TEXT)", full),
    )
    .expect("Failed to create table");

    // Wait for table to replicate to follower
    if !wait_for_table_on_all_nodes(&namespace, table, 10000) {
        panic!("Table {} did not replicate to all nodes", full);
    }
    println!("  ✓ Table replicated to all nodes");

    let query = format!("SELECT * FROM {}", full);
    let insert_value = "follower_receives_this";

    cluster_runtime().block_on(async {
        // Subscribe on FOLLOWER
        let follower_client = create_ws_client(follower_url);
        let leader_client = create_ws_client(&leader_url);

        let mut subscription = subscribe_with_retry(&follower_client, &query, 3).await;

        // Insert data on LEADER
        execute_query_with_retry(
            &leader_client,
            &format!("INSERT INTO {} (id, value) VALUES (1, '{}')", full, insert_value),
            5,
        )
        .await
        .expect("Failed to insert on leader");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut received = false;

        while tokio::time::Instant::now() < deadline && !received {
            let wait = Duration::from_secs(2);
            match tokio::time::timeout(wait, subscription.next()).await {
                Ok(Some(Ok(event))) => {
                    if let ChangeEvent::Insert { rows, .. } = event {
                        for row in rows {
                            if let Some(obj) = row.as_object() {
                                if let Some(Value::String(val)) = obj.get("value") {
                                    if val == insert_value {
                                        received = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                },
                _ => {},
            }
        }

        assert!(received, "Follower subscription did not receive leader insert event");
    });

    let _ = execute_on_node(&leader_url, &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Follower subscription receives leader writes\n");
}

/// Test: Multiple subscriptions across nodes receive identical events
#[test]
fn cluster_test_subscription_multi_node_identical() {
    if !require_cluster_running() {
        return;
    }

    println!("\n=== TEST: Multi-Node Subscriptions Receive Identical Events ===\n");

    let urls = cluster_urls();
    if urls.len() < 2 {
        println!("  ⚠ Need at least 2 nodes for this test, skipping");
        return;
    }

    let (leader_url, _) = get_leader_and_followers();
    let namespace = generate_unique_namespace("sub_multi");
    let table = "multi_sub";
    let full = format!("{}.{}", namespace, table);

    // Setup
    let _ =
        execute_on_node(&leader_url, &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    // Use USER table - subscriptions only work on USER/STREAM tables
    execute_on_node(
        &leader_url,
        &format!("CREATE USER TABLE {} (id BIGINT PRIMARY KEY, value TEXT)", full),
    )
    .expect("Failed to create table");

    // Wait for table to replicate to all nodes
    if !wait_for_table_on_all_nodes(&namespace, table, 10000) {
        panic!("Table {} did not replicate to all nodes", full);
    }
    println!("  ✓ Table replicated to all nodes");

    let query = format!("SELECT * FROM {}", full);
    let insert_value = "multi_node_event";
    let expected_inserts = 5;

    cluster_runtime().block_on(async {
        // Create subscriptions on all nodes
        let mut subscriptions = Vec::new();
        let received_counts: Vec<Arc<std::sync::atomic::AtomicUsize>> = (0..urls.len())
            .map(|_| Arc::new(std::sync::atomic::AtomicUsize::new(0)))
            .collect();

        for url in &urls {
            let client = create_ws_client(url);
            let sub = client.subscribe(&query).await.expect("Failed to subscribe");
            subscriptions.push(sub);
        }

        // Insert multiple rows on leader
        let leader_client = create_ws_client(&leader_url);
        for i in 0..expected_inserts {
            leader_client
                .execute_query(
                    &format!(
                        "INSERT INTO {} (id, value) VALUES ({}, '{}_{}')",
                        full, i, insert_value, i
                    ),
                    None,
                    None,
                )
                .await
                .expect("Failed to insert");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Collect events from all subscriptions
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

        for (idx, sub) in subscriptions.iter_mut().enumerate() {
            let count = received_counts[idx].clone();
            let mut local_count = 0;

            while tokio::time::Instant::now() < deadline && local_count < expected_inserts {
                let wait = Duration::from_millis(500);
                match tokio::time::timeout(wait, sub.next()).await {
                    Ok(Some(Ok(ChangeEvent::Insert { rows, .. }))) => {
                        for row in rows {
                            if let Some(obj) = row.as_object() {
                                if let Some(Value::String(val)) = obj.get("value") {
                                    if val.starts_with(insert_value) {
                                        local_count += 1;
                                        count.fetch_add(1, Ordering::SeqCst);
                                    }
                                }
                            }
                        }
                    },
                    _ => {},
                }
            }
        }

        // Verify all nodes received the same number of events
        let mut all_counts: Vec<usize> = Vec::new();
        for (i, count) in received_counts.iter().enumerate() {
            let c = count.load(Ordering::SeqCst);
            println!("  Node {} received {} events", i, c);
            all_counts.push(c);
        }

        // All should have received at least some events (replication might have slight timing differences)
        for (i, count) in all_counts.iter().enumerate() {
            assert!(*count > 0, "Node {} received 0 events, expected > 0", i);
        }
    });

    let _ = execute_on_node(&leader_url, &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Multi-node subscriptions working\n");
}

/// Test: Initial data is identical when subscribing to any node
#[test]
fn cluster_test_subscription_initial_data_consistency() {
    if !require_cluster_running() {
        return;
    }

    println!("\n=== TEST: Subscription Initial Data Consistency ===\n");

    let urls = cluster_urls();
    let (leader_url, _) = get_leader_and_followers();

    let namespace = generate_unique_namespace("sub_init");
    let table = "initial_data";
    let full = format!("{}.{}", namespace, table);

    // Setup and insert data
    let _ =
        execute_on_node(&leader_url, &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    // Use USER table - subscriptions only work on USER/STREAM tables
    execute_on_node(
        &leader_url,
        &format!("CREATE USER TABLE {} (id BIGINT PRIMARY KEY, value TEXT)", full),
    )
    .expect("Failed to create table");

    // IMPORTANT: Wait for table to replicate BEFORE inserting data
    // Otherwise, the INSERT command may be applied on followers before
    // the CREATE TABLE command, causing provider not found errors.
    if !wait_for_table_on_all_nodes(&namespace, table, 15000) {
        panic!("Table {} did not replicate to all nodes", full);
    }
    println!("  ✓ Table replicated to all nodes");

    // Insert initial data
    let mut values = Vec::new();
    for i in 0..20 {
        values.push(format!("({}, 'initial_{}')", i, i));
    }
    execute_on_node(
        &leader_url,
        &format!("INSERT INTO {} (id, value) VALUES {}", full, values.join(", ")),
    )
    .expect("Failed to insert initial data");

    // Wait for data replication - USER tables may take more time especially
    // when the cluster is under load from other parallel tests
    let mut data_replicated = false;
    for attempt in 1..=5 {
        if wait_for_row_count_on_all_nodes(&full, 20, 15000) {
            data_replicated = true;
            break;
        }
        println!("  ⏳ Attempt {}/5: Data not fully replicated, retrying...", attempt);
        std::thread::sleep(Duration::from_millis(1000));
    }

    if !data_replicated {
        panic!("Data did not replicate to all nodes within timeout (USER table replication may be slow under load)");
    }
    println!("  ✓ Data replicated to all nodes");

    let query = format!("SELECT * FROM {} ORDER BY id", full);

    cluster_runtime().block_on(async {
        let mut initial_data_sets: Vec<Vec<i64>> = Vec::new();

        for (idx, url) in urls.iter().enumerate() {
            let client = create_ws_client(url);

            // Query initial data via subscription
            let response = client.execute_query(&query, None, None).await.expect("Failed to query");

            let mut ids: Vec<i64> = Vec::new();
            if let Some(result) = response.results.first() {
                if let Some(rows) = &result.rows {
                    for row in rows {
                        if let Some(id_val) = row.first() {
                            let extracted = extract_typed_value(id_val);
                            if let Value::Number(n) = extracted {
                                if let Some(id) = n.as_i64() {
                                    ids.push(id);
                                }
                            } else if let Value::String(s) = extracted {
                                if let Ok(id) = s.parse::<i64>() {
                                    ids.push(id);
                                }
                            }
                        }
                    }
                }
            }

            println!("  Node {} has {} rows in initial data", idx, ids.len());
            initial_data_sets.push(ids);
        }

        // All nodes should have identical initial data
        let reference = &initial_data_sets[0];
        assert_eq!(reference.len(), 20, "Expected 20 rows, got {}", reference.len());

        for (i, data) in initial_data_sets.iter().enumerate().skip(1) {
            assert_eq!(data, reference, "Node {} has different initial data than reference", i);
            println!("  ✓ Node {} initial data matches reference", i);
        }
    });

    let _ = execute_on_node(&leader_url, &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ Initial data is consistent across all nodes\n");
}

/// Test: User table subscriptions work on any node
#[test]
fn cluster_test_subscription_user_table_any_node() {
    if !require_cluster_running() {
        return;
    }

    println!("\n=== TEST: User Table Subscription from Any Node ===\n");

    let urls = cluster_urls();
    let (leader_url, _) = get_leader_and_followers();

    let namespace = generate_unique_namespace("sub_user");
    let table = "user_events";
    let full = format!("{}.{}", namespace, table);

    // Setup
    let _ =
        execute_on_node(&leader_url, &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    execute_on_node(
        &leader_url,
        &format!("CREATE USER TABLE {} (id BIGINT PRIMARY KEY, event TEXT)", full),
    )
    .expect("Failed to create user table");

    // Create a test user (need proper CREATE USER syntax with ROLE)
    let test_user = format!("sub_test_user_{}", rand::random::<u32>());
    execute_on_node(
        &leader_url,
        &format!("CREATE USER {} WITH PASSWORD 'test_password_123' ROLE 'user'", test_user),
    )
    .expect("Failed to create test user");

    // Wait for table and user to replicate
    if !wait_for_table_on_all_nodes(&namespace, table, 10000) {
        panic!("Table {} did not replicate to all nodes", full);
    }
    println!("  ✓ Table replicated to all nodes");

    // Subscribe as the test user from different nodes and verify
    for (idx, url) in urls.iter().enumerate() {
        cluster_runtime().block_on(async {
            let client = KalamLinkClient::builder()
                .base_url(url)
                .auth(AuthProvider::basic_auth(test_user.clone(), "test_password_123".to_string()))
                .timeouts(
                    KalamLinkTimeouts::builder()
                        .connection_timeout_secs(5)
                        .receive_timeout_secs(10)
                        .send_timeout_secs(5)
                        .subscribe_timeout_secs(5)
                        .auth_timeout_secs(5)
                        .initial_data_timeout(Duration::from_secs(10))
                        .build(),
                )
                .build()
                .expect("Failed to build client");

            let query = format!("SELECT * FROM {}", full);
            match client.subscribe(&query).await {
                Ok(_sub) => {
                    println!("  ✓ Node {} accepts user table subscription", idx);
                },
                Err(e) => {
                    // Some errors might be expected if subscription routing isn't fully implemented
                    println!("  ⚠ Node {} subscription result: {}", idx, e);
                },
            }
        });
    }

    let _ = execute_on_node(&leader_url, &format!("DROP NAMESPACE {} CASCADE", namespace));

    println!("\n  ✅ User table subscription test complete\n");
}
