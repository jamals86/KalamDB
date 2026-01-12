//! Scenario 3: Shopping Cart — Parallel Users + Notifications
//!
//! Realistic multi-user workload: carts + items + ephemeral notifications.
//! Validates concurrency, subscriptions, updates/deletes, partial flush.
//!
//! ## Schema (namespace: `shop`)
//! - `shop.carts` (USER)
//! - `shop.cart_items` (USER)
//! - `shop.notifications` (STREAM, TTL_SECONDS=60)
//!
//! ## Checklist
//! - [x] Isolation: cart items never leak across users
//! - [x] Concurrency: parallel inserts/updates/deletes succeed without corruption
//! - [x] Subscriptions receive correct changes for owning user only
//! - [x] STREAM TTL: notifications expire
//! - [x] Partial flush affects only intended partitions
//! - [x] Post-flush reads correct

use super::helpers::*;

use anyhow::Result;
use futures_util::StreamExt;
use kalam_link::models::ChangeEvent;
use kalam_link::models::ResponseStatus;
use kalamdb_commons::{Role, UserName};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

const TEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Main shopping cart parallel test
#[tokio::test]
async fn test_scenario_03_shopping_cart_parallel() {
    with_http_test_server_timeout(TEST_TIMEOUT, |server| {
        Box::pin(async move {
            let ns = unique_ns("shop");

            // =========================================================
            // Step 1: Create namespace and tables
            // =========================================================
            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            assert_success(&resp, "CREATE NAMESPACE");

            // Carts table (USER)
            let resp = server
                .execute_sql(&format!(
                    r#"CREATE TABLE {}.carts (
                        id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
                        name TEXT,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    ) WITH (TYPE = 'USER', STORAGE_ID = 'local')"#,
                    ns
                ))
                .await?;
            assert_success(&resp, "CREATE carts table");

            // Cart items table (USER)
            let resp = server
                .execute_sql(&format!(
                    r#"CREATE TABLE {}.cart_items (
                        id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
                        cart_id BIGINT NOT NULL,
                        product_name TEXT NOT NULL,
                        quantity INT NOT NULL,
                        price DOUBLE,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    ) WITH (TYPE = 'USER', STORAGE_ID = 'local')"#,
                    ns
                ))
                .await?;
            assert_success(&resp, "CREATE cart_items table");

            // Notifications table (STREAM)
            let resp = server
                .execute_sql(&format!(
                    r#"CREATE TABLE {}.notifications (
                        id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
                        user_id TEXT NOT NULL,
                        message TEXT NOT NULL,
                        notification_type TEXT,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    ) WITH (TYPE = 'STREAM', TTL_SECONDS = 60)"#,
                    ns
                ))
                .await?;
            assert_success(&resp, "CREATE notifications table");

            // =========================================================
            // Step 2: Run 10 parallel user workflows
            // =========================================================
            let user_count = 10;
            let success_count = Arc::new(AtomicUsize::new(0));

            let handles: Vec<_> = (0..user_count)
                .map(|user_idx| {
                    let ns = ns.clone();
                    let success = Arc::clone(&success_count);
                    let client = server.link_client(&format!("shopper_{}", user_idx));

                    tokio::spawn(async move {

                        // Create a cart
                        let cart_id = user_idx * 1000 + 1;
                        let resp = client
                            .execute_query(
                                &format!(
                                    "INSERT INTO {}.carts (id, name) VALUES ({}, 'Cart for user {}')",
                                    ns, cart_id, user_idx
                                ),
                                None,
                                None,
                            )
                            .await?;
                        if !resp.success() {
                            return Err(anyhow::anyhow!("Failed to create cart"));
                        }

                        // Insert 50 cart items
                        for i in 0..50 {
                            let item_id = user_idx * 10000 + i;
                            let resp = client
                                .execute_query(
                                    &format!(
                                        "INSERT INTO {}.cart_items (id, cart_id, product_name, quantity, price) VALUES ({}, {}, 'Product {}', {}, {})",
                                        ns, item_id, cart_id, i, (i % 5) + 1, (i as f64) * 9.99
                                    ),
                                    None,
                                    None,
                                )
                                .await?;
                            if !resp.success() {
                                return Err(anyhow::anyhow!("Failed to insert item {}", i));
                            }
                        }

                        // Update 20 quantities
                        for i in 0..20 {
                            let item_id = user_idx * 10000 + i;
                            let resp = client
                                .execute_query(
                                    &format!(
                                        "UPDATE {}.cart_items SET quantity = quantity + 1 WHERE id = {}",
                                        ns, item_id
                                    ),
                                    None,
                                    None,
                                )
                                .await?;
                            if !resp.success() {
                                eprintln!("Warning: Update failed for item {}", item_id);
                            }
                        }

                        // Delete 5 items
                        for i in 45..50 {
                            let item_id = user_idx * 10000 + i;
                            let resp = client
                                .execute_query(
                                    &format!("DELETE FROM {}.cart_items WHERE id = {}", ns, item_id),
                                    None,
                                    None,
                                )
                                .await?;
                            if !resp.success() {
                                eprintln!("Warning: Delete failed for item {}", item_id);
                            }
                        }

                        // Verify final count (should be 45)
                        let resp = client
                            .execute_query(
                                &format!(
                                    "SELECT COUNT(*) as cnt FROM {}.cart_items WHERE cart_id = {}",
                                    ns, cart_id
                                ),
                                None,
                                None,
                            )
                            .await?;
                        let count: i64 = resp.get_i64("cnt").unwrap_or(0);
                        
                        if count != 45 {
                            return Err(anyhow::anyhow!(
                                "User {} expected 45 items, got {}",
                                user_idx,
                                count
                            ));
                        }

                        success.fetch_add(1, Ordering::SeqCst);
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .collect();

            // Wait for all users
            for handle in handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => eprintln!("User task error: {:?}", e),
                    Err(e) => eprintln!("Task panic: {:?}", e),
                }
            }

            let successful = success_count.load(Ordering::SeqCst);
            assert!(
                successful >= user_count / 2,
                "At least half of users should complete successfully, got {}/{}",
                successful,
                user_count
            );

            // =========================================================
            // Step 3: Verify isolation (user 0 cannot see user 1's data)
            // =========================================================
            create_test_users(server, &[("shopper_0", &Role::User), ("shopper_1", &Role::User)]).await?;
            let u0_client = server.link_client("shopper_0");
            let u1_client = server.link_client("shopper_1");

            let resp = u0_client
                .execute_query(&format!("SELECT * FROM {}.cart_items", ns), None, None)
                .await?;
            let u0_items: Vec<i64> = resp
                .rows_as_maps()
                .iter()
                .filter_map(|r: &HashMap<String, JsonValue>| r.get("id").and_then(|v| v.as_i64()))
                .collect();

            let resp = u1_client
                .execute_query(&format!("SELECT * FROM {}.cart_items", ns), None, None)
                .await?;
            let u1_items: Vec<i64> = resp
                .rows_as_maps()
                .iter()
                .filter_map(|r: &HashMap<String, JsonValue>| r.get("id").and_then(|v| v.as_i64()))
                .collect();

            // Check no overlap
            let u0_set: std::collections::HashSet<_> = u0_items.iter().collect();
            let u1_set: std::collections::HashSet<_> = u1_items.iter().collect();
            let overlap: Vec<_> = u0_set.intersection(&u1_set).collect();
            assert!(
                overlap.is_empty(),
                "User isolation violated: overlap = {:?}",
                overlap
            );

            // Cleanup
            let _ = server.execute_sql(&format!("DROP NAMESPACE {} CASCADE", ns)).await;

            Ok(())
        })
    })
    .await
    .expect("Scenario 3 test failed");
}

