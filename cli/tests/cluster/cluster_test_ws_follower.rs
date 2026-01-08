//! Cluster WebSocket follower tests
//!
//! Verifies followers can deliver live query updates from leader writes.

use crate::cluster_common::*;
use crate::common::*;
use kalam_link::{AuthProvider, KalamLinkClient, KalamLinkTimeouts, ChangeEvent};
use serde_json::Value;
use std::time::Duration;

fn parse_cluster_nodes() -> (String, String) {
    let urls = cluster_urls();
    let response = execute_on_node_response(
        &urls[0],
        "SELECT node_id, api_addr, is_leader FROM system.cluster",
    )
    .expect("Failed to query system.cluster");

    let result = response
        .results
        .first()
        .expect("Missing cluster result");
    let rows = result.rows.as_ref().expect("Missing cluster rows");

    let mut leader_url: Option<String> = None;
    let mut follower_url: Option<String> = None;

    for row in rows {
        if row.len() < 3 {
            continue;
        }

        let api_addr = extract_typed_value(&row[1]);
        let is_leader = extract_typed_value(&row[2]);

        let api_addr = match api_addr {
            Value::String(s) => s,
            other => other.to_string(),
        };

        let leader = matches!(is_leader, Value::Bool(true))
            || matches!(is_leader, Value::String(ref s) if s == "true");

        if leader {
            leader_url = Some(api_addr);
        } else if follower_url.is_none() {
            follower_url = Some(api_addr);
        }
    }

    let leader_url = leader_url.expect("Leader URL not found");
    let follower_url = follower_url.expect("Follower URL not found");

    (leader_url, follower_url)
}

fn create_ws_client(base_url: &str) -> KalamLinkClient {
    KalamLinkClient::builder()
        .base_url(base_url)
        .auth(AuthProvider::basic_auth(
            "root".to_string(),
            root_password().to_string(),
        ))
        .timeouts(
            KalamLinkTimeouts::builder()
                .connection_timeout_secs(5)
                .receive_timeout_secs(30)
                .send_timeout_secs(10)
                .subscribe_timeout_secs(10)
                .auth_timeout_secs(10)
                .initial_data_timeout(Duration::from_secs(30))
                .build(),
        )
        .build()
        .expect("Failed to build cluster client")
}

#[test]
fn cluster_test_ws_follower_receives_leader_changes() {
    require_cluster_running();

    println!("\n=== TEST: WebSocket Follower Receives Leader Changes ===\n");

    let (leader_url, follower_url) = parse_cluster_nodes();
    println!("Leader: {}", leader_url);
    println!("Follower: {}", follower_url);

    let namespace = generate_unique_namespace("cluster_ws");
    let table = "ws_follow";
    let full = format!("{}.{}", namespace, table);

    let _ = execute_on_node(
        &leader_url,
        &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace),
    );
    execute_on_node(&leader_url, &format!("CREATE NAMESPACE {}", namespace))
        .expect("Failed to create namespace");
    // Use USER TABLE - subscriptions only work on USER/STREAM tables
    execute_on_node(
        &leader_url,
        &format!(
            "CREATE USER TABLE {} (id BIGINT PRIMARY KEY, value TEXT)",
            full
        ),
    )
    .expect("Failed to create table");

    // Wait for table to replicate to all nodes including follower
    if !wait_for_table_on_all_nodes(&namespace, table, 10000) {
        panic!("Table {} did not replicate to all nodes within timeout", full);
    }
    println!("  ✓ Table replicated to all nodes");

    let query = format!("SELECT * FROM {}", full);
    let insert_value = "follower_event";
    let insert_sql = format!(
        "INSERT INTO {} (id, value) VALUES (1, '{}')",
        full, insert_value
    );

    cluster_runtime()
        .block_on(async {
            let follower_client = create_ws_client(&follower_url);
            let leader_client = create_ws_client(&leader_url);

            let mut subscription = follower_client
                .subscribe(&query)
                .await
                .expect("Failed to subscribe on follower");

            leader_client
                .execute_query(&insert_sql, None, None)
                .await
                .expect("Failed to insert on leader");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
            let mut received = false;

            while tokio::time::Instant::now() < deadline {
                let remaining = deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .unwrap_or_default();
                let wait = std::cmp::min(remaining, Duration::from_secs(2));

                match tokio::time::timeout(wait, subscription.next()).await {
                    Ok(Some(Ok(event))) => {
                        match event {
                            ChangeEvent::Insert { rows, .. } => {
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
                            ChangeEvent::Error { code, message, .. } => {
                                panic!("Subscription error: {} - {}", code, message);
                            }
                            _ => {}
                        }
                    }
                    Ok(Some(Err(err))) => panic!("Subscription error: {}", err),
                    Ok(None) => break,
                    Err(_) => {}
                }

                if received {
                    break;
                }
            }

            assert!(
                received,
                "Follower did not receive leader insert event via WebSocket"
            );
        });

    let _ = execute_on_node(
        &leader_url,
        &format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace),
    );

    println!("\n  ✅ WebSocket follower delivery test passed\n");
}