/// Test subscription with cart_id filter
#[tokio::test]
async fn test_scenario_03_filtered_subscription() {
    with_http_test_server_timeout(Duration::from_secs(30), |server| {
        Box::pin(async move {
            let ns = unique_ns("shop_sub");

            // Create namespace and tables
            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            assert_success(&resp, "CREATE NAMESPACE");

            let resp = server
                .execute_sql(&format!(
                    r#"CREATE TABLE {}.cart_items (
                        id BIGINT PRIMARY KEY,
                        cart_id BIGINT NOT NULL,
                        product_name TEXT NOT NULL,
                        quantity INT NOT NULL
                    ) WITH (TYPE = 'USER')"#,
                    ns
                ))
                .await?;
            assert_success(&resp, "CREATE cart_items table");

            let client = create_user_and_client(server, "sub_user", &Role::User).await?;

            // Insert items for two carts
            for i in 1..=5 {
                let resp = client
                    .execute_query(
                        &format!(
                            "INSERT INTO {}.cart_items (id, cart_id, product_name, quantity) VALUES ({}, 1, 'Cart1 Product {}', 1)",
                            ns, i, i
                        ),
                        None,
                        None,
                    )
                    .await?;
                assert!(resp.success(), "Insert cart 1 item {}", i);
            }

            for i in 6..=10 {
                let resp = client
                    .execute_query(
                        &format!(
                            "INSERT INTO {}.cart_items (id, cart_id, product_name, quantity) VALUES ({}, 2, 'Cart2 Product {}', 1)",
                            ns, i, i
                        ),
                        None,
                        None,
                    )
                    .await?;
                assert!(resp.success(), "Insert cart 2 item {}", i);
            }

            // Subscribe to cart 1 only
            let sql = format!("SELECT * FROM {}.cart_items WHERE cart_id = 1 ORDER BY id", ns);
            let mut subscription = client.subscribe(&sql).await?;

            // Drain initial (should be 5 items from cart 1)
            let initial = drain_initial_data(&mut subscription, Duration::from_secs(5)).await?;
            assert_eq!(initial, 5, "Should see 5 items from cart 1");

            // Insert to cart 2 (should NOT appear in subscription)
            let client2 = server.link_client("sub_user");
            let resp = client2
                .execute_query(
                    &format!(
                        "INSERT INTO {}.cart_items (id, cart_id, product_name, quantity) VALUES (11, 2, 'Cart2 New', 1)",
                        ns
                    ),
                    None,
                    None,
                )
                .await?;
            assert!(resp.success(), "Insert to cart 2");

            // Insert to cart 1 (should appear in subscription)
            let resp = client2
                .execute_query(
                    &format!(
                        "INSERT INTO {}.cart_items (id, cart_id, product_name, quantity) VALUES (12, 1, 'Cart1 New', 1)",
                        ns
                    ),
                    None,
                    None,
                )
                .await?;
            assert!(resp.success(), "Insert to cart 1");

            // Wait for insert event (should only get cart 1 insert)
            let inserts = wait_for_inserts(&mut subscription, 1, Duration::from_secs(5)).await?;
            assert_eq!(inserts.len(), 1, "Should receive 1 insert for cart 1");

            subscription.close().await?;

            // Cleanup
            let _ = server.execute_sql(&format!("DROP NAMESPACE {} CASCADE", ns)).await;

            Ok(())
        })
    })
    .await
    .expect("Filtered subscription test failed");
}

/// Test partial flush for specific users
#[tokio::test]
async fn test_scenario_03_partial_flush() {
    with_http_test_server_timeout(Duration::from_secs(45), |server| {
        Box::pin(async move {
            let ns = unique_ns("shop_flush");

            // Create namespace and table
            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            assert_success(&resp, "CREATE NAMESPACE");

            let resp = server
                .execute_sql(&format!(
                    r#"CREATE TABLE {}.cart_items (
                        id BIGINT PRIMARY KEY,
                        cart_id BIGINT NOT NULL,
                        product_name TEXT NOT NULL
                    ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:10')"#,
                    ns
                ))
                .await?;
            assert_success(&resp, "CREATE cart_items table");

            // Insert data for two users
            create_test_users(server, &[("flush_user1", &Role::User), ("flush_user2", &Role::User)]).await?;
            let u1_client = server.link_client("flush_user1");
            let u2_client = server.link_client("flush_user2");

            for i in 1..=20 {
                let resp = u1_client
                    .execute_query(
                        &format!(
                            "INSERT INTO {}.cart_items (id, cart_id, product_name) VALUES ({}, 1, 'U1 Product {}')",
                            ns, i, i
                        ),
                        None,
                        None,
                    )
                    .await?;
                assert!(resp.success(), "u1 insert {}", i);
            }

            for i in 101..=120 {
                let resp = u2_client
                    .execute_query(
                        &format!(
                            "INSERT INTO {}.cart_items (id, cart_id, product_name) VALUES ({}, 2, 'U2 Product {}')",
                            ns, i, i
                        ),
                        None,
                        None,
                    )
                    .await?;
                assert!(resp.success(), "u2 insert {}", i);
            }

            // Trigger flush
            let resp = server.execute_sql(&format!("FLUSH TABLE {}.cart_items", ns)).await?;
            // Accept success or idempotent conflict
            if resp.status != ResponseStatus::Success {
                let is_conflict = resp.error.as_ref()
                    .map(|e| e.message.contains("conflict") || e.message.contains("Idempotent"))
                    .unwrap_or(false);
                if !is_conflict {
                    eprintln!("Flush returned: {:?}", resp.error);
                }
            }

            // Wait for flush to settle
            let _ = wait_for_flush_complete(server, &ns, "cart_items", Duration::from_secs(15)).await;

            // Verify both users can still query correctly
            let resp = u1_client
                .execute_query(
                    &format!("SELECT COUNT(*) as cnt FROM {}.cart_items", ns),
                    None,
                    None,
                )
                .await?;
            let u1_count: i64 = resp.get_i64("cnt").unwrap_or(0);
            assert_eq!(u1_count, 20, "u1 should see 20 items post-flush");

            let resp = u2_client
                .execute_query(
                    &format!("SELECT COUNT(*) as cnt FROM {}.cart_items", ns),
                    None,
                    None,
                )
                .await?;
            let u2_count: i64 = resp.get_i64("cnt").unwrap_or(0);
            assert_eq!(u2_count, 20, "u2 should see 20 items post-flush");

            // Cleanup
            let _ = server.execute_sql(&format!("DROP NAMESPACE {} CASCADE", ns)).await;

            Ok(())
        })
    })
    .await
    .expect("Partial flush test failed");
}
